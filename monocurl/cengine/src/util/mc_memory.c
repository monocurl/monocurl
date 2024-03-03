//
//  memory.c
//  Monocurl
//
//  Created by Manu Bhat on 9/25/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "mc_memory.h"

mc_bitmask_t
mc_po2_ceil(mc_bitmask_t current)
{
    // fill in all 1s that currently exist
    // then overflow
    --current;

    current |= current >> 1;
    current |= current >> 2;
    current |= current >> 4;
    current |= current >> 8;
    current |= current >> 16;
    current |= current >> 32;

    return current + 1;
}

mc_bitmask_t
mc_memory_upsize(mc_bitmask_t current)
{
    // fill in all 1s that currently exist
    // then overflow
    --current;

    current |= current >> 1;
    current |= current >> 2;
    current |= current >> 4;
    current |= current >> 8;
    current |= current >> 16;
    current |= current >> 32;

    if (current < (1 << MC_MEM_INITIAL_COUNT_EXP)) {
        return 1 << MC_MEM_INITIAL_COUNT_EXP;
    }

    return current + 1;
}

void
mc_buffer_insert(
    void *pointer, void const *element, size_t of_size, mc_ind_t at_index,
    mc_count_t *count
)
{
    char *const base = (char *) pointer + of_size * at_index;
    memmove(base + of_size, base, (*count - at_index) * of_size);
    memcpy(base, element, of_size);

    ++*count;
}

void
mc_buffer_remove(
    void *pointer, size_t of_size, mc_ind_t at_index, mc_count_t *count
)
{
    char *const base = (char *) pointer + of_size * at_index;
    memmove(base, base + of_size, (*count - at_index - 1) * of_size);

    --*count;
}

extern inline void *
_mc_malloc(mc_count_t bytes);
extern inline void *
_mc_calloc(mc_count_t count, size_t size);
extern inline void *
_mc_reallocf(void *ptr, mc_count_t bytes);
extern inline void
_mc_free(void *ptr);
