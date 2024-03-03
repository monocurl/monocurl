//
//  lvalue.h
//  Monocurl
//
//  Created by Manu Bhat on 12/16/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_types.h"
#include "vector_field.h"

#if MC_INTERNAL
extern struct vector_field_vtable const reference_vtable;
extern struct vector_field_vtable const derived_persistent_vtable;

struct vector_field
lvalue_init(
    struct timeline_execution_context *executor, struct vector_field *main
);

struct vector_field
lvalue_const_init(
    struct timeline_execution_context *executor, struct vector_field *main
);

struct vector_field
lvalue_persistent_init(
    struct timeline_execution_context *executor, struct vector_field *main
);

struct vector_field
lvalue_parameter_init(
    struct timeline_execution_context *executor, struct vector_field *main
);

struct vector_field
lvalue_reference_init(
    struct timeline_execution_context *executor, struct vector_field *main
);

struct vector_field
lvalue_copy(
    struct timeline_execution_context *executor, struct vector_field lvalue
);
struct vector_field
lvalue_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
);
struct vector_field
lvalue_parameter_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
);
struct vector_field
lvalue_plus_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
);
struct vector_field
lvalue_reference_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
);
struct vector_field
lvalue_reference_plus_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
);

void
lvalue_call(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    mc_count_t count, struct vector_field *args
);

mc_count_t
lvalue_bytes(
    struct timeline_execution_context *executor, struct vector_field field
);

struct vector_field
lvalue_add(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
lvalue_multiply(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
lvalue_sub(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
lvalue_divide(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
lvalue_power(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
lvalue_negative(
    struct timeline_execution_context *executor, struct vector_field field
);

struct vector_field
lvalue_bool(
    struct timeline_execution_context *executor, struct vector_field field
);
struct vector_field
lvalue_contains(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *element
);

struct vector_field
lvalue_index(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *index
);
struct vector_field
const_lvalue_index(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *index
);
struct vector_field
persistent_lvalue_index(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *index
);
struct vector_field
lvalue_attribute(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    char const *attribute
);
struct vector_field
const_lvalue_attribute(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    char const *attribute
);
struct vector_field
persistent_lvalue_attribute(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    char const *attribute
);

struct vector_field
lvalue_comp(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *rhs
);
mc_hash_t
lvalue_hash(
    struct timeline_execution_context *executor, struct vector_field lvalue
);
#endif
