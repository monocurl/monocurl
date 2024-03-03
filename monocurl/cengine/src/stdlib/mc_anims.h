//
//  mc_anims.h
//  Monocurl
//
//  Created by Manu Bhat on 2/19/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once
#include <stdio.h>

#include "anim_show.h"
#include "anim_util.h"
#include "mc_env.h"
#include "mc_lib_helpers.h"
#include "mc_meshes.h"
#include "vector.h"

#if MC_INTERNAL
/*
 func Anim(x) = x
 func NullAnim(x) = x
 func TimedAnim(x) = x
 */

LIBMC_DEC_FUNC(animation);

// in form of t, config, time, unit_map!
// returns < 0 for initial frame, 1 for past target, and nAn for error
double
anim_current_time(
    struct timeline_execution_context *executor, struct vector_field *fields,
    double (*default_lerp)(double t)
);

struct mesh_mapped_subset {
    struct mesh_tag_subset subset;
    struct mesh_tag_subset invert;
};

void
mesh_mapped_subset_free(
    struct timeline_execution_context *executor,
    struct mesh_mapped_subset mapped
);

/* prepares a mesh tree for modification */
mc_status_t
owned_mesh_tree(
    struct timeline_execution_context *executor, struct vector_field lvalue
);

#define ANIM_TIME(time, index)                                                 \
    float const time =                                                         \
        (float) anim_current_time(executor, &fields[index], &anim_smooth);     \
    do {                                                                       \
        if (time != time) {                                                    \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            return;                                                            \
        }                                                                      \
    } while (0)

struct mesh_tag_subset
anim_read_flat_subset(
    struct timeline_execution_context *executor, struct vector const *vector
);

#pragma message(                                                                                                              \
    "TODO make these into their own methods and only have necessary stuff in macros. Makes a lot of assumptions about set up" \
)
#endif
