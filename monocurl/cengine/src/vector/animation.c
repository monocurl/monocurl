//
//  animation.c
//  Monocurl
//
//  Created by Manu Bhat on 12/27/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>

#include "animation.h"
#include "function.h"
#include "lvalue.h"
#include "primitives.h"

static struct vector_field_vtable const vtable = {
    .type = VECTOR_FIELD_TYPE_ANIMATION,
    .type_name = "animation",

    .copy = animation_copy,
    .assign = NULL,
    .plus_assign = NULL,

    .op_call = NULL,

    .op_add = NULL,
    .op_multiply = NULL,
    .op_subtract = NULL,
    .op_negative = NULL,
    .op_divide = NULL,
    .op_power = NULL,

    .op_bool = NULL,
    .op_contains = NULL,
    .op_comp = animation_comp,

    .op_index = NULL,
    .op_attribute = NULL,

    .hash = animation_hash,

    .bytes = animation_bytes,
    .free = animation_free,

    .out_of_frame_like = 0,
};

static struct vector_field
capture_vec(
    struct timeline_execution_context *executor, struct vector_field tree
)
{
    if (tree.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        if (tree.vtable == &reference_vtable) {
            return capture_vec(
                executor, *(struct vector_field *) tree.value.pointer
            );
        }
        else {
            struct vector_field *const follower = timeline_get_follower(
                executor, (struct vector_field *) tree.value.pointer
            );
            if (!follower) {
                VECTOR_FIELD_ERROR(
                    executor,
                    "Invalid variable for animation. Expected a screen "
                    "variable (e.g. `tree main`) or a scene variable "
                    "such as `camera` or `background`"
                );
                return VECTOR_FIELD_NULL;
            }

            return lvalue_init(executor, follower);
        }
    }
    else {
        struct vector_field ret = vector_init(executor);

        struct vector *const vec = tree.value.pointer;
        for (mc_ind_t i = 0; i < vec->field_count; ++i) {
            struct vector_field sub = capture_vec(executor, vec->fields[i]);
            vector_literal_plus(executor, ret, &sub);
        }

        return ret;
    }
}

/* ok so we need to eliminate reference vtables entirely... */
static struct vector_field
capture_copy(
    struct timeline_execution_context *executor, struct vector_field tree
)
{
    if (!tree.vtable) {
        return tree;
    }

    if (tree.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        return tree;
    }
    else {
        struct vector *const vec = tree.value.pointer;
        struct vector_field const ret = vector_init(executor);
        for (mc_ind_t i = 0; i < vec->field_count; ++i) {
            struct vector_field rhs = capture_copy(executor, vec->fields[i]);
            vector_literal_plus(executor, ret, &rhs);
        }

        return ret;
    }
}

struct vector_field
animation_init(
    struct timeline_execution_context *executor, struct vector_field pull_state,
    struct vector_field push_state, struct vector_field sentinel
)
{
    struct animation *const animation = mc_malloc(sizeof(struct animation));

    animation->pull_state = capture_vec(executor, pull_state);
    animation->push_state = capture_vec(executor, push_state);

    animation->sentinel = VECTOR_FIELD_COPY(executor, sentinel);

    animation->sticky = 0;
    animation->time = -FLT_EPSILON;
    animation->sentinel_state = ANIMATION_UNPLAYED;

    animation->slide = executor->execution_slide;
    animation->line = executor->execution_line;

    animation->hash_cache = 0;

    return (struct vector_field){
        .value = { .pointer = animation },
        .vtable = &vtable,
    };
}

struct vector_field
animation_sticky_init(struct timeline_execution_context *executor)
{
    struct animation *const animation = mc_malloc(sizeof(struct animation));

    animation->pull_state = VECTOR_FIELD_NULL;
    animation->push_state = VECTOR_FIELD_NULL;

    animation->sentinel = VECTOR_FIELD_NULL;

    animation->sticky = 1;
    animation->time = -FLT_EPSILON;
    animation->sentinel_state = ANIMATION_UNPLAYED;

    animation->slide = executor->execution_slide;
    animation->line = executor->execution_line;

    animation->hash_cache = 0;

    return (struct vector_field){
        .value = { .pointer = animation },
        .vtable = &vtable,
    };
}

struct vector_field
animation_copy(
    struct timeline_execution_context *executor, struct vector_field source
)
{
    struct animation *const src = source.value.pointer;
    struct animation *const animation = mc_malloc(sizeof(struct animation));

    animation->pull_state = capture_copy(executor, src->pull_state);
    animation->push_state = capture_copy(executor, src->push_state);

    animation->sentinel = VECTOR_FIELD_COPY(executor, src->sentinel);

    animation->sticky = src->sticky;
    animation->time = src->time;
    animation->sentinel_state = src->sentinel_state;

    animation->slide = src->slide;
    animation->line = src->line;

    animation->hash_cache = src->hash_cache;

    return (struct vector_field){
        .value = { .pointer = animation },
        .vtable = &vtable,
    };
}

mc_hash_t
animation_hash(
    struct timeline_execution_context *executor, struct vector_field a
)
{
    struct animation *const animation = a.value.pointer;
    if (animation->hash_cache) {
        return animation->hash_cache;
    }

    mc_hash_t hash = (mc_hash_t) animation->sticky;
    mc_hash_t sub = VECTOR_FIELD_HASH(executor, animation->push_state);
    if (!sub) {
        return 0;
    }
    hash ^= 0x9e3779b9 + sub + (hash << 16) + (hash >> 12);
    sub = VECTOR_FIELD_HASH(executor, animation->pull_state);
    if (!sub) {
        return 0;
    }
    hash ^= 0x9e3779b9 + sub + (hash << 16) + (hash >> 12);
    hash ^=
        0x9e3779b9 + animation->sentinel_state + (hash << 16) + (hash >> 12);
    sub = VECTOR_FIELD_HASH(executor, animation->sentinel);
    if (!sub) {
        return 0;
    }
    hash ^= 0x9e3779b9 + sub + (hash << 16) + (hash >> 12);

    /* time is not hashed, since doubles are weird */

    return animation->hash_cache = hash;
}

mc_count_t
animation_bytes(
    struct timeline_execution_context *executor, struct vector_field source
)
{
    struct animation *a = source.value.pointer;
    mc_count_t ret = sizeof(*a) + sizeof(source);
    ret += VECTOR_FIELD_BYTES(executor, a->pull_state);
    ret += VECTOR_FIELD_BYTES(executor, a->push_state);
    ret += VECTOR_FIELD_BYTES(executor, a->sentinel);
    return ret;
}

struct vector_field
animation_comp(
    struct timeline_execution_context *executor, struct vector_field source,
    struct vector_field *rhs
)
{
    struct vector_field rhs_val = vector_field_safe_extract_type(
        executor, *rhs, VECTOR_FIELD_TYPE_ANIMATION
    );

    long long ret;
    if (!rhs_val.vtable) {
        return double_init(executor, 1);
    }
    else if ((ret =
                  ((int) rhs_val.vtable->type -
                   (int) VECTOR_FIELD_TYPE_ANIMATION))) {
        return double_init(executor, (double) ret);
    }

    struct animation *const animation = source.value.pointer;
    struct animation *const ranimation = rhs_val.value.pointer;

    struct vector_field vret;
    if (VECTOR_FIELD_DBOOL(
            vret = function_comp(
                executor, animation->sentinel, &ranimation->sentinel
            )
        ) || !vret.vtable) {
        return vret;
    }
    if ((ret = animation->sticky - ranimation->sticky)) {
        return double_init(executor, (double) ret);
    }
    if (animation->push_state.value.hash - ranimation->push_state.value.hash) {
        return double_init(
            executor, (double) (animation->push_state.value.hash -
                                ranimation->push_state.value.hash)
        );
    }
    if (animation->pull_state.value.hash - ranimation->pull_state.value.hash) {
        return double_init(
            executor, (double) (animation->pull_state.value.hash -
                                ranimation->pull_state.value.hash)
        );
    }
    if (animation->time != ranimation->time) {
        return double_init(executor, animation->time - ranimation->time);
    }
    if (animation->sentinel_state != ranimation->sentinel_state) {
        return double_init(
            executor,
            (int) animation->sentinel_state - (int) ranimation->sentinel_state
        );
    }

    return double_init(executor, 0);
}

static mc_status_t
really_step(
    struct timeline_execution_context *executor, struct animation *animation,
    double dt
)
{
    if (animation->sticky) {
        animation->sentinel_state = ANIMATION_SENTINEL_FORCE;
        animation->time = 0;
        return MC_STATUS_SUCCESS;
    }

    mc_count_t const line = executor->execution_line;
    executor->execution_line = animation->line;

    /* negative epsilon only for the first frame */
    animation->time += dt;

    /* t, dt, state in, state b */
    mc_count_t const org = executor->stack_frame;
    if (!timeline_executor_var_push(
            executor, double_init(executor, animation->time)
        )) {
        goto error;
    }
    if (!timeline_executor_var_push(executor, double_init(executor, dt))) {
        goto error;
    }
    if (!timeline_executor_var_push(executor, animation->pull_state)) {
        goto error;
    }
    if (!timeline_executor_var_push(executor, animation->push_state)) {
        goto error;
    }

    if (animation->time < 0) {
        animation->time = 0;
    }

    function_call(executor, animation->sentinel, 4, &executor->stack[org]);

    executor->stack_frame -= 4;

    struct vector_field const ret = vector_field_extract_type(
        executor, &executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
    );
    executor->return_register = VECTOR_FIELD_NULL;

    if (!ret.vtable) {
        goto error;
    }
    else if (ret.value.doub > 0) {
        animation->sentinel_state = ANIMATION_SENTINEL_FORCE;
    }
    else if (ret.value.doub == 0) {
        animation->sentinel_state = ANIMATION_PLAYING;
    }
    else {
        animation->sentinel_state = ANIMATION_SENTINEL_IF_OTHERS;
    }

    executor->execution_line = line;

    return MC_STATUS_SUCCESS;

error:
    executor->execution_line = line;
    animation->sentinel_state = ANIMATION_SENTINEL_FORCE;
    return MC_STATUS_FAIL;
}

mc_status_t
animation_step(
    struct timeline_execution_context *executor, struct animation *animation,
    double dt
)
{
    if (animation->time < 0) {
        if (really_step(executor, animation, 0) != MC_STATUS_SUCCESS) {
            return MC_STATUS_FAIL;
        }
        else if (animation->sentinel_state == ANIMATION_SENTINEL_FORCE) {
            return MC_STATUS_SUCCESS;
        }
    }

    return really_step(executor, animation, dt);
}

void
animation_free(
    struct timeline_execution_context *executor, struct vector_field a
)
{
    struct animation *const animation = a.value.pointer;

    VECTOR_FIELD_FREE(executor, animation->pull_state);
    VECTOR_FIELD_FREE(executor, animation->push_state);

    VECTOR_FIELD_FREE(executor, animation->sentinel);

    mc_free(animation);
}
