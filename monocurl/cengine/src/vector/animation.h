//
//  animation.h
//  Monocurl
//
//  Created by Manu Bhat on 12/27/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "entry.h"
#include "mc_types.h"
#include "vector_field.h"

struct animation {
    struct vector_field pull_state;
    struct vector_field push_state;

    struct vector_field sentinel;
    mc_bool_t sticky;

    /* animation cannot be played multiple times, which helps out a lot*/
    enum animation_sentinel {
        ANIMATION_UNPLAYED,
        ANIMATION_PLAYING,
        ANIMATION_SENTINEL_IF_OTHERS,
        ANIMATION_SENTINEL_FORCE,
        ANIMATION_ERROR
    } sentinel_state;
    double time;

    mc_hash_t hash_cache;

    /* unowned */
    struct raw_slide_model *slide;
    mc_count_t line;
};

#if MC_INTERNAL
struct vector_field
animation_init(
    struct timeline_execution_context *executor, struct vector_field pull_state,
    struct vector_field push_state, struct vector_field sentinel
);

struct vector_field
animation_sticky_init(struct timeline_execution_context *executor);

struct vector_field
animation_copy(
    struct timeline_execution_context *executor, struct vector_field source
);

mc_hash_t
animation_hash(
    struct timeline_execution_context *executor, struct vector_field source
);
mc_count_t
animation_bytes(
    struct timeline_execution_context *executor, struct vector_field source
);
struct vector_field
animation_comp(
    struct timeline_execution_context *executor, struct vector_field source,
    struct vector_field *rhs
);

mc_status_t
animation_step(
    struct timeline_execution_context *executor, struct animation *animation,
    double dt
);

void
animation_free(
    struct timeline_execution_context *executor, struct vector_field animation
);
#endif
