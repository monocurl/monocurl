//
//  map.h
//  Monocurl
//
//  Created by Manu Bhat on 10/29/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "vector_field.h"

/* hash for now, but since a global comparision function is defined, we might be
 * able to do with a treemap */

struct map_node {
    struct vector_field field;
    struct vector_field value;

    struct map_node *next_bucket;
    struct map_node *next_ins;
};

struct map {
    mc_count_t field_count;
    mc_count_t node_capacity;

    struct map_node **nodes;
    struct map_node head; /* for in-order insertion */
    struct map_node *tail;

    mc_hash_t hash_cache;
};

#if MC_INTERNAL
struct vector_field
map_init(struct timeline_execution_context *executor);
struct vector_field
map_copy(struct timeline_execution_context *executor, struct vector_field map);

/* l value, creates if not present */
struct vector_field
map_index(
    struct timeline_execution_context *executor, struct vector_field map,
    struct vector_field *index
);

mc_hash_t
map_hash(struct timeline_execution_context *executor, struct vector_field map);
struct vector_field
map_comp(
    struct timeline_execution_context *executor, struct vector_field map,
    struct vector_field *rhs
);

struct vector_field
map_contains(
    struct timeline_execution_context *executor, struct vector_field map,
    struct vector_field *element
);

void
map_free(
    struct timeline_execution_context *executor, struct vector_field field
);

mc_count_t
map_bytes(
    struct timeline_execution_context *executor, struct vector_field field
);

#endif
