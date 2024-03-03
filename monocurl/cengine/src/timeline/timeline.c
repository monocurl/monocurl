//
//  timeline.c
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>

#include "callback.h"
#include "mc_memory.h"
#include "timeline+export.h"
#include "timeline+simulate.h"
#include "timeline.h"

#define MC_LOG_TAG "timeline"
#include "mc_log.h"

static inline void
pre_write(struct timeline *timeline)
{
    mc_rwlock_writer_lock(timeline->state_lock);
}

static inline void
post_write(struct timeline *timeline, mc_bool_t flush)
{
    if (flush) {
        timeline_flush(timeline);
    }
    mc_rwlock_writer_unlock(timeline->state_lock);
}

static void *
loop(void *data)
{
    // initially, there might be a seek command which will execute before thread
    // startup
    struct timeline *const timeline = data;

    /*
     * This is actually
     * a really intricate bug (?)
     * but from my understanding, it seems like what is essentially happening is
     * that whenever multiple threads go from native to managed, there is an
     * interrupt inserted by the debugger (?) int 3 iirc (well the interrupt is
     * there even if just 1 but the problem arises with multiple). Anyways, for
     * some reason, every now and then Visual Studio seems to not handle the
     * interrupt properly and then the entire program hangs. My evidence is that
     * not only is the program hanging, but I'm not even able to "Break all",
     * another indication that Visual Studio is at fault. Moreover, memory dump
     * shows that one thread is waiting on another thread (typically main
     * waiting for Timeline Worker), but timeline worker itself doesn't seem to
     * be waiting on anyone, and from the assembly it is just stuck on int 3,
     * which again seems to indicate Visual Studio is not handling this
     * properly. For solution, I'm just commenting out this line on windows
     * because then we dont have multiple conflicts. Of course, there are other
     * contentions possible, but this was the main one.
     *
     * Thankfully, this seems to be a debugger related issue so it should not be
     * seen in production.
     */
#if !(MC_ENV_OS & MC_ENV_OS_WINDOWS)
    mc_logn("start loop", "", timeline);
#endif

    mc_mutex_lock(timeline->q_lock);

    for (;;) {

        mc_rwlock_reader_lock(timeline->state_lock);
        mc_count_t const count = timeline->tasks->count;
        mc_rwlock_reader_unlock(timeline->state_lock);

        if (!count) {
            mc_cond_variable_wait(timeline->has_q, timeline->q_lock);
        }

        mc_rwlock_reader_lock(timeline->state_lock);

        while (timeline->tasks->count) {
            enum timeline_task_type const task = thread_q_peek(timeline->tasks);
            mc_rwlock_reader_unlock(timeline->state_lock);

            switch (task) {
            case TIMELINE_NO_TASK:
                // technically log statements take reading lock...
                mc_logn("null task", "", timeline);
                break;
            case TIMELINE_SEEK:
                //                    Too much information...
                // #if (MC_LOGGING)
                //                    mc_rwlock_reader_lock(timeline->state_lock);
                //                    mc_logn("seek", " at: %02zu:%.3f",
                //                    timeline, timeline->timestamp.slide,
                //                    timeline->timestamp.offset);
                //                    mc_rwlock_reader_unlock(timeline->state_lock);
                // #endif
                timeline->timestamp = timeline->seekstamp;
                timeline_really_seek_to(timeline);
                break;
            case TIMELINE_PLAY:
#if (MC_LOGGING)
                mc_rwlock_reader_lock(timeline->state_lock);
                mc_logn(
                    "play", " at: %02zu:%.3f", timeline,
                    timeline->timestamp.slide, timeline->timestamp.offset
                );
                mc_rwlock_reader_unlock(timeline->state_lock);
#endif
                timeline->timestamp = timeline->seekstamp;
                timeline_play(timeline);
                break;
            case TIMELINE_EXPORT:
#if (MC_LOGGING)
                mc_rwlock_reader_lock(timeline->state_lock);
                struct timeline_export_mode e = timeline->export_mode;
                mc_logn(
                    "export", " w: %u h: %u fps: %u upf: %u out: '%s'",
                    timeline, e.w, e.h, e.fps, e.upf, e.out_path
                );
                mc_rwlock_reader_unlock(timeline->state_lock);
#endif
                timeline_export(timeline);
                break;
            case TIMELINE_TERMINATE:
                mc_cond_variable_signal(timeline->has_q);
                mc_mutex_unlock(timeline->q_lock);
                goto exit_loop;
            }

            pre_write(timeline);
            thread_q_poll(timeline->tasks);
            post_write(timeline, 1);

            mc_rwlock_reader_lock(timeline->state_lock);
        }
        mc_rwlock_reader_unlock(timeline->state_lock);
    }

exit_loop:
    mc_logn("end thread", "", timeline);
    return NULL;
}

struct timeline *
timeline_init(struct scene_handle *handle)
{
    struct timeline *const timeline = mc_calloc(1, sizeof(struct timeline));

    timeline->handle = handle;

    timeline->tasks = thread_q_init(0);

    timeline->state_lock = mc_rwlock_init();
    timeline->has_q = mc_cond_variable_init();
    timeline->q_lock = mc_mutex_init();
    timeline->thread = mc_thread_init(loop, timeline, 1, "Timeline Worker");

    timeline->executor = timeline_executor_init(timeline);

    mc_logn("init", "", timeline);

    return timeline;
}

// called with state_lock
static inline void
task(struct timeline *timeline, enum timeline_task_type task, mc_bool_t flush)
{
    thread_q_push(timeline->tasks, task);

    // signal iff it's not already reading
    if (timeline->tasks->count == 1) {
        post_write(timeline, flush);

        mc_mutex_lock(timeline->q_lock);
        mc_cond_variable_signal(timeline->has_q);
        mc_mutex_unlock(timeline->q_lock);

        return;
    }

    post_write(timeline, flush);
}

void
timeline_seek_to(
    struct timeline *timeline, struct timestamp timestamp, mc_bool_t lock
)
{
    if (lock) {
        pre_write(timeline);
    }

    if (timeline->tasks->count &&
        thread_q_peek(timeline->tasks) == TIMELINE_EXPORT) {
        post_write(timeline, 0);
        return;
    }

    timeline->seekstamp = timeline->timestamp = timestamp;
    task(timeline, TIMELINE_SEEK, 1);
}

void
timeline_play_toggle(struct timeline *timeline)
{
    pre_write(timeline);
    if (timeline->tasks->count &&
        thread_q_peek(timeline->tasks) == TIMELINE_EXPORT) {
        post_write(timeline, 1);
        return;
    }

    if ((timeline->is_playing = !timeline->is_playing)) {
        task(timeline, TIMELINE_PLAY, 1);
    }
    else {
        mc_logn(
            "stop play", " at: %02zu:%.3f", timeline, timeline->timestamp.slide,
            timeline->timestamp.offset
        );
        post_write(timeline, 1);
    }
}

void
timeline_toggle_presentation_mode(struct timeline *timeline)
{
    pre_write(timeline);

    timeline->in_presentation_mode ^= 1;
    timeline->is_playing = 0;

    post_write(timeline, 1);
}

void
timeline_start_export(
    struct timeline *timeline, char const *out_path, unsigned int w,
    unsigned int h, unsigned int fps, unsigned int upf
)
{
    pre_write(timeline);

    timeline->export_mode = (struct timeline_export_mode){
        .w = w,
        .h = h,
        .fps = fps,
        .upf = upf,
        .out_path = out_path,
        .frame_buffer = NULL // initialized once frames are actually dealt
    };
    task(timeline, TIMELINE_EXPORT, 1);
}

void
timeline_interrupt_export(struct timeline *timeline)
{
    pre_write(timeline);

    task(timeline, TIMELINE_NO_TASK, 1);
}

// synchronization is guaranteed (by virtue that during exporting nothing else
// can be done...) then, no locks needed
void
timeline_write_frame(struct timeline *timeline, uint8_t *byte_data)
{
    // here, this means you can simulate next step
    timeline->export_mode.frame_buffer = byte_data;
    mc_cond_variable_signal(timeline->has_q);
}

void
timeline_read_lock(struct timeline *timeline)
{
    mc_rwlock_reader_lock(timeline->state_lock);
}

void
timeline_read_unlock(struct timeline *timeline)
{
    mc_rwlock_reader_unlock(timeline->state_lock);
}

void
timeline_free(struct timeline *timeline)
{
    // end thread...
    pre_write(timeline);

    if (timeline->is_playing) {
        timeline->is_playing = 0;
    }

    task(timeline, TIMELINE_TERMINATE, 0); // calls post write for us

    mc_thread_join(timeline->thread);

    mc_rwlock_free(timeline->state_lock);
    mc_cond_variable_free(timeline->has_q);
    mc_mutex_free(timeline->q_lock);

    thread_q_free(timeline->tasks);

    if (timeline->executor) {
        timeline_executor_free(timeline->executor);
    }

    mc_logn("free", "", timeline);

    mc_free(timeline);
}
