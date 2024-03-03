//
//  function.c
//  Monocurl
//
//  Created by Manu Bhat on 12/10/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>

#include "function.h"
#include "functor.h"
#include "lvalue.h"
#include "mc_types.h"
#include "primitives.h"

static struct vector_field_vtable const vtable = {
    .type = VECTOR_FIELD_TYPE_FUNCTION,
    .type_name = "function",

    .copy = function_copy,
    .assign = NULL,
    .plus_assign = NULL,

    .op_call = function_call,

    .op_add = NULL,
    .op_multiply = NULL,
    .op_subtract = NULL,
    .op_negative = NULL,
    .op_divide = NULL,
    .op_power = NULL,

    .op_bool = NULL,
    .op_contains = NULL,
    .op_comp = function_comp,

    .op_index = NULL,
    .op_attribute = NULL,

    .hash = function_hash,

    .bytes = function_bytes,
    .free = function_free,

    .out_of_frame_like = 0,
};

struct vector_field
function_init(
    struct timeline_execution_context *executor,
    struct timeline_instruction *head, mc_count_t cache_count,
    struct vector_field *caches
)
{
    struct function *const function = mc_malloc(sizeof(struct function));
    function->head = head;
    function->cache_count = cache_count;
    function->caches = caches;

    return (struct vector_field){
        .value = { .pointer = function },
        .vtable = &vtable,
    };
}

struct vector_field
function_copy(
    struct timeline_execution_context *executor, struct vector_field func
)
{
    struct function *const src = func.value.pointer;
    struct function *const function = mc_malloc(sizeof(struct function));
    function->head = src->head;
    function->hash_cache = src->hash_cache;

    function->cache_count = src->cache_count;
    function->caches =
        mc_malloc(sizeof(struct vector_field) * src->cache_count);
    for (mc_ind_t i = 0; i < src->cache_count; i++) {
        function->caches[i] = VECTOR_FIELD_COPY(executor, src->caches[i]);
    }

    return (struct vector_field){
        .value = { .pointer = function },
        .vtable = &vtable,
    };
}

mc_hash_t
function_hash(
    struct timeline_execution_context *executor, struct vector_field func
)
{
    struct function *const function = func.value.pointer;
    if (function->hash_cache) {
        return function->hash_cache;
    }

    mc_hash_t hash = function->cache_count + (mc_hash_t) function->head;
    for (mc_ind_t i = 0; i < function->cache_count; i++) {
        /* Golden Ratio * (1 << 32) */
        if (function->caches[i].vtable) {
            /* caches may be null in the case of unused branches */

            mc_hash_t const sub =
                VECTOR_FIELD_HASH(executor, function->caches[i]);
            if (!sub) {
                return 0;
            }
            hash ^= 0x9e3779b9 + i + sub + (hash << 16) + (hash >> 12);
        }
    }

    return function->hash_cache = hash;
}

struct vector_field
function_comp(
    struct timeline_execution_context *executor, struct vector_field func,
    struct vector_field *rhs
)
{
    struct vector_field rhs_val = vector_field_safe_extract_type(
        executor, *rhs, VECTOR_FIELD_TYPE_FUNCTION
    );

    int ret;
    if (!rhs_val.vtable) {
        return double_init(executor, 1);
    }
    else if ((ret = (int) rhs_val.vtable->type -
                    (int) VECTOR_FIELD_TYPE_FUNCTION)) {
        return double_init(executor, ret);
    }

    struct function *const function = func.value.pointer;
    struct function *const rhs_v = rhs_val.value.pointer;

    if (function->head > rhs_v->head) {
        return double_init(executor, 1);
    }
    else if (function->head < rhs_v->head) {
        return double_init(executor, -1);
    }

    struct vector_field vret;
    for (mc_ind_t i = 0; i < function->cache_count; i++) {
        if (i >= rhs_v->cache_count) {
            return double_init(executor, 1);
        }
        if (VECTOR_FIELD_DBOOL(
                vret = VECTOR_FIELD_BINARY(
                    executor, function->caches[i], op_comp, &rhs_v->caches[i]
                )
            )) {
            return vret;
        }
    }

    return double_init(
        executor, (function->cache_count == rhs_v->cache_count) ? 0 : -1
    );
}

void
function_call(
    struct timeline_execution_context *executor, struct vector_field v,
    mc_count_t count, struct vector_field *args
)
{
    if (!v.vtable) {
        VECTOR_FIELD_ERROR(executor, "Cannot call uninitialized function");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct function *const func = v.value.pointer;
    for (long long i = (long long) func->cache_count - 1; i >= 0; --i) {
        if (!timeline_executor_var_push(executor, func->caches[i])) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }

    /* recursion handle */
    if (!timeline_executor_var_push(executor, v)) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    /* functor call */
    for (mc_ind_t i = 0; i < count; ++i) {
        if (!timeline_executor_var_push(executor, args[i])) {
            /* unravel so we don't free the function twice*/
            executor->stack_frame -= i;
            --executor->stack_frame;
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }

    /* avoid dereferencing memory after a potential interrupt */
    mc_count_t const cache_count = func->cache_count;
    executor->return_register =
        timeline_executor_execute(executor, func->head, 1);

    /* executor->return_register =
        vector_field_functor_elide(executor, &executor->return_register); */

    for (mc_ind_t i = 0; i < count; ++i) {
        args[i] = executor->stack[executor->stack_frame - 1 - i];
    }

    executor->stack_frame -=
        cache_count + count + 1; /* caches, handle, and arguments shouldn't be
                                    freed either, unless caller wants to */
}

mc_count_t
function_bytes(
    struct timeline_execution_context *executor, struct vector_field v
)
{
    struct function *f = v.value.pointer;
    mc_count_t ret = sizeof(v) + sizeof(*f);
    for (mc_ind_t i = 0; i < f->cache_count; ++i) {
        ret += VECTOR_FIELD_BYTES(executor, f->caches[i]);
    }

    return ret;
}

void
function_free(
    struct timeline_execution_context *executor, struct vector_field function
)
{
    struct function *const func = function.value.pointer;

    for (mc_ind_t i = 0; i < func->cache_count; i++) {
        VECTOR_FIELD_FREE(executor, func->caches[i]);
    }
    mc_free(func->caches);

    mc_free(func);
}
