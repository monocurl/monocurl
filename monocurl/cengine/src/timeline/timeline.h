//
//  timeline.h
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdint.h>
#include <stdio.h>

#include "mc_env.h"
#include "scene_handle.h"
#include "thread_queue.h"
#include "timeline_execution_context.h"

// timeline also responsible for memory management

struct timestamp {
    mc_ind_t slide;
    double offset;
};

struct timeline {
    struct scene_handle *handle;

    struct timestamp timestamp;
    struct timestamp seekstamp;

    struct timeline_execution_context *executor;

    mc_bool_t is_playing;

#pragma message("ORGANIZATION: should this belong here?")
    mc_bool_t in_presentation_mode;

    mc_thread_t *thread;
    mc_rwlock_t
        *state_lock; // can't have other threads reading this while we write
    mc_cond_variable_t *has_q; // makes sure previous frame has completed before
                               // exporting next frame
    mc_mutex_t *q_lock;

    // only valid if task contains TIMELINE_EXPORT
    struct timeline_export_mode {
        unsigned int w, h, fps, upf;
        char const *out_path;
        uint8_t *frame_buffer;
    } export_mode;

    struct thread_q *tasks;
};

#if MC_INTERNAL
struct timeline *
timeline_init(struct scene_handle *handle);
void
timeline_free(struct timeline *timeline);
#endif

void
timeline_seek_to(
    struct timeline *timeline, struct timestamp timestamp, mc_bool_t lock
);

void
timeline_play_toggle(struct timeline *timeline);

void
timeline_toggle_presentation_mode(struct timeline *timeline);

// export...
// upf is updates per frame
void
timeline_start_export(
    struct timeline *timeline, char const *out_path, unsigned int w,
    unsigned int h, unsigned int fps, unsigned int upf
);
void
timeline_interrupt_export(struct timeline *timeline);
void
timeline_write_frame(struct timeline *timeline, uint8_t *byte_data);

void
timeline_read_lock(struct timeline *timeline);

void
timeline_read_unlock(struct timeline *timeline);
