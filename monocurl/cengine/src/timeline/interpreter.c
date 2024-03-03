//
//  interpreter.c
//  Monocurl
//
//  Created by Manu Bhat on 10/23/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "callback.h"
#include "interpreter.h"
#include "timeline.h"

static void
do_scene(struct raw_scene_model *scene, struct raw_slide_model const *start)
{
    mc_bool_t encountered = start == NULL;

    for (mc_ind_t i = 0; i < scene->slide_count; i++) {
        if (!encountered && scene->slides[i] == start) {
            encountered = 1;
        }

        if (encountered) {
            slide_write_error(scene->slides[i], (struct slide_error){ 0 }, 1);
        }
    }
}

void
interpreter_slide(
    struct raw_slide_model *slide, mc_bool_t maintain_tree_invariants
)
{
    // invalidate all subsequent caches;
    // previous caches should remain unchanged
    struct timeline *timeline;
    if (slide->scene->handle && (timeline = slide->scene->handle->timeline)) {
        mc_rwlock_writer_lock(timeline->state_lock);

        do_scene(slide->scene, slide);
        timeline_executor_invalidate(
            timeline->executor, slide, maintain_tree_invariants
        );

        if (slide->scene->handle) {
            struct viewport *const viewport = slide->scene->handle->viewport;
            viewport_set_state(
                viewport,
                timeline->executor->state == TIMELINE_EXECUTOR_STATE_IDLE
                    ? VIEWPORT_IDLE
                    : VIEWPORT_COMPILER_ERROR
            );
        }

        mc_rwlock_reader_unlock(slide->scene->lock);
        /* handles timeline locking */
        timeline_seek_to(timeline, timeline->seekstamp, 0);
        mc_rwlock_reader_lock(slide->scene->lock);
    }
}

void
interpreter_scene(
    struct raw_scene_model *scene, mc_bool_t maintain_tree_invariants,
    mc_bool_t reseek
)
{
    struct timeline *timeline;

    do_scene(scene, NULL);

    if (scene->handle && (timeline = scene->handle->timeline)) {
        mc_rwlock_writer_lock(timeline->state_lock);

        timeline_executor_resize(
            timeline->executor, scene, maintain_tree_invariants
        );

        if (reseek) {
            // will free lock for us
            timeline_seek_to(timeline, timeline->seekstamp, 0);
        }
        else {
            mc_rwlock_writer_unlock(timeline->state_lock);
        }
    }
}
