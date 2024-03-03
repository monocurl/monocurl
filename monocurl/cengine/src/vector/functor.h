//
//  functor.h
//  Monocurl
//
//  Created by Manu Bhat on 12/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "function.h"
#include "mc_types.h"
#include "timeline_instruction.h"
#include "vector_field.h"

struct vector_named_field {
    char const *name; /* NULL when not assignable */
    struct vector_field field;
    mc_hash_t last_hash;
    mc_bool_t dirty;
};

struct functor {
    struct vector_field function;

    mc_bool_t force_const; /* whenever theres a reference argument */
    mc_count_t argument_count;
    /* names are reused and therefore unowned */
    struct vector_named_field *arguments;

    struct vector_field result;

    mc_bool_t dirty;
};

#if MC_INTERNAL
struct vector_field
functor_get_res(
    struct timeline_execution_context *executor, struct vector_field functor
);

struct vector_field
functor_steal_res(
    struct timeline_execution_context *executor, struct vector_field functor
);

struct vector_field
functor_init(
    struct timeline_execution_context *executor, mc_count_t arg_c,
    struct vector_named_field *arguments, struct vector_field function,
    struct vector_field start, mc_bool_t force_const
);

struct vector_field
functor_copy(
    struct timeline_execution_context *executor, struct vector_field functor
);

mc_count_t
functor_bytes(
    struct timeline_execution_context *executor, struct vector_field functor
);

struct vector_field
functor_add(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
functor_multiply(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
functor_sub(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
functor_divide(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
functor_power(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
functor_negative(
    struct timeline_execution_context *executor, struct vector_field field
);

struct vector_field
functor_bool(
    struct timeline_execution_context *executor, struct vector_field functor
);

struct vector_field
functor_index(
    struct timeline_execution_context *executor, struct vector_field functor,
    struct vector_field *index
);
struct vector_field
functor_attribute(
    struct timeline_execution_context *executor, struct vector_field functor,
    char const *attribute
);

struct vector_field
functor_comp(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);
struct vector_field
functor_contains(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
);

mc_hash_t
functor_hash(
    struct timeline_execution_context *executor, struct vector_field functor
);

void
functor_free(
    struct timeline_execution_context *executor, struct vector_field functor
);
#endif
