//
//  memory.h
//  Monocurl
//
//  Created by Manu Bhat on 9/25/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>
#include <stdlib.h>

#include "mc_env.h"
#include "mc_types.h"

#if MC_INTERNAL
#define MC_MEM_RESIZE_SCALE 1.4
#define MC_MEM_INITIAL_COUNT_EXP 3

/* https://graphics.stanford.edu/~seander/bithacks.html#DetermineIfPowerOf2 */
/* this is done whenever we append monotonically, so scaling by 2 is not really
 * of concern */
#define MC_MEM_NEXT_CAPACITY(count)                                            \
    (mc_count_t)(                                                              \
        count < (1ull << MC_MEM_INITIAL_COUNT_EXP)                             \
            ? 1ull << MC_MEM_INITIAL_COUNT_EXP                                 \
            : (count) * MC_MEM_RESIZE_SCALE                                    \
    )
#define MC_MEM_RESERVE(var, count)                                             \
    if (!((count) & (((count) -1) | ((1ull << MC_MEM_INITIAL_COUNT_EXP) - 1))  \
        ))                                                                     \
    (var) = mc_reallocf(                                                       \
        var, sizeof(*var) *                                                    \
                 (!(count) ? 1 << MC_MEM_INITIAL_COUNT_EXP : (count) << 1)     \
    )
#define MC_MEM_RESERVEN(var, count, delta)                                     \
    if ((!(count) && (delta)) ||                                               \
        (((count) -1) ^ ((count) -1 + (delta))) >                              \
            (((count) -1) | ((1ull << MC_MEM_INITIAL_COUNT_EXP) - 1)))         \
    (var) = mc_reallocf(                                                       \
        var, sizeof(*var) * (mc_po2_ceil(                                      \
                                (count) + (delta) |                            \
                                ((1ull << MC_MEM_INITIAL_COUNT_EXP) - 1)       \
                            ))                                                 \
    )

#define MC_HASHING_PRIME 53982894593057ULL

mc_bitmask_t
mc_po2_ceil(mc_bitmask_t current);

mc_bitmask_t
mc_memory_upsize(mc_bitmask_t current);

// assumes already allocated memory
void
mc_buffer_insert(
    void *pointer, void const *element, size_t of_size, mc_ind_t at_index,
    mc_count_t *count
);
void
mc_buffer_remove(
    void *pointer, size_t of_size, mc_ind_t at_index, mc_count_t *count
);

inline void *
_mc_malloc(mc_count_t bytes)
{
    return malloc(bytes);
}

inline void *
_mc_calloc(mc_count_t count, size_t size)
{
    return calloc(count, size);
}

inline void *
_mc_reallocf(void *ptr, mc_count_t bytes)
{
    /* not undefined behavior since if ptr was originally allocated for zero
       bytes (but is non null) and now bytes is zero, can an implementation
       return null? This would screw up some things if so Hard to read if the C
       standard says that an implementation may choose either or, or if a given
       implementation is locked to a choice. I do not believe they can switch
    */

    void *const ret = realloc(ptr, bytes);
    if (!ret && bytes) {
        free(ptr);
    }
    return ret;
}

inline void
_mc_free(void *ptr)
{
    free(ptr);
}

#define mc_malloc(bytes) _mc_malloc(bytes)
#define mc_calloc(count, size) _mc_calloc(count, size)
#define mc_reallocf(ptr, bytes) _mc_reallocf(ptr, bytes)
#define mc_free(ptr) _mc_free(ptr)
#endif
