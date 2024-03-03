//
//  primitives.h
//  Monocurl
//
//  Created by Manu Bhat on 12/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "vector_field.h"

// double
#if MC_INTERNAL
struct vector_field
double_init(struct timeline_execution_context *executor, double value);
struct vector_field
double_copy(
    struct timeline_execution_context *executor, struct vector_field source
);

struct vector_field
double_add(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);

mc_count_t
double_bytes(
    struct timeline_execution_context *executor, struct vector_field field
);

struct vector_field
double_multiply(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
double_sub(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
double_divide(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
double_power(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
double_negative(
    struct timeline_execution_context *executor, struct vector_field field
);

struct vector_field
double_bool(
    struct timeline_execution_context *executor, struct vector_field field
);

struct vector_field
double_comp(
    struct timeline_execution_context *executor, struct vector_field field,
    struct vector_field *rhs
);
mc_hash_t
double_hash(
    struct timeline_execution_context *executor, struct vector_field field
);

// char
struct vector_field
char_init(struct timeline_execution_context *executor, char c);
struct vector_field
char_copy(struct timeline_execution_context *executor, struct vector_field c);

struct vector_field
char_sub(
    struct timeline_execution_context *executor, struct vector_field c,
    struct vector_field *rhs
);
struct vector_field
char_add(
    struct timeline_execution_context *executor, struct vector_field c,
    struct vector_field *rhs
);

struct vector_field
char_comp(
    struct timeline_execution_context *executor, struct vector_field field,
    struct vector_field *rhs
);
mc_hash_t
char_hash(
    struct timeline_execution_context *executor, struct vector_field field
);

mc_count_t
char_bytes(
    struct timeline_execution_context *executor, struct vector_field field
);

#endif
