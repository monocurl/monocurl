//
//  lvalue.c
//  Monocurl
//
//  Created by Manu Bhat on 12/16/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include "lvalue.h"

static struct vector_field_vtable const vtable = {
    .type = VECTOR_FIELD_TYPE_LVALUE,
    .type_name = "lvalue",

    .copy = lvalue_copy,
    .assign = lvalue_assign,
    .plus_assign = lvalue_plus_assign,

    .op_call = lvalue_call,

    .op_add = lvalue_add,
    .op_multiply = lvalue_multiply,
    .op_subtract = lvalue_sub,
    .op_negative = lvalue_negative,
    .op_divide = lvalue_divide,
    .op_power = lvalue_power,

    .op_bool = lvalue_bool,
    .op_comp = lvalue_comp,
    .op_contains = lvalue_contains,

    .op_index = lvalue_index,
    .op_attribute = lvalue_attribute,

    .hash = lvalue_hash,

    .bytes = lvalue_bytes,
    .free = NULL,

    .out_of_frame_like = 0,
};

struct vector_field_vtable const reference_vtable = {
    .type = VECTOR_FIELD_TYPE_LVALUE,
    .type_name = "lvalue reference",

    .copy = lvalue_copy,
    .assign = lvalue_reference_assign,
    .plus_assign = lvalue_reference_plus_assign,

    .op_call = lvalue_call,

    .op_add = lvalue_add,
    .op_multiply = lvalue_multiply,
    .op_subtract = lvalue_sub,
    .op_negative = lvalue_negative,
    .op_divide = lvalue_divide,
    .op_power = lvalue_power,

    .op_bool = lvalue_bool,
    .op_comp = lvalue_comp,
    .op_contains = lvalue_contains,

    .op_index = lvalue_index,
    .op_attribute = lvalue_attribute,

    .hash = lvalue_hash,

    .bytes = lvalue_bytes,
    .free = NULL,

    .out_of_frame_like = 1,
};

static struct vector_field_vtable const parameter_vtable = {
    .type = VECTOR_FIELD_TYPE_LVALUE,
    .type_name = "lvalue reference parameter",

    .copy = lvalue_copy,
    .assign = lvalue_parameter_assign,
    .plus_assign = NULL,

    .op_call = lvalue_call,

    .op_add = lvalue_add,
    .op_multiply = lvalue_multiply,
    .op_subtract = lvalue_sub,
    .op_negative = lvalue_negative,
    .op_divide = lvalue_divide,
    .op_power = lvalue_power,

    .op_bool = lvalue_bool,
    .op_comp = lvalue_comp,
    .op_contains = lvalue_contains,

    .op_index = lvalue_index,
    .op_attribute = lvalue_attribute,

    .hash = lvalue_hash,

    .bytes = lvalue_bytes,
    .free = NULL,

    .out_of_frame_like = 0,
};

static struct vector_field_vtable const persistent_vtable = {
    .type = VECTOR_FIELD_TYPE_LVALUE,
    .type_name = "persistent lvalue",

    .copy = lvalue_copy,
    .assign = NULL,
    .plus_assign = NULL,

    .op_call = lvalue_call,

    .op_add = lvalue_add,
    .op_multiply = lvalue_multiply,
    .op_subtract = lvalue_sub,
    .op_negative = lvalue_negative,
    .op_divide = lvalue_divide,
    .op_power = lvalue_power,

    .op_bool = lvalue_bool,
    .op_comp = lvalue_comp,
    .op_contains = lvalue_contains,

    .op_index = persistent_lvalue_index,
    .op_attribute = persistent_lvalue_attribute,

    .hash = lvalue_hash,

    .bytes = lvalue_bytes,
    .free = NULL,

    .out_of_frame_like = 1,
};

struct vector_field_vtable const derived_persistent_vtable = {
    .type = VECTOR_FIELD_TYPE_LVALUE,
    .type_name = "derived persistent lvalue",

    .copy = lvalue_copy,
    .assign = NULL,
    .plus_assign = NULL,

    .op_call = lvalue_call,

    .op_add = lvalue_add,
    .op_multiply = lvalue_multiply,
    .op_subtract = lvalue_sub,
    .op_negative = lvalue_negative,
    .op_divide = lvalue_divide,
    .op_power = lvalue_power,

    .op_bool = lvalue_bool,
    .op_comp = lvalue_comp,
    .op_contains = lvalue_contains,

    .op_index = persistent_lvalue_index,
    .op_attribute = persistent_lvalue_attribute,

    .hash = lvalue_hash,

    .bytes = lvalue_bytes,
    .free = NULL,

    .out_of_frame_like = 1,
};

static struct vector_field_vtable const const_vtable = {
    .type = VECTOR_FIELD_TYPE_LVALUE,
    .type_name = "constant lvalue",

    .copy = lvalue_copy,
    .assign = NULL,
    .plus_assign = NULL,

    .op_call = lvalue_call,

    .op_add = lvalue_add,
    .op_multiply = lvalue_multiply,
    .op_subtract = lvalue_sub,
    .op_negative = lvalue_negative,
    .op_divide = lvalue_divide,
    .op_power = lvalue_power,

    .op_bool = lvalue_bool,
    .op_comp = lvalue_comp,
    .op_contains = lvalue_contains,

    .op_index = const_lvalue_index,
    .op_attribute = const_lvalue_attribute,

    .hash = lvalue_hash,

    .bytes = lvalue_bytes,
    .free = NULL,

    .out_of_frame_like = 0,
};

struct vector_field
lvalue_init(
    struct timeline_execution_context *executor, struct vector_field *main
)
{
    return (struct vector_field){
        .value = { .pointer = main },
        .vtable = &vtable,
    };
}

struct vector_field
lvalue_const_init(
    struct timeline_execution_context *executor, struct vector_field *main
)
{
    return (struct vector_field){
        .value = { .pointer = main },
        .vtable = &const_vtable,
    };
}

struct vector_field
lvalue_persistent_init(
    struct timeline_execution_context *executor, struct vector_field *main
)
{
    return (struct vector_field){
        .value = { .pointer = main },
        .vtable = &persistent_vtable,
    };
}

struct vector_field
lvalue_parameter_init(
    struct timeline_execution_context *executor, struct vector_field *main
)
{
    return (struct vector_field){
        .value = { .pointer = main },
        .vtable = &parameter_vtable,
    };
}

struct vector_field
lvalue_reference_init(
    struct timeline_execution_context *executor, struct vector_field *main
)
{
    return (struct vector_field){
        .value = { .pointer = main },
        .vtable = &reference_vtable,
    };
}

struct vector_field
lvalue_copy(
    struct timeline_execution_context *executor, struct vector_field lvalue
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable || !dst->vtable->copy) {
        VECTOR_FIELD_ERROR(executor, "Unable to copy field (likely uninitialized)");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_COPY(executor, *dst);
}

struct vector_field
lvalue_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (dst->vtable && dst->vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        VECTOR_FIELD_BINARY(executor, *dst, assign, source);
        return lvalue;
    }

    struct vector_field const src =
        vector_field_lvalue_unwrap(executor, source);

    if (!src.vtable) {
        return VECTOR_FIELD_NULL;
    }
    else if (src.vtable->type & VECTOR_FIELD_TYPE_FUNCTION) {
        VECTOR_FIELD_FREE(executor, src);
        VECTOR_FIELD_ERROR(executor, "Cannot assign from function");
        return VECTOR_FIELD_NULL;
    }

    VECTOR_FIELD_FREE(executor, *dst);
    *dst = src;

    *source = VECTOR_FIELD_NULL;

    return lvalue;
}

struct vector_field
lvalue_plus_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
)
{
    if (!source->vtable) {
        VECTOR_FIELD_ERROR(executor, "Cannot assign from uninitialized field");
        return VECTOR_FIELD_NULL;
    }

    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable) {
        VECTOR_FIELD_ERROR(executor, "Cannot add to uninitialized field");
        return VECTOR_FIELD_NULL;
    }

    if (dst->vtable->plus_assign &&
        !(dst->vtable->type & VECTOR_FIELD_TYPE_LVALUE)) {
        if (!VECTOR_FIELD_BINARY(executor, *dst, plus_assign, source).vtable) {
            return VECTOR_FIELD_NULL;
        }
    }
    else if (dst->vtable->op_add) {
        struct vector_field const tmp =
            VECTOR_FIELD_BINARY(executor, *dst, op_add, source);
        if (!tmp.vtable) {
            return VECTOR_FIELD_NULL;
        }

        *dst = tmp;
    }
    else {
        VECTOR_FIELD_ERROR(executor, "Cannot perform add in place");
        return VECTOR_FIELD_NULL;
    }

    return lvalue;
}

/* does not unravel lvalues*/
struct vector_field
lvalue_parameter_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
)
{
    struct vector_field const src = *source;
    struct vector_field *const dst = lvalue.value.pointer;

    if (!src.vtable) {
        return VECTOR_FIELD_NULL;
    }
    else if (src.vtable->type & VECTOR_FIELD_TYPE_FUNCTION) {
        VECTOR_FIELD_ERROR(executor, "Cannot assign from function");
        return VECTOR_FIELD_NULL;
    }

    VECTOR_FIELD_FREE(executor, *dst);
    *dst = src;

    *source = VECTOR_FIELD_NULL;

    return lvalue;
}

struct vector_field
lvalue_reference_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (dst->vtable->assign) {
        VECTOR_FIELD_BINARY(executor, *dst, assign, source);
    }
    else {
        VECTOR_FIELD_ERROR(executor, "Invalid assignment to reference field");
        return VECTOR_FIELD_NULL;
    }
    return lvalue;
}

mc_count_t
lvalue_bytes(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    /* only include bytes of owned memory*/
    return sizeof(field);
}

struct vector_field
lvalue_reference_plus_assign(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *source
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (dst->vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        VECTOR_FIELD_BINARY(executor, *dst, plus_assign, source);
        return lvalue;
    }
    else {
        VECTOR_FIELD_ERROR(executor, "Unable to += field");
        return VECTOR_FIELD_NULL;
    }
}

void
lvalue_call(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    mc_count_t count, struct vector_field *args
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable || !dst->vtable->op_call) {
        VECTOR_FIELD_ERROR(executor, "Unable to call field");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    dst->vtable->op_call(executor, *dst, count, args);
}

struct vector_field
lvalue_add(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field *const dst = lhs.value.pointer;
    if (!dst->vtable || !dst->vtable->op_add) {
        VECTOR_FIELD_ERROR(executor, "Unable to add field");
        return VECTOR_FIELD_NULL;
    }

    /* necessary for vector adds */
#pragma message(                                                                   \
    "TODO memory model shouldn't be this inconsistent, need something much better" \
)
    lhs = VECTOR_FIELD_COPY(executor, *dst);
    return VECTOR_FIELD_BINARY(executor, lhs, op_add, rhs);
}

struct vector_field
lvalue_multiply(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field *const dst = lhs.value.pointer;
    if (!dst->vtable || !dst->vtable->op_multiply) {
        VECTOR_FIELD_ERROR(executor, "Unable to multiply field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, *dst, op_multiply, rhs);
}

struct vector_field
lvalue_sub(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field *const dst = lhs.value.pointer;
    if (!dst->vtable || !dst->vtable->op_subtract) {
        VECTOR_FIELD_ERROR(executor, "Unable to subtract field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, *dst, op_subtract, rhs);
}

struct vector_field
lvalue_divide(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field *const dst = lhs.value.pointer;
    if (!dst->vtable || !dst->vtable->op_divide) {
        VECTOR_FIELD_ERROR(executor, "Unable to divide field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, *dst, op_divide, rhs);
}

struct vector_field
lvalue_power(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field *const dst = lhs.value.pointer;
    if (!dst->vtable || !dst->vtable->op_power) {
        VECTOR_FIELD_ERROR(executor, "Unable to take power of field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, *dst, op_power, rhs);
}

struct vector_field
lvalue_negative(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    struct vector_field *const dst = field.value.pointer;
    if (!dst->vtable || !dst->vtable->op_negative) {
        VECTOR_FIELD_ERROR(executor, "Unable to take negative of field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_UNARY(executor, *dst, op_negative);
}

struct vector_field
lvalue_bool(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    struct vector_field *const dst = field.value.pointer;
    if (!dst->vtable || !dst->vtable->op_bool) {
        VECTOR_FIELD_ERROR(executor, "Unable to convert field to boolean");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_UNARY(executor, *dst, op_bool);
}

struct vector_field
lvalue_index(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *index
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable || !dst->vtable->op_index) {
        VECTOR_FIELD_ERROR(executor, "Field not indexable");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, *dst, op_index, index);
}

struct vector_field
const_lvalue_index(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *index
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable || !dst->vtable->op_index) {
        VECTOR_FIELD_ERROR(executor, "Field not indexable");
        return VECTOR_FIELD_NULL;
    }

    struct vector_field ret =
        VECTOR_FIELD_BINARY(executor, *dst, op_index, index);
    if (ret.vtable) {
        ret.vtable = &const_vtable;
    }

    return ret;
}

struct vector_field
persistent_lvalue_index(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *index
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable || !dst->vtable->op_index) {
        VECTOR_FIELD_ERROR(executor, "Field not indexable");
        return VECTOR_FIELD_NULL;
    }

    struct vector_field ret =
        VECTOR_FIELD_BINARY(executor, *dst, op_index, index);
    if (ret.vtable) {
        ret.vtable = &derived_persistent_vtable;
    }

    return ret;
}

struct vector_field
lvalue_attribute(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    char const *attribute
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable || !dst->vtable->op_attribute) {
        VECTOR_FIELD_ERROR(executor, "Field does not have attributes");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, *dst, op_attribute, attribute);
}

struct vector_field
const_lvalue_attribute(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    char const *attribute
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable || !dst->vtable->op_attribute) {
        VECTOR_FIELD_ERROR(executor, "Field does not have attributes");
        return VECTOR_FIELD_NULL;
    }

    struct vector_field ret =
        VECTOR_FIELD_BINARY(executor, *dst, op_attribute, attribute);
    if (ret.vtable) {
        ret.vtable = &const_vtable;
    }

    return ret;
}

struct vector_field
persistent_lvalue_attribute(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    char const *attribute
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable || !dst->vtable->op_attribute) {
        VECTOR_FIELD_ERROR(executor, "Field does not have attributes");
        return VECTOR_FIELD_NULL;
    }

    struct vector_field ret =
        VECTOR_FIELD_BINARY(executor, *dst, op_attribute, attribute);
    /* make an auxillary node that points outside, and that makes everything
     * right?... */
    if (ret.vtable) {
        ret.vtable = &derived_persistent_vtable;
    }

    return ret;
}

struct vector_field
lvalue_comp(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *rhs
)
{
    struct vector_field *const dst = lvalue.value.pointer;

    if (!dst->vtable) {
        VECTOR_FIELD_ERROR(executor, "Cannot compare uninitialized field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, *dst, op_comp, rhs);
}

struct vector_field
lvalue_contains(
    struct timeline_execution_context *executor, struct vector_field lvalue,
    struct vector_field *element
)
{
    struct vector_field *const dst = lvalue.value.pointer;

    if (!dst->vtable || !dst->vtable->op_contains) {
        VECTOR_FIELD_ERROR(
            executor, "Cannot iterate over field; not a container"
        );
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, *dst, op_contains, element);
}

mc_hash_t
lvalue_hash(
    struct timeline_execution_context *executor, struct vector_field lvalue
)
{
    struct vector_field *const dst = lvalue.value.pointer;
    if (!dst->vtable) {
        VECTOR_FIELD_ERROR(executor, "Uninitialized field");
        return 0;
    }

    return VECTOR_FIELD_UNARY(executor, *dst, hash);
}
