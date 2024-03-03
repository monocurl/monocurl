//
//  entry.h
//  Monocurl
//
//  Created by Manu Bhat on 10/21/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "group.h"
#include "mc_env.h"
#include "mc_types.h"

struct aux_entry_model {
    struct aux_group_model *group;

    char const *title;
    char const *data;
    mc_bool_t is_empty;

    mc_ind_t overall_start_index;
    mc_ind_t title_end_index;
};

#if MC_INTERNAL
void
aux_entry_free(struct aux_entry_model *entry);
#endif

void
aux_entry_write_data(
    struct aux_entry_model *entry, char *data, mc_bool_t stack_allocated
);

/* assumes it's in the active mode! */
mc_ind_t
aux_entry_index_in_parent(struct aux_entry_model const *entry);
