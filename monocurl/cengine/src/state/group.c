//
//  group.c
//  Monocurl
//
//  Created by Manu Bhat on 10/21/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <limits.h>
#include <stdlib.h>
#include <string.h>

#include "callback.h"
#include "mc_memory.h"
#include "tree.h"

void
aux_group_insert_custom_child(
    struct aux_group_model *group, struct aux_group_model *entry, mc_ind_t index
)
{
    tree_general_insert(group->slide, group, entry, index);
}

extern inline struct aux_group_model **
aux_group_parent_child_array(struct aux_group_model const *group);

extern inline mc_count_t
aux_group_parent_child_count(struct aux_group_model const *group);

void
aux_group_switch_mode(struct aux_group_model *group, char const *new_key)
{
    for (mc_ind_t i = 0; i < group->mode_count; ++i) {
        if (!strcmp(group->modes[i].key, new_key)) {
            group->mode_key = group->modes[i].key;
        }
    }
}

void
aux_group_partial_switch_mode(
    struct aux_group_model *group, char const *new_key
)
{
    group->mode_key = new_key;
}

mc_ind_t
aux_group_mode_index(struct aux_group_model const *group)
{
    for (mc_ind_t i = 0; i < group->mode_count; i++) {
        if (!strcmp(group->modes[i].key, group->mode_key)) {
            return i;
        }
    }

    return SIZE_MAX;
}

// what index is this node
mc_ind_t
aux_group_index_in_parent(struct aux_group_model const *group)
{
    struct aux_group_model **const array = aux_group_parent_child_array(group);
    mc_count_t const count = aux_group_parent_child_count(group);

    for (mc_ind_t i = 0; i < count; ++i) {
        if (array[i] == group) {
            return i;
        }
    }

    return SIZE_MAX;
}

struct aux_group_model_mode *
aux_group_mode(struct aux_group_model const *group)
{
    for (mc_ind_t i = 0; i < group->mode_count; i++) {
        if (!strcmp(group->modes[i].key, group->mode_key)) {
            return group->modes + i;
        }
    }
    return NULL;
}

mc_bool_t
aux_group_is_last_in_parent(struct aux_group_model const *group)
{
    mc_ind_t const index = aux_group_index_in_parent(group);
    mc_count_t const count = aux_group_parent_child_count(group);

    return index == count - 1;
}

struct aux_entry_model *
aux_group_first_entry(struct aux_group_model *group)
{
    return aux_group_mode(group)->entries;
}

struct aux_entry_model *
aux_group_last_entry(struct aux_group_model *group)
{
    struct aux_group_model_mode *mode = aux_group_mode(group);
    return mode->entries + mode->aux_entry_count - 1;
}

void
aux_group_swap_class(struct aux_group_model *group, char const *class)
{
    char const *const old = group->group_class;
    group->group_class = class;

    mc_free((char *) old);
}

void
aux_group_partial_swap_mode(
    struct aux_group_model *group, mc_count_t count,
    struct aux_group_model_mode *modes
)
{
    mc_count_t const old_count = group->mode_count;
    struct aux_group_model_mode *const old_modes = group->modes;

    group->mode_count = count;
    group->modes = modes;

    for (mc_ind_t i = 0; i < old_count; ++i) {
        struct aux_group_model_mode const mode = old_modes[i];

        mc_free((char *) mode.key);
        for (mc_ind_t j = 0; j < mode.aux_entry_count; j++) {
            aux_entry_free(mode.entries + j);
        }
        mc_free(mode.entries);
    }
    mc_free(old_modes);
}

void
aux_group_delete(struct aux_group_model *group)
{
    /* detached entirely...*/
    if (!group->parent && !group->slide) {
        return;
    }

    mc_ind_t const index = aux_group_index_in_parent(group);
    mc_count_t *const count = group->parent ? &group->parent->child_count
                                            : &group->slide->child_count;

    // by definition of a deletable group, it must be custom, therefore there
    // will only be 1 entry
    struct aux_group_model **const child_array =
        group->parent ? group->parent->children : group->slide->children;

    mc_buffer_remove(
        child_array, sizeof(struct aux_group_model *), index, count
    );

    if (*count == 0) {
        if (group->parent) {
            group->parent->children = NULL;
            group->parent->child_capacity = 0;
        }
        else {
            group->slide->children = NULL;
            group->slide->child_count = 0;
        }

        mc_free(child_array);
    }

    aux_group_free(group);
}

void
aux_group_free(struct aux_group_model *group)
{
    // group class need be freed
    mc_free((char *) group->group_class);

    // free modes
    for (mc_ind_t i = 0; i < group->mode_count; ++i) {
        struct aux_group_model_mode *const mode = group->modes + i;

        mc_free((char *) mode->key);
        for (mc_ind_t j = 0; j < mode->aux_entry_count; j++) {
            aux_entry_free(mode->entries + j);
        }
        mc_free(mode->entries);
    }
    mc_free(group->modes);

    // free children
    for (mc_ind_t i = 0; i < group->child_count; ++i) {
        aux_group_free(group->children[i]);
    }
    mc_free(group->children);

    mc_free(group);
}
