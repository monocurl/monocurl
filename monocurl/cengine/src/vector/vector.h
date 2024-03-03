//
//  vector.h
//  Monocurl
//
//  Created by Manu Bhat on 10/29/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "timeline_execution_context.h"
#include "vector_field.h"

struct vector {
    mc_count_t field_count;
    struct vector_field *fields;
    mc_hash_t hash_cache;
};

#if MC_INTERNAL
struct vector_field
vector_init(struct timeline_execution_context *executor);

struct vector_field
vector_copy(
    struct timeline_execution_context *executor, struct vector_field vector
);
struct vector_field
vector_assign(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *source
);

struct vector_field
vector_contains(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *element
);

struct vector_field
vector_plus(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *field
);
struct vector_field
vector_literal_plus(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *field
);
struct vector_field
vector_index(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *index
); // always returns an lvalue

struct vector_field
vector_comp(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *rhs
);
mc_hash_t
vector_hash(
    struct timeline_execution_context *executor, struct vector_field vector
);

void
vector_free(
    struct timeline_execution_context *executor, struct vector_field vector
);

mc_count_t
vector_bytes(
    struct timeline_execution_context *executor, struct vector_field field
);

#endif
