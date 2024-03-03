//
//  tree.c
//  Monocurl
//
//  Created by Manu Bhat on 9/23/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <assert.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>

#include "callback.h"
#include "mc_memory.h"
#include "tree.h"

void
tree_general_insert(
    struct aux_slide_model *slide, struct aux_group_model *parent,
    struct aux_group_model *entry, mc_ind_t index
)
{
    entry->parent = parent;
    entry->slide = slide;

    if (parent) {
        if (parent->child_capacity <= parent->child_count) {
            parent->child_capacity = MC_MEM_NEXT_CAPACITY(parent->child_count);
            parent->children = mc_reallocf(
                parent->children,
                sizeof(struct aux_group_model *) * parent->child_capacity
            );
        }
        mc_buffer_insert(
            parent->children, &entry, sizeof(struct aux_group_model *), index,
            &parent->child_count
        );
    }
    else {
        if (slide->child_capacity <= slide->child_count) {
            slide->child_capacity = MC_MEM_NEXT_CAPACITY(slide->child_count);
            slide->children = mc_reallocf(
                slide->children,
                sizeof(struct aux_group_model *) * slide->child_capacity
            );
        }
        mc_buffer_insert(
            slide->children, &entry, sizeof(struct aux_group_model *), index,
            &slide->child_count
        );
    }
}
