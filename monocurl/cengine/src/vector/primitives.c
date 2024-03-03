//
//  primitives.c
//  Monocurl
//
//  Created by Manu Bhat on 12/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <math.h>
#include <stdlib.h>
#include <string.h>

#include "primitives.h"

static struct vector_field_vtable const double_vtable = {
    .type = VECTOR_FIELD_TYPE_DOUBLE,
    .type_name = "double",

    .copy = double_copy,
    .assign = NULL,
    .plus_assign = NULL,

    .op_call = NULL,

    .op_add = double_add,
    .op_multiply = double_multiply,
    .op_subtract = double_sub,
    .op_negative = double_negative,
    .op_divide = double_divide,
    .op_power = double_power,

    .op_bool = double_bool,
    .op_contains = NULL,
    .op_comp = double_comp,

    .op_index = NULL,
    .op_attribute = NULL,

    .hash = double_hash,

    .bytes = double_bytes,
    .free = NULL,

    .out_of_frame_like = 0,
};

static struct vector_field_vtable const char_vtable = {
    .type = VECTOR_FIELD_TYPE_CHAR,
    .type_name = "character",

    .copy = char_copy,
    .assign = NULL,
    .plus_assign = NULL,

    .op_call = NULL,

    .op_add = char_add,
    .op_multiply = NULL,
    .op_subtract = char_sub,
    .op_negative = NULL,
    .op_divide = NULL,
    .op_power = NULL,

    .op_bool = NULL,
    .op_contains = NULL,
    .op_comp = char_comp,

    .op_index = NULL,
    .op_attribute = NULL,

    .hash = char_hash,

    .bytes = char_bytes,
    .free = NULL,

    .out_of_frame_like = 0,
};

struct vector_field
double_init(struct timeline_execution_context *executor, double value)
{
    return (struct vector_field){
        .value = { .doub = value },
        .vtable = &double_vtable,
    };
}

struct vector_field
double_copy(
    struct timeline_execution_context *executor, struct vector_field source
)
{
    return source;
}

struct vector_field
double_add(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field rhs_v =
        vector_field_extract_type(executor, rhs, VECTOR_FIELD_TYPE_DOUBLE);
    if (!rhs_v.vtable) {
        return VECTOR_FIELD_NULL;
    }

    return (struct vector_field){
        .value = { .doub = lhs.value.doub + rhs_v.value.doub },
        .vtable = &double_vtable,
    };
}

struct vector_field
double_multiply(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field rhs_v =
        vector_field_extract_type(executor, rhs, VECTOR_FIELD_TYPE_DOUBLE);
    if (!rhs_v.vtable) {
        return VECTOR_FIELD_NULL;
    }

    return (struct vector_field){
        .value = { .doub = lhs.value.doub * rhs_v.value.doub },
        .vtable = &double_vtable,
    };
}

mc_count_t
double_bytes(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    return sizeof(field);
}

struct vector_field
double_sub(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field rhs_v =
        vector_field_extract_type(executor, rhs, VECTOR_FIELD_TYPE_DOUBLE);
    if (!rhs_v.vtable) {
        return VECTOR_FIELD_NULL;
    }

    return (struct vector_field){
        .value = { .doub = lhs.value.doub - rhs_v.value.doub },
        .vtable = &double_vtable,
    };
}

struct vector_field
double_divide(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field rhs_v =
        vector_field_extract_type(executor, rhs, VECTOR_FIELD_TYPE_DOUBLE);
    if (!rhs_v.vtable) {
        return VECTOR_FIELD_NULL;
    }

    double const ret = lhs.value.doub / rhs_v.value.doub;
    if (isinf(ret) || isnan(ret)) {
        VECTOR_FIELD_ERROR(executor, "Divide by zero");
        return VECTOR_FIELD_NULL;
    }

    return (struct vector_field){
        .value = { .doub = lhs.value.doub / rhs_v.value.doub },
        .vtable = &double_vtable,
    };
}

struct vector_field
double_power(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field rhs_v =
        vector_field_extract_type(executor, rhs, VECTOR_FIELD_TYPE_DOUBLE);
    if (!rhs_v.vtable) {
        return VECTOR_FIELD_NULL;
    }

    double const res = pow(lhs.value.doub, rhs_v.value.doub);

    if (res != res) {
        VECTOR_FIELD_ERROR(executor, "Invalid power argument");
        return VECTOR_FIELD_NULL;
    }

    return (struct vector_field){
        .value = { .doub = res },
        .vtable = &double_vtable,
    };
}

struct vector_field
double_negative(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    return (struct vector_field){
        .value = { .doub = -field.value.doub },
        .vtable = &double_vtable,
    };
}

struct vector_field
double_comp(
    struct timeline_execution_context *executor, struct vector_field field,
    struct vector_field *rhs
)
{
    struct vector_field rhs_val = vector_field_safe_extract_type(
        executor, *rhs, VECTOR_FIELD_TYPE_DOUBLE
    );

    int ret;
    if (!rhs_val.vtable) {
        return double_init(executor, 1);
    }
    else if ((ret = (int) rhs_val.vtable->type - (int) VECTOR_FIELD_TYPE_DOUBLE) != 0) {
        return double_init(executor, ret);
    }

    double const l = field.value.doub;
    double const r = rhs_val.value.doub;
    if (l - r > 0) {
        return double_init(executor, 1);
    }
    else if (r - l > 0) {
        return double_init(executor, -1);
    }
    else {
        return double_init(executor, 0);
    }
}

struct vector_field
double_bool(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    return double_init(executor, fabs(field.value.doub) > DBL_EPSILON);
}

mc_hash_t
double_hash(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    mc_hash_t const ret = field.value.hash;
    return !ret ? (mc_hash_t) 1 << (sizeof(mc_hash_t) * 8 - 1) : ret;
}

struct vector_field
char_init(struct timeline_execution_context *executor, char c)
{
    return (struct vector_field){
        .value = { .c = c },
        .vtable = &char_vtable,
    };
}

struct vector_field
char_copy(struct timeline_execution_context *executor, struct vector_field c)
{
    return c;
}

struct vector_field
char_sub(
    struct timeline_execution_context *executor, struct vector_field c,
    struct vector_field *rhs
)
{
    struct vector_field rhs_v =
        vector_field_extract_type(executor, rhs, VECTOR_FIELD_TYPE_CHAR);
    if (!rhs_v.vtable) {
        return VECTOR_FIELD_NULL;
    }

    return double_init(executor, c.value.c - rhs_v.value.c);
}

struct vector_field
char_add(
    struct timeline_execution_context *executor, struct vector_field c,
    struct vector_field *rhs
)
{
    struct vector_field rhs_v =
        vector_field_extract_type(executor, rhs, VECTOR_FIELD_TYPE_DOUBLE);
    if (!rhs_v.vtable) {
        return VECTOR_FIELD_NULL;
    }

    return (struct vector_field){
        .value = { .c = c.value.c + (char) rhs_v.value.doub },
        .vtable = &char_vtable,
    };
}

struct vector_field
char_comp(
    struct timeline_execution_context *executor, struct vector_field field,
    struct vector_field *rhs
)
{
    struct vector_field rhs_val =
        vector_field_safe_extract_type(executor, *rhs, VECTOR_FIELD_TYPE_CHAR);

    int ret;
    if (!rhs_val.vtable) {
        return double_init(executor, 1);
    }
    else if ((ret = (int) rhs_val.vtable->type - (int) VECTOR_FIELD_TYPE_CHAR) != 0) {
        return double_init(executor, ret);
    }

    return double_init(executor, (int) field.value.c - (int) rhs_val.value.c);
}

mc_hash_t
char_hash(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    mc_hash_t const ret = (mc_hash_t) field.value.c;

    return !ret ? (mc_hash_t) CHAR_MAX + 1 : ret;
}

mc_count_t
char_bytes(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    return sizeof(field);
}
