//
//  timeline+simulate.c
//  monocurl
//
//  Created by Manu Bhat on 11/30/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>

#include "callback.h"
#include "mc_threading.h"
#include "mc_time_util.h"
#include "tetramesh.h"
#include "timeline+simulate.h"
#include "viewport.h"

#define MC_LOG_TAG "timeline+simulate"
#include "mc_log.h"

#define PLAY_TARGET_FPS 60
// essentially how much to simulate when seeking, and updates are required
#define SEEK_TARGET_FPS 7

mc_status_t
timeline_slide_startup(
    struct timeline *timeline, mc_ind_t index, mc_bool_t context_switch
)
{
    if (timeline->tasks->count > 1) {
        return MC_STATUS_FAIL;
    }

    return timeline_executor_startup(timeline->executor, index, context_switch);
}

mc_ternary_status_t
timeline_step(struct timeline *timeline, double dt)
{
    // more tasks, which means we need to exit immediately and move on to next
    // one checked in several areas to ensure nothing doesn't hang
    if (timeline->tasks->count > 1) {
        return MC_TERNARY_STATUS_FAIL;
    }

    timeline->timestamp.offset += dt;

    return timeline_executor_step(timeline->executor, dt);
}

mc_status_t
timeline_blit_trailing_cache(struct timeline *timeline)
{
    return timeline_executor_blit_cache(timeline->executor);
}

static void
timeline_visual_flush(struct timeline *timeline)
{
    timeline_flush(timeline);

    mc_count_t mesh_c;
    struct tetramesh **const meshes =
        timeline_meshes(timeline->executor, &mesh_c);
    struct viewport_camera const camera = timeline_camera(timeline->executor);
    struct vec4 const background = timeline_background(timeline->executor);

    viewport_set_unordered_mesh(
        timeline->handle->viewport, background, camera, meshes, mesh_c
    );
}

mc_ternary_status_t
timeline_frame(struct timeline *timeline, double dt, unsigned int upf)
{
    mc_ternary_status_t ret = 0;

    for (mc_ind_t i = 0; i < upf; ++i) {
        ret = timeline_step(timeline, dt);
        if (ret == MC_TERNARY_STATUS_FAIL) {
            timeline_flush(timeline);
            return MC_TERNARY_STATUS_FAIL;
        }
        else if (ret == MC_TERNARY_STATUS_FINISH) {
            break;
        }
    }

    // flush viewport and timeline
    timeline->seekstamp = timeline->timestamp;
    timeline_visual_flush(timeline);

    return ret;
}

void
timeline_play(struct timeline *timeline)
{

    // get to first position
    if (timeline_really_seek_to(timeline) != MC_STATUS_SUCCESS) {
        return;
    }

    mc_rwlock_reader_lock(timeline->state_lock);

    struct raw_scene_model *scene = timeline->handle->model;
    struct viewport *viewport = timeline->handle->viewport;

    mc_rwlock_reader_unlock(timeline->state_lock);

    viewport_set_state(viewport, VIEWPORT_PLAYING);

    mc_ind_t curr;
    mc_timestamp_t last_time = mc_timestamp_now();

    // done alongside main thread with sleeps
    for (;;) {
        mc_timestamp_t curr_time = mc_timestamp_now();
        double const dt =
            (double) mc_timeinterval_to_millis(mc_timediff(curr_time, last_time)
            ) /
            1000.0;
        last_time = curr_time;

        mc_rwlock_writer_lock(timeline->state_lock);

        mc_ternary_status_t const error = timeline_frame(timeline, dt, 1);
        mc_bool_t finished = 0;

        if (error == MC_TERNARY_STATUS_FAIL) {
            // actual error (or interrupt)
            mc_log_errorn("play interrupt", " errno: %d", timeline, error);
            timeline->is_playing = 0;
            finished = 1;
            timeline_flush(timeline);
            viewport_set_state(viewport, VIEWPORT_RUNTIME_ERROR);
        }
        else if (error == MC_TERNARY_STATUS_FINISH) {
            if (timeline->timestamp.slide < scene->slide_count - 1) {
                // error generally indicates interrupt of some sort
                if (timeline_blit_trailing_cache(timeline) !=
                    MC_STATUS_SUCCESS) {
                    timeline->is_playing = 0;
                    finished = 1;
                    timeline_flush(timeline);
                }
                else {
                    ++timeline->timestamp.slide;
                    timeline->timestamp.offset = 0;
                    curr = timeline->timestamp.slide;
                    viewport_set_state(viewport, VIEWPORT_LOADING);

                    if (timeline->in_presentation_mode) {
                        viewport_set_state(viewport, VIEWPORT_IDLE);
                        timeline->is_playing = 0;
                        finished = 1;
                    }
                    else if (timeline_slide_startup(timeline, curr, 1) != MC_STATUS_SUCCESS) {
                        viewport_set_state(viewport, VIEWPORT_RUNTIME_ERROR);
                        timeline->is_playing = 0;
                        finished = 1;
                    }
                    else {
                        viewport_set_state(viewport, VIEWPORT_PLAYING);
                        last_time = mc_timestamp_now();
                    }
                }
            }
            else {
                viewport_set_state(viewport, VIEWPORT_IDLE);
                timeline->is_playing = 0;
                finished = 1;
                mc_logn(
                    "play finish", " at: %02zu:%.3f", timeline,
                    timeline->timestamp.slide, timeline->timestamp.offset
                );
            }

            timeline->seekstamp = timeline->timestamp;
            timeline_visual_flush(timeline);
        }
        else {
            timeline->seekstamp = timeline->timestamp;
            timeline_visual_flush(timeline);
        }

        if (!finished && !timeline->is_playing) {
            viewport_set_state(viewport, VIEWPORT_IDLE);
            finished = 1;
        }

        mc_rwlock_writer_unlock(timeline->state_lock);

        if (!finished) {
            curr_time = mc_timestamp_now();
            long long const elapsed =
                mc_timeinterval_to_millis(mc_timediff(curr_time, last_time));

            double sleep_seconds =
                1.0 / PLAY_TARGET_FPS - (double) elapsed / 1e6;
            if (sleep_seconds > 0) {
                mc_thread_wait(mc_timeinterval_from_millis(
                    (long long) (sleep_seconds * 1e3)
                ));
            }
        }
        else {
            return;
        }
    }
}

static mc_status_t
timeline_skip_over_seconds(
    struct timeline *timeline, mc_ind_t slide, double delta
)
{
    // simulates until then, but we might be able to index directly...
#pragma message("TODO see when it actually is a context switch")
    if (timeline_slide_startup(timeline, slide, 1) == MC_STATUS_FAIL) {
        timeline->is_playing = 0;
        mc_rwlock_writer_unlock(timeline->state_lock);
        return MC_STATUS_FAIL;
    }

    // release and reacquire lock
    if (timeline_executor_check_interrupt(timeline->executor, 1)) {
        mc_rwlock_writer_unlock(timeline->state_lock);
        return MC_STATUS_FAIL;
    }

    double elapsed = 0;

    for (;;) {

        double dt = 1.0 / SEEK_TARGET_FPS;
        if (elapsed + dt > delta) {
            dt = delta - elapsed;
        }
        mc_ternary_status_t const error = timeline_step(timeline, dt);
        elapsed += dt;

        if (error == MC_TERNARY_STATUS_FAIL) {
            // actual error (or interrupt)
            mc_log_errorn(
                "seek interrupt", " errno: %d seconds %f", timeline, error,
                timeline->timestamp.offset
            );
            timeline->is_playing = 0;
            mc_rwlock_writer_unlock(timeline->state_lock);
            return MC_STATUS_FAIL;
        }
        else if (error == MC_TERNARY_STATUS_FINISH) {
            // finished frame
            mc_status_t ret = timeline_blit_trailing_cache(timeline);
            mc_rwlock_writer_unlock(timeline->state_lock);
            return ret;
        }
        else if (elapsed >= delta) {
            mc_rwlock_writer_unlock(timeline->state_lock);
            return MC_STATUS_SUCCESS; // ignore unlock...
        }

        if (timeline_executor_check_interrupt(timeline->executor, 1)) {
            mc_rwlock_writer_unlock(timeline->state_lock);
            return MC_STATUS_FAIL;
        }
    }
}

static mc_status_t
timeline_skip_over_slide(struct timeline *timeline, mc_ind_t slide)
{
    // simulates entire slide
    struct timeline_execution_context *executor = timeline->executor;
    if (executor->slides[slide + 1].trailing_valid) {
        mc_rwlock_writer_unlock(timeline->state_lock);
        return MC_STATUS_SUCCESS;
    }

    return timeline_skip_over_seconds(timeline, slide, FLT_MAX);
}

mc_status_t
timeline_really_seek_to(struct timeline *timeline)
{
    // released at various check points
    mc_rwlock_writer_lock(timeline->state_lock);

    struct viewport *viewport = timeline->handle->viewport;

    viewport_set_state(viewport, VIEWPORT_LOADING);

    struct timestamp target = timeline->seekstamp;

    // offset clipped later
    if (target.slide < 1) { /* 1 instead of zero because of config slide*/
        target.slide = 1;
        target.offset = 0;
    }
    else if (target.slide >= timeline->executor->slide_count - 1) {
        target.slide = timeline->executor->slide_count - 2;
        target.offset = FLT_MAX;
    }

    timeline->timestamp.slide = 0;
    timeline->timestamp.offset = 0;

    // prepopulate leading caches
    mc_status_t ret = MC_STATUS_SUCCESS;
    mc_ind_t i;
    for (i = 0; i < target.slide; ++i) {
        if ((ret = timeline_skip_over_slide(timeline, i)) == MC_STATUS_FAIL) {
            break;
        }

        mc_rwlock_writer_lock(timeline->state_lock);

        timeline->timestamp.slide = i + 1;
        timeline->timestamp.offset = 0;
    }

    // go as far as possible
    if (ret == MC_STATUS_SUCCESS) {
        ret = timeline_skip_over_seconds(timeline, i, target.offset);
    }

    if (ret == MC_STATUS_SUCCESS) {
        viewport_set_state(viewport, VIEWPORT_IDLE);

        mc_rwlock_writer_lock(timeline->state_lock);
        timeline->seekstamp = timeline->timestamp;
        timeline_visual_flush(timeline);
        mc_rwlock_writer_unlock(timeline->state_lock);
    }
    else {
        viewport_set_state(viewport, VIEWPORT_RUNTIME_ERROR);
    }

    return ret;
}
