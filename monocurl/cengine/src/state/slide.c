//
//  slide.c
//  Monocurl
//
//  Created by Manu Bhat on 10/21/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <assert.h>
#include <stdlib.h>

#include "callback.h"
#include "mc_memory.h"
#include "slide.h"
#include "tree.h"

#define MC_LOG_TAG "slide"
#include "mc_log.h"

void
slide_append_to_scene(
    struct raw_slide_model *slide, struct raw_scene_model *scene, mc_ind_t index
)
{
    pre_modify(scene);

    MC_MEM_RESERVE(scene->slides, scene->slide_count);
    if (scene->slide_count >= scene->slide_capacity) {
        scene->slide_capacity = MC_MEM_NEXT_CAPACITY(scene->slide_count);
        scene->slides = mc_reallocf(
            scene->slides,
            sizeof(struct raw_slide_model *) * scene->slide_capacity
        );
    }
    mc_buffer_insert(
        scene->slides, &slide, sizeof(struct raw_slide_model *), index,
        &scene->slide_count
    );
    slide->scene = scene;

    for (mc_ind_t i = 0; i < index; ++i) {
        scene->slides[i]->scene_modify_safe = 1;
    }

    post_scene_modify(scene, 1);
}

void
slide_write_error(
    struct raw_slide_model *slide, struct slide_error error,
    mc_bool_t full_flush
)
{
    if (!slide->error.message && !error.message) {
        return;
    }

    if (full_flush) {
        pre_modify(slide->scene);
    }

    mc_free(slide->error.message);

    slide->error = error;
    slide->dirty = 1;

    if (full_flush) {
        post_slide_modify(slide, 1);
    }
    else {
        slide_flush(slide, 1);
    }
}

/* always called with a lock anyaways */
void
aux_slide_insert_custom_child(
    struct aux_slide_model *slide, struct aux_group_model *entry, mc_ind_t index
)
{
    tree_general_insert(slide, NULL, entry, index);
}

void
slide_delete(struct raw_slide_model *node)
{
    pre_modify(node->scene);

    mc_ind_t const index = slide_index_in_parent(node);

    mc_buffer_remove(
        node->scene->slides, sizeof(struct raw_slide_model *), index,
        &node->scene->slide_count
    );

    for (mc_ind_t i = 0; i < index; ++i) {
        node->scene->slides[i]->scene_modify_safe = 1;
    }

    post_scene_modify(node->scene, 1);
    slide_free(node);
}

extern inline mc_ind_t
slide_index_in_parent(struct raw_slide_model const *slide);

void
slide_write_data(struct raw_slide_model *slide, char *buffer, mc_count_t size)
{
    pre_modify(slide->scene);

    mc_free(slide->buffer);
    slide->buffer = buffer;
    slide->buffer_size = size;

    post_slide_modify(slide, 1);
}

void
slide_group_free(struct slide_functor_group *group)
{
    for (mc_ind_t i = 0; i < group->mode_count; ++i) {
        for (mc_ind_t j = 0; j < group->modes[i].arg_count; ++j) {
            mc_free((char *) group->modes[i].arg_titles[j]);
        }
        mc_free(group->modes[i].arg_titles);
        mc_free((char *) group->modes[i].title);
    }
    mc_free(group->modes);
    mc_free((char *) group->title);
}

void
slide_free(struct raw_slide_model *slide)
{
    mc_free((char *) slide->buffer);

    for (mc_ind_t i = 0; i < slide->group_count; ++i) {
        slide_group_free(&slide->functor_groups[i]);
    }
    mc_free(slide->functor_groups);
    mc_free(slide->functor_arg_start);
    mc_free(slide->functor_arg_end);

    mc_free(slide->error.message);

    mc_free((char *) slide->title);
    mc_free(slide);
}

void
aux_slide_free(struct aux_slide_model *slide)
{
    for (mc_ind_t i = 0; i < slide->child_count; ++i) {
        aux_group_free(slide->children[i]);
    }
    mc_free(slide->children);

    mc_free((char *) slide->title);
    mc_free(slide);
}
