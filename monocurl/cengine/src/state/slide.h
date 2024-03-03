//
//  slide.h
//  Monocurl
//
//  Created by Manu Bhat on 10/21/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <limits.h>
#include <stdio.h>
#include <stdlib.h>

#include "mc_env.h"
#include "mc_types.h"
#include "scene.h"

struct raw_slide_model {
    struct raw_scene_model *scene;

    char const *title;

    mc_count_t buffer_size;
    char *buffer;

    struct slide_error {
        char *message;
        enum slide_error_type { SLIDE_ERROR_SYNTAX, SLIDE_ERROR_RUNTIME } type;
        mc_count_t line;
    } error;

    mc_count_t group_count;
    struct slide_functor_group {
        char const *title;

        mc_count_t tabs;
        mc_count_t mode_count;
        mc_ind_t current_mode;
        struct slide_functor_mode {
            char const *title;

            mc_count_t arg_count;
            char const **arg_titles;
        } *modes;

        // index of where the first tabs start
        mc_ind_t overall_start_index;
        mc_ind_t line;
    } *functor_groups;

    mc_count_t total_functor_args;
    mc_count_t *functor_arg_start;
    mc_count_t *functor_arg_end;

    // as the result of a global scene modification, is this node
    // safe from recompilation?
    mc_bool_t scene_modify_safe;
    // ui dirty
    mc_bool_t dirty;
};

struct aux_slide_model {
    mc_bool_t is_std;

    char const *title;

    mc_count_t child_count, child_capacity;
    struct aux_group_model **children;
};

#if MC_INTERNAL

void
slide_append_to_scene(
    struct raw_slide_model *slide, struct raw_scene_model *scene, mc_ind_t index
);

void
slide_write_error(
    struct raw_slide_model *slide, struct slide_error error,
    mc_bool_t full_flush
);

void
slide_delete(struct raw_slide_model *node);

void
aux_slide_insert_custom_child(
    struct aux_slide_model *slide, struct aux_group_model *entry, mc_ind_t index
);

void
aux_slide_free(struct aux_slide_model *slide);

void
slide_group_free(struct slide_functor_group *group);

void
slide_free(struct raw_slide_model *slide);

#endif

inline mc_ind_t
slide_index_in_parent(struct raw_slide_model const *slide)
{
    if (!slide->scene) {
        return SIZE_MAX;
    }

    for (mc_ind_t i = 0; i < slide->scene->slide_count; i++) {
        if (slide == slide->scene->slides[i]) {
            return i;
        }
    }

    return SIZE_MAX;
}

void
slide_write_data(struct raw_slide_model *slide, char *buffer, mc_count_t size);
