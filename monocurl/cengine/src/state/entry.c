//
//  entry.c
//  Monocurl
//
//  Created by Manu Bhat on 10/21/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <limits.h>
#include <stdlib.h>
#include <string.h>

#include "callback.h"
#include "group.h"
#include "mc_memory.h"
#include "tree.h"

void
aux_entry_write_data(
    struct aux_entry_model *entry, char *data, mc_bool_t stack_allocated
)
{
    char const *old_data = entry->data;

    if (stack_allocated) {
        entry->data = mc_strdup(data);
    }
    else {
        entry->data = data;
    }

    mc_free((char *) old_data);
}

/* assumes it's in the active mode! */
mc_ind_t
aux_entry_index_in_parent(struct aux_entry_model const *entry)
{
    struct aux_group_model_mode const *const mode =
        aux_group_mode(entry->group);
    for (mc_ind_t i = 0; i < mode->aux_entry_count; ++i) {
        if (mode->entries + i == entry) {
            return i;
        }
    }

    return SIZE_MAX;
}

void
aux_entry_free(struct aux_entry_model *entry)
{
    if (!entry) {
        return;
    }

    mc_free((char *) entry->title);
    mc_free((char *) entry->data);
}
