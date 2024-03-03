//
//  vector_field.h
//  Monocurl
//
//  Created by Manu Bhat on 12/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_types.h"
#include <stdio.h>

struct timeline_execution_context;
struct vector_field_vtable;
struct vector_field {
    union {
        char c;
        double doub;
        void *pointer;
        mc_hash_t hash;
    } value;

    struct vector_field_vtable const *vtable; // null for unitialized
};

#include "mc_env.h"
#include "mc_memory.h"
#include "timeline_execution_context.h"

struct vector_field_vtable {
    /* type */
    enum vector_field_type {
        VECTOR_FIELD_TYPE_DOUBLE = 1 << 0, //
        VECTOR_FIELD_TYPE_CHAR = 1 << 1,

        VECTOR_FIELD_TYPE_FUNCTION = 1 << 2,
        VECTOR_FIELD_TYPE_FUNCTOR = 1 << 3, // stores function parameters

        VECTOR_FIELD_TYPE_VECTOR = 1 << 4, //
        VECTOR_FIELD_TYPE_MAP = 1 << 5,    //

        VECTOR_FIELD_TYPE_MESH = 1 << 6, // Also used for domains
        VECTOR_FIELD_TYPE_ANIMATION = 1 << 7,

        VECTOR_FIELD_TYPE_LVALUE = 1 << 8,
    } type;

    char const *type_name;

    struct vector_field (*copy)(
        struct timeline_execution_context *executor, struct vector_field field
    );
    // only applicable to an l-value and vector of l-value
    struct vector_field (*assign)(
        struct timeline_execution_context *executor, struct vector_field dst,
        struct vector_field *src
    );
    struct vector_field (*plus_assign)(
        struct timeline_execution_context *executor, struct vector_field dst,
        struct vector_field *src
    );

    // return value goes to register
    void (*op_call)(
        struct timeline_execution_context *executor, struct vector_field caller,
        mc_count_t fc, struct vector_field *fields
    );

    /* algberaic operators */
    struct vector_field (*op_add)(
        struct timeline_execution_context *executor, struct vector_field lhs,
        struct vector_field *rhs
    );
    struct vector_field (*op_multiply)(
        struct timeline_execution_context *executor, struct vector_field lhs,
        struct vector_field *rhs
    );
    struct vector_field (*op_subtract)(
        struct timeline_execution_context *executor, struct vector_field lhs,
        struct vector_field *rhs
    );
    struct vector_field (*op_negative)(
        struct timeline_execution_context *executor, struct vector_field field
    );
    struct vector_field (*op_divide)(
        struct timeline_execution_context *executor, struct vector_field lhs,
        struct vector_field *rhs
    );
    struct vector_field (*op_power)(
        struct timeline_execution_context *executor, struct vector_field lhs,
        struct vector_field *rhs
    );

    /* boolean operators */
    struct vector_field (*op_bool)(
        struct timeline_execution_context *executor, struct vector_field field
    );
    struct vector_field (*op_contains)(
        struct timeline_execution_context *executor, struct vector_field field,
        struct vector_field *element
    );
    struct vector_field (*op_comp)(
        struct timeline_execution_context *executor, struct vector_field field,
        struct vector_field *rhs
    );

    /* return type must be an lvalue. Root is freed after execution */
    struct vector_field (*op_index)(
        struct timeline_execution_context *executor, struct vector_field field,
        struct vector_field *index
    );
    struct vector_field (*op_attribute)(
        struct timeline_execution_context *executor, struct vector_field field,
        char const *attribute
    );

    mc_hash_t (*hash)(
        struct timeline_execution_context *executor, struct vector_field field
    );

    mc_count_t (*bytes)(
        struct timeline_execution_context *executor, struct vector_field
    );

    void (*free)(
        struct timeline_execution_context *executor, struct vector_field field
    );

    mc_bool_t out_of_frame_like;
};

#if MC_INTERNAL
#define VECTOR_FIELD_TYPE_STR_BUFFER 128

void
vector_field_type_to_a(enum vector_field_type type, char *out);
struct vector_field
vector_field_extract_type_message(
    struct timeline_execution_context *executor, struct vector_field *raw,
    enum vector_field_type target, char const *msg
);
struct vector_field
vector_field_extract_type(
    struct timeline_execution_context *executor, struct vector_field *raw,
    enum vector_field_type target
);
struct vector_field
vector_field_nocopy_extract_type_message(
    struct timeline_execution_context *executor, struct vector_field raw,
    enum vector_field_type target, char const *message
);
struct vector_field
vector_field_nocopy_extract_type(
    struct timeline_execution_context *executor, struct vector_field raw,
    enum vector_field_type target
);
struct vector_field
vector_field_safe_extract_type(
    struct timeline_execution_context *executor, struct vector_field raw,
    enum vector_field_type target
);
struct vector_field
vector_field_lvalue_unwrap(
    struct timeline_execution_context *executor, struct vector_field *raw
);

struct vector_field
vector_field_functor_elide(
    struct timeline_execution_context *executor, struct vector_field *raw
);

struct vector_field
vector_field_lvalue_copy(
    struct timeline_execution_context *executor, struct vector_field raw
);

char const *
vector_field_str(
    struct timeline_execution_context *executor, struct vector_field str
);

#define VECTOR_FIELD_PURE                                                      \
    (unsigned int) ((unsigned int) ((1 << 15) - 1) &                           \
                    ~(unsigned int) VECTOR_FIELD_TYPE_LVALUE &                 \
                    ~(unsigned int) VECTOR_FIELD_TYPE_FUNCTOR)
#define VECTOR_FIELD_NONLVALUE                                                 \
    (unsigned int) ((unsigned int) ((1 << 15) - 1) &                           \
                    ~(unsigned int) VECTOR_FIELD_TYPE_LVALUE)
#define VECTOR_FIELD_NULL                                                      \
    (struct vector_field) { 0 }

#define VECTOR_FIELD_COPY(executor, field)                                     \
    ((field).vtable ? (field).vtable->copy(executor, field) : VECTOR_FIELD_NULL)
#define VECTOR_FIELD_BYTES(executor, field)                                    \
    ((field).vtable ? (field).vtable->bytes(executor, field) : sizeof(field))
#define VECTOR_FIELD_FREE(executor, field)                                     \
    do {                                                                       \
        if ((field).vtable && (field).vtable->free)                            \
            (field).vtable->free(executor, field);                             \
    } while (0)

#define VECTOR_FIELD_UNARY(executor, field, op)                                \
    (field).vtable->op(executor, field)
#define VECTOR_FIELD_HASH(executor, field)                                     \
    ((field).vtable                                                            \
         ? VECTOR_FIELD_UNARY(executor, field, hash) % MC_HASHING_PRIME        \
         : 1)

#define VECTOR_FIELD_DBOOL(field)                                              \
    ((int) (fabs((field).value.doub) > DBL_EPSILON))
#define VECTOR_FIELD_BINARY(executor, lhs, op, rhs)                            \
    (lhs).vtable->op(executor, lhs, rhs)
#define VECTOR_FIELD_ERROR(executor, ...)                                      \
    timeline_executor_report_error(executor, __VA_ARGS__)
#endif
