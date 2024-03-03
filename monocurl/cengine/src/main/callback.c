//
//  callback.c
//  Monocurl
//
//  Created by Manu Bhat on 10/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>

#include "callback.h"
#include "file_manager.h"
#include "interpreter.h"

#if MC_DEBUG
#define MC_CHECKPOINT_INTERVAL_SECONDS (-1)
#else
#define MC_CHECKPOINT_INTERVAL_SECONDS 60
#endif

static mc_count_t stack;

static struct raw_slide_model *slide;
static struct raw_scene_model *scene;

void
pre_modify(struct raw_scene_model *s)
{
    // increase mod count
    if (!stack++ && s && s->lock) {
        mc_rwlock_writer_lock(s->lock);
    }
}

#if !(MC_ENV_OS & MC_ENV_OS_WINDOWS)
void (*aux_entry_flush)(struct aux_entry_model *reference, mc_bool_t is_global);
void (*aux_group_flush)(struct aux_group_model *reference, mc_bool_t is_global);
void (*slide_flush)(struct raw_slide_model *reference, mc_bool_t is_global);
void (*scene_flush)(struct raw_scene_model *reference, mc_bool_t is_global);

void (*viewport_flush)(struct viewport *reference);
void (*timeline_flush)(struct timeline *reference);

mc_handle_t (*poll_texture)(char const *path);
void (*free_buffer)(mc_handle_t handle);

// exporting
void (*export_frame)(struct timeline const *timeline);
void (*export_finish)(struct timeline const *timeline, char const *error);

// for media

// expected to return allocated string
char const *(*path_translation)(char const *handle);

// expected to return string literals (should not be freed)
char const *(*std_lib_path)(void);
char const *(*default_scene_path)(void);
char const *(*tex_binary_path)(void);
char const *(*tex_intermediate_path)(void);
#endif

static void
frontend_flush(void)
{
    // flush cached content
    if (slide) {
        slide_flush(slide, 1);
    }
    else if (scene) {
        scene_flush(scene, 1);
    }
}

static void
maintain_tree_invariants(mc_bool_t modify)
{
    // relock
    ++stack;

    // essentally write compiler warnings, and if modify is allowed, modify as
    // well applied to entire slide
    if (slide) {
        interpreter_slide(slide, modify);
    }
    else if (scene) {
        interpreter_scene(scene, modify, 1);
        for (mc_ind_t i = 0; i < scene->slide_count; ++i) {
            scene->slides[i]->scene_modify_safe = 0;
        }
    }

    --stack;
}

static void
post_modify(mc_bool_t modify_invariants_if_needed)
{
    if (!--stack) {
        maintain_tree_invariants(modify_invariants_if_needed);

        // write data to frontend flush
        frontend_flush();

        struct raw_scene_model *scn;
        if (slide) {
            scn = slide->scene;
        }
        else {
            scn = scene;
        }
        mc_rwlock_writer_unlock(scn->lock);

        // save if necessary, relocks
        mc_timestamp_t const now = mc_timestamp_now();
        if (mc_timeinterval_to_seconds(
                mc_timediff(now, scn->handle->last_auto_save)
            ) > MC_CHECKPOINT_INTERVAL_SECONDS) {
            file_write_model(scn->handle);
            scn->handle->last_auto_save = now;
        }

        // clear everything
        slide = NULL;
        scene = NULL;
    }
}

static void
slide_slide(struct raw_slide_model *s, struct raw_slide_model *s2)
{
    if (s != s2) {
        slide = NULL;
        scene = s->scene;
    }
}

static void
up_scene(struct raw_scene_model *scene_)
{
    scene_->dirty = 1;
}

static void
up_slide(struct raw_slide_model *slide_)
{
    if (!slide_->dirty) {
        slide_->dirty = 1;
        up_scene(slide_->scene);
    }
}

void
post_slide_modify(struct raw_slide_model *reference, mc_bool_t is_global)
{
    if (scene)
        ;
    else if (slide) {
        slide_slide(reference, slide);
    }
    else {
        slide = reference;
    }

    up_slide(reference);

    post_modify(1);
}

void
post_scene_modify(struct raw_scene_model *reference, mc_bool_t is_global)
{
    slide = NULL;
    scene = reference;

    up_scene(reference);

    post_modify(1);
}

void
post_history_modify(void)
{
    post_modify(0);
}

void
post_filewriter_modify(void)
{
    slide = NULL;
    scene = NULL;

    stack = 0;
}
