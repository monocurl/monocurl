//
//  functor.c
//  Monocurl
//
//  Created by Manu Bhat on 12/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "functor.h"
#include "lvalue.h"
#include "primitives.h"

static struct vector_field_vtable const vtable = {
    .type = VECTOR_FIELD_TYPE_FUNCTOR,
    .type_name = "functor",

    .copy = functor_copy,
    .assign = NULL,
    .plus_assign = NULL,

    .op_call = NULL,

    .op_add = functor_add,
    .op_multiply = functor_multiply,
    .op_subtract = functor_sub,
    .op_negative = functor_negative,
    .op_divide = functor_divide,
    .op_power = functor_power,

    .op_bool = functor_bool,
    .op_contains = functor_contains,
    .op_comp = functor_comp,

    .op_index = functor_index,
    .op_attribute = functor_attribute,

    .hash = functor_hash,

    .bytes = functor_bytes,
    .free = functor_free,

    .out_of_frame_like = 0,
};

static struct shared_vector_field *
shared_init(struct vector_field val)
{
    struct shared_vector_field *ret =
        mc_malloc(sizeof(struct shared_vector_field));
    ret->ref_count = 1;
    ret->res = val;
    return ret;
}

static void
unref_shared(
    struct timeline_execution_context *executor,
    struct shared_vector_field *shared
)
{
    if (shared && !--shared->ref_count) {
        VECTOR_FIELD_FREE(executor, shared->res);
        mc_free(shared);
    }
}

struct vector_field
functor_get_res(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    struct functor *const functor = field.value.pointer;

    // first call from the arguments
    if (functor->force_const || !functor->dirty) {
        return functor->result->res;
    }

    functor->dirty = 0;
    for (mc_ind_t i = 0; i < functor->argument_count; ++i) {
        mc_hash_t hash;
        if (functor->arguments[i].dirty &&
            (hash = VECTOR_FIELD_HASH(executor, functor->arguments[i].field)) !=
                functor->arguments[i].last_hash) {
            if (!hash) {
                return VECTOR_FIELD_NULL;
            }
            functor->arguments[i].dirty = 0;
            functor->arguments[i].last_hash = hash;
            functor->dirty = 1;
        }
    }

    if (!functor->dirty) {
        /* it's possible a reference still exists and may need to be assigned */
        /* but for now we know nothing has changed */
        functor->dirty = 1;
        return functor->result->res;
    }

    mc_count_t const org = executor->stack_frame;
    for (mc_ind_t i = 0; i < functor->argument_count; i++) {
        if (!timeline_executor_var_push(
                executor, functor->arguments[i].field
            )) {
            return VECTOR_FIELD_NULL;
        }
    }

    function_call(
        executor, functor->function, functor->argument_count,
        &executor->stack[org]
    );

    executor->stack_frame -= functor->argument_count;

    functor->dirty = 0;

    unref_shared(executor, functor->result);
    struct vector_field new = vector_field_extract_type(
        executor, &executor->return_register, VECTOR_FIELD_PURE
    );
    functor->result = shared_init(new);
    executor->return_register = VECTOR_FIELD_NULL;
    return new;
}

/* only called when just about to free functor */
struct vector_field
functor_steal_res(
    struct timeline_execution_context *executor, struct vector_field functor
)
{
    struct functor *func = functor.value.pointer;
    struct vector_field ret = functor_get_res(executor, functor);
    if (func->result->ref_count > 1) {
        ret = VECTOR_FIELD_COPY(executor, ret);
    }
    else {
        mc_free(func->result);
        func->result = NULL;
        func->dirty = 1;
    }

    return ret;
}

// index and null fields should also be passed along
struct vector_field
functor_init(
    struct timeline_execution_context *executor, mc_count_t arg_c,
    struct vector_named_field *arguments, struct vector_field function,
    struct vector_field current, mc_bool_t force_const
)
{
    struct functor *const func = mc_malloc(sizeof(struct functor));
    func->dirty = 0;
    func->arguments = arguments;
    func->argument_count = arg_c;
    func->force_const = force_const;
    func->function = function;
    func->result = shared_init(current);

    executor->byte_alloc += arg_c;

    struct vector_field const ret = {
        .value = { .pointer = func },
        .vtable = &vtable,
    };

    return ret;
}

struct vector_field
functor_copy(
    struct timeline_execution_context *executor, struct vector_field functor
)
{
    struct functor *const src = functor.value.pointer;
    struct functor *const copy = mc_malloc(sizeof(struct functor));

    if (!src->dirty) {
        copy->result = src->result;
        copy->result->ref_count++;
    }
    else {
        copy->result =
            shared_init(VECTOR_FIELD_COPY(executor, src->result->res));
    }

    copy->force_const = src->force_const;
    copy->dirty = src->dirty;
    copy->function = function_copy(executor, src->function);
    copy->arguments =
        mc_malloc(sizeof(struct vector_named_field) * src->argument_count);
    copy->argument_count = src->argument_count;
    for (mc_ind_t i = 0; i < copy->argument_count; i++) {
        /* unowned pointer */
        copy->arguments[i].name = src->arguments[i].name;
        copy->arguments[i].field =
            vector_field_lvalue_copy(executor, src->arguments[i].field);
        copy->arguments[i].dirty = src->arguments[i].dirty;
        copy->arguments[i].last_hash = src->arguments[i].last_hash;
    }

    executor->byte_alloc += src->argument_count;

    struct vector_field const ret = (struct vector_field){
        .value = { .pointer = copy },
        .vtable = &vtable,
    };

    return ret;
}

struct vector_field
functor_add(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field res = functor_steal_res(executor, lhs);
    if (!res.vtable || !res.vtable->op_add) {
        VECTOR_FIELD_ERROR(
            executor, "Cannot add field (you might want to surround this "
                      "variable in a vector?)"
        );
        return VECTOR_FIELD_NULL;
    }

    functor_free(executor, lhs);
    return VECTOR_FIELD_BINARY(executor, res, op_add, rhs);
}

struct vector_field
functor_multiply(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field const res = functor_get_res(executor, lhs);
    if (!res.vtable || !res.vtable->op_multiply) {
        VECTOR_FIELD_ERROR(executor, "Cannot multiply field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, res, op_multiply, rhs);
}

struct vector_field
functor_sub(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field const res = functor_get_res(executor, lhs);
    if (!res.vtable || !res.vtable->op_subtract) {
        VECTOR_FIELD_ERROR(executor, "Cannot subtract field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, res, op_subtract, rhs);
}

struct vector_field
functor_divide(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field const res = functor_get_res(executor, lhs);
    if (!res.vtable || !res.vtable->op_divide) {
        VECTOR_FIELD_ERROR(executor, "Cannot divide field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, res, op_divide, rhs);
}

struct vector_field
functor_power(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field const res = functor_get_res(executor, lhs);
    if (!res.vtable || !res.vtable->op_power) {
        VECTOR_FIELD_ERROR(executor, "Cannot exponentiate field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, res, op_power, rhs);
}

struct vector_field
functor_negative(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    struct vector_field const res = functor_get_res(executor, field);
    if (!res.vtable || !res.vtable->op_negative) {
        VECTOR_FIELD_ERROR(executor, "Cannot take negative of field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_UNARY(executor, res, op_negative);
}

struct vector_field
functor_comp(
    struct timeline_execution_context *executor, struct vector_field functor,
    struct vector_field *rhs
)
{
    if (rhs->vtable->type & VECTOR_FIELD_TYPE_FUNCTOR) {
        struct functor *this = functor.value.pointer;
        struct functor *other = rhs->value.pointer;
        int ret;
        if ((ret = (int) this->argument_count - (int) other->argument_count)) {
            return double_init(executor, ret);
        }
        for (mc_ind_t i = 0; i < this->argument_count; ++i) {
            if ((ret = !!this->arguments[i].field.vtable -
                       !!other->arguments[i].field.vtable)) {
                return double_init(executor, ret);
            }
            if (this->arguments[i].field.vtable) {
                struct vector_field dret;
                if (VECTOR_FIELD_DBOOL(
                        dret = VECTOR_FIELD_BINARY(
                            executor, this->arguments[i].field, op_comp,
                            &other->arguments[i].field
                        )
                    ) ||
                    !dret.vtable) {
                    return dret;
                }
            }
        }

        return double_init(executor, 0);
    }
    else {
        struct vector_field const res = functor_get_res(executor, functor);
        if (!res.vtable || !res.vtable->op_comp) {
            VECTOR_FIELD_ERROR(executor, "Cannot compare field");
            return VECTOR_FIELD_NULL;
        }

        return VECTOR_FIELD_BINARY(executor, res, op_comp, rhs);
    }
}

struct vector_field
functor_index(
    struct timeline_execution_context *executor, struct vector_field functor,
    struct vector_field *index
)
{
#pragma message(                                                                               \
    "TODO, this should actually elide the functor entirely since its state is inconsistent..." \
)
    struct vector_field res = functor_get_res(executor, functor);
    if (!res.vtable || !res.vtable->op_index) {
        VECTOR_FIELD_ERROR(executor, "Cannot index field");
        return VECTOR_FIELD_NULL;
    }

    /* need this to be owned operation! */
    struct functor *func = functor.value.pointer;
    if (func->result->ref_count > 1) {
        func->result =
            shared_init(VECTOR_FIELD_COPY(executor, func->result->res));
        res = func->result->res;
    }

    return VECTOR_FIELD_BINARY(executor, res, op_index, index);
}

struct vector_field
functor_attribute(
    struct timeline_execution_context *executor, struct vector_field field,
    char const *attribute
)
{
    struct functor *const functor = field.value.pointer;
    if (functor->force_const) {
        VECTOR_FIELD_ERROR(
            executor,
            "Cannot read attributes of a functor with functional or reference "
            "parameters (this behavior may be changed in the future)"
        );
        return VECTOR_FIELD_NULL;
    }
    for (mc_ind_t i = 0; i < functor->argument_count; i++) {
        if (functor->arguments[i].name &&
            !strcmp(attribute, functor->arguments[i].name)) {
            functor->dirty = 1;
            functor->arguments[i].dirty = 1;
            return lvalue_init(executor, &functor->arguments[i].field);
        }
    }

    VECTOR_FIELD_ERROR(executor, "Functor attribute `%s` not found", attribute);
    return VECTOR_FIELD_NULL;
}

mc_count_t
functor_bytes(
    struct timeline_execution_context *executor, struct vector_field functor
)
{
    struct functor *f = functor.value.pointer;
    mc_count_t ret = sizeof(*f) + sizeof(functor);
    ret += VECTOR_FIELD_BYTES(executor, f->result->res);
    ret += VECTOR_FIELD_BYTES(executor, f->function);
    for (mc_ind_t i = 0; i < f->argument_count; ++i) {
        ret += VECTOR_FIELD_BYTES(executor, f->arguments[i].field);
    }

    return ret;
}

struct vector_field
functor_bool(
    struct timeline_execution_context *executor, struct vector_field lhs
)
{
    struct vector_field const res = functor_get_res(executor, lhs);
    if (!res.vtable || !res.vtable->op_bool) {
        VECTOR_FIELD_ERROR(executor, "Cannot coerce field to boolean");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_UNARY(executor, res, op_bool);
}

struct vector_field
functor_contains(
    struct timeline_execution_context *executor, struct vector_field lhs,
    struct vector_field *rhs
)
{
    struct vector_field const res = functor_get_res(executor, lhs);
    if (!res.vtable || !res.vtable->op_contains) {
        VECTOR_FIELD_ERROR(executor, "Cannot perform contains on field");
        return VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_BINARY(executor, res, op_contains, rhs);
}

mc_hash_t
functor_hash(
    struct timeline_execution_context *executor, struct vector_field functor
)
{
    struct vector_field const res = functor_get_res(executor, functor);
    if (!res.vtable || !res.vtable->hash) {
        VECTOR_FIELD_ERROR(executor, "Cannot hash field");
        return 0;
    }

    return VECTOR_FIELD_UNARY(executor, res, hash);
}

void
functor_free(
    struct timeline_execution_context *executor, struct vector_field functor
)
{
    struct functor *const func = functor.value.pointer;

    unref_shared(executor, func->result);

    function_free(executor, func->function);

    for (mc_ind_t i = 0; i < func->argument_count; i++) {
        VECTOR_FIELD_FREE(executor, func->arguments[i].field);
    }
    mc_free(func->arguments);

    executor->byte_alloc -= func->argument_count;

    mc_free(func);
}
