//
//  group.h
//  Monocurl
//
//  Created by Manu Bhat on 10/21/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_types.h"
#include "slide.h"

struct aux_group_model {
    struct aux_group_model *parent;
    struct aux_slide_model *slide;

    // null for custom, essentially dictates the properties that are chosen
    char const *group_class;

    mc_count_t mode_count;
    struct aux_group_model_mode {
        char const *key;

        mc_count_t aux_entry_count;
        struct aux_entry_model *entries;
    } *modes;
    char const *mode_key;

    // zero for either no children, or an empty array
    mc_count_t child_count, child_capacity;
    struct aux_group_model **children;

    mc_count_t tabs;
    mc_bool_t doesnt_want_children;

    // index of the character at which the contents of this group is dumped into
    // the buffer
    mc_ind_t dump_start_index;
    mc_ind_t union_header_end_index;
};

#if MC_INTERNAL

#define aux_group_FIRST_ENTRY(group) (group->modes[0].entries)

inline struct aux_group_model **
aux_group_parent_child_array(struct aux_group_model const *group)
{
    return group->parent ? group->parent->children : group->slide->children;
}

inline mc_count_t
aux_group_parent_child_count(struct aux_group_model const *group)
{
    return group->parent ? group->parent->child_count
                         : group->slide->child_count;
}

struct aux_group_model *
aux_group_max_parent(struct aux_group_model *group);

struct aux_group_model *
aux_group_next_in_parent(struct aux_group_model const *group);

struct aux_group_model *
aux_group_prev_in_parent(struct aux_group_model const *group);

struct aux_group_model_mode *
aux_group_mode(struct aux_group_model const *group);

struct aux_entry_model *
aux_group_first_entry(struct aux_group_model *group);

struct aux_entry_model *
aux_group_last_entry(struct aux_group_model *group);

struct aux_group_model *
aux_group_lca(struct aux_group_model *lhs, struct aux_group_model *rhs);

void
aux_group_swap_class(
    struct aux_group_model *group, char const *aux_group_class
);

void
aux_group_partial_swap_mode(
    struct aux_group_model *group, mc_count_t count,
    struct aux_group_model_mode *modes
);

void
aux_group_insert_custom_child(
    struct aux_group_model *group, struct aux_group_model *entry, mc_ind_t index
);

// returns the next focus if successful
void
aux_group_delete(struct aux_group_model *group);

void
aux_group_free(struct aux_group_model *group);

#endif

mc_count_t
aux_group_depth(struct aux_group_model *group);

mc_ind_t
aux_group_mode_index(struct aux_group_model const *group);

mc_bool_t
aux_group_is_last_in_parent(struct aux_group_model const *group);

void
aux_group_set_expands(struct aux_group_model *group, mc_bool_t expands);

void
aux_group_switch_mode(struct aux_group_model *group, char const *new_key);

void
aux_group_partial_switch_mode(
    struct aux_group_model *group, char const *new_key
);

mc_ind_t
aux_group_index_in_parent(struct aux_group_model const *group);
