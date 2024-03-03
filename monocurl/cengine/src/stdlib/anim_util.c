//
//  mc_interpolation.c
//  Monocurl
//
//  Created by Manu Bhat on 2/19/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <string.h>

#include "anim_invariants.h"
#include "anim_util.h"
#include "mc_util.h"

/*
 func lerp(a, b, t) = native lerp(a, b, t)
 func linear(t) = t
 func smooth([args] {[default] {t}, [parameterized] {t, rate_in, rate_out}}) =
 native smooth(args, t, rate_in, rate_out) func smooth_in([args] {[default] {t},
 [parameterized] {t, rate}}) = native smooth_in(args, t, rate) func
 smooth_out([args] {[default] {t}, [parameterized] {t, rate}}) = native
 smooth_out(args, t, rate)
 */

double
anim_smooth(double t)
{
    /* straight from manimgl */
    double const s = 1 - t;
    return (t * t * t) * (10 * s * s + 5 * s * t + t * t);
}

static mc_status_t
lerp_recurse(
    struct timeline_execution_context *executor, struct vector_field const a,
    struct vector_field *dump, struct vector_field b, float t
)
{
    enum vector_field_type const non_lvalue =
        VECTOR_FIELD_PURE | VECTOR_FIELD_TYPE_FUNCTOR;
    struct vector_field alpha =
        vector_field_nocopy_extract_type(executor, a, non_lvalue);
    struct vector_field beta =
        vector_field_nocopy_extract_type(executor, b, non_lvalue);

    /* match types, if both are functors, keep */
    /* if one is functor wrapping the type of other, extrude down */
    alpha = vector_field_nocopy_extract_type_message(
        executor, alpha, beta.vtable->type,
        "Could not ensure value (or subvalue) was of matching types for start "
        "and end. A type: %s "
        "B type: %s"
    );
    if (!alpha.vtable) {
        return MC_STATUS_FAIL;
    }

    beta = vector_field_nocopy_extract_type_message(
        executor, beta, alpha.vtable->type,
        "Could not ensure value (or subvalue) was of matching types for start "
        "and end. B type: %s "
        "A type: %s"
    );
    if (!beta.vtable) {
        return MC_STATUS_FAIL;
    }

    /* guaranteed success */
    vector_field_extract_type(executor, dump, alpha.vtable->type);

    if (!VECTOR_FIELD_DBOOL(VECTOR_FIELD_BINARY(executor, alpha, op_comp, &beta)
        )) {
        return MC_STATUS_SUCCESS;
    }
    else if (alpha.vtable->type & VECTOR_FIELD_TYPE_FUNCTOR) {
        struct functor *const alpha_func = alpha.value.pointer;
        struct functor *const dump_func = dump->value.pointer;
        struct functor *const beta_func = beta.value.pointer;

        /* if the same functor, argument order will be the exact same */
        if (alpha_func->argument_count != beta_func->argument_count) {
            VECTOR_FIELD_ERROR(
                executor, "Cannot interpolate two functors with different "
                          "argument counts!"
            );
            return MC_STATUS_FAIL;
        }

        if (alpha_func->force_const || beta_func->force_const) {
            VECTOR_FIELD_ERROR(
                executor,
                "Cannot interpolate functors with a reference argument!"
            );
            return MC_STATUS_FAIL;
        }

        for (mc_ind_t i = 0; i < dump_func->argument_count; ++i) {
            if (!alpha_func->arguments[i].name &&
                !beta_func->arguments[i].name) {
                continue;
            }

            if (!alpha_func->arguments[i].name ||
                !beta_func->arguments[i].name ||
                strcmp(
                    alpha_func->arguments[i].name, beta_func->arguments[i].name
                )) {
                VECTOR_FIELD_ERROR(
                    executor,
                    "Cannot interpolate functor arguments with different names "
                    "(i.e. `%s` to `%s`)",
                    alpha_func->arguments[i].name
                        ? alpha_func->arguments[i].name
                        : "null",
                    beta_func->arguments[i].name ? beta_func->arguments[i].name
                                                 : "null"
                );
                return MC_STATUS_FAIL;
            }

            if (lerp_recurse(
                    executor, alpha_func->arguments[i].field,
                    &dump_func->arguments[i].field,
                    beta_func->arguments[i].field, t
                ) != MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }

            dump_func->arguments[i].dirty = 1;
        }
        dump_func->dirty = 1;

        return MC_STATUS_SUCCESS;
    }
    else if (alpha.vtable->type & VECTOR_FIELD_TYPE_DOUBLE) {
        *dump = double_init(
            executor, alpha.value.doub * (1 - t) + beta.value.doub * t
        );
        return MC_STATUS_SUCCESS;
    }
    else if (alpha.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const alpha_vec = alpha.value.pointer;
        struct vector *const dump_vec = dump->value.pointer;
        struct vector *const beta_vec = beta.value.pointer;
        if (alpha_vec->field_count != beta_vec->field_count) {
            VECTOR_FIELD_ERROR(
                executor,
                "Cannot interpolate vector with %zu elements to  vector with "
                "%zu elements",
                alpha_vec->field_count, beta_vec->field_count
            );
            return MC_STATUS_FAIL;
        }

        for (mc_ind_t i = 0; i < alpha_vec->field_count; ++i) {
            if (lerp_recurse(
                    executor, alpha_vec->fields[i], &dump_vec->fields[i],
                    beta_vec->fields[i], t
                ) != MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }
        }

        return MC_STATUS_SUCCESS;
    }
    else if (alpha.vtable->type & VECTOR_FIELD_TYPE_MAP) {
        struct map *const alpha_map = alpha.value.pointer;
        struct map *const dump_map = dump->value.pointer;
        struct map *const beta_map = beta.value.pointer;

        if (alpha_map->field_count != beta_map->field_count) {
            VECTOR_FIELD_ERROR(
                executor,
                "Cannot interpolate vector with %zu elements to  vector with "
                "%zu elements",
                alpha_map->field_count, beta_map->field_count
            );
            return MC_STATUS_FAIL;
        }

        for (struct map_node *head = alpha_map->head.next_ins,
                             *root = dump_map->head.next_ins;
             head; head = head->next_ins, root = root->next_ins) {
            if (!VECTOR_FIELD_DBOOL(map_contains(executor, beta, &head->field)
                )) {
                VECTOR_FIELD_ERROR(
                    executor,
                    "Cannot interpolate maps that do not have the same keyset"
                );
                return MC_STATUS_FAIL;
            }
            struct vector_field const lvalue =
                map_index(executor, beta, &head->field);

            if (lerp_recurse(executor, head->value, &root->value, lvalue, t) !=
                MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }
        }

        return MC_STATUS_SUCCESS;
    }
    else {
        char buffer[VECTOR_FIELD_TYPE_STR_BUFFER];
        vector_field_type_to_a(alpha.vtable->type, buffer);
        VECTOR_FIELD_ERROR(
            executor, "Cannot interpolate value (or subvalue) of type %s",
            buffer
        );
        return MC_STATUS_FAIL;
    }
}

struct vector_field
general_lerp(
    struct timeline_execution_context *executor, struct vector_field *fields
)
{
    LIBMC_FULL_CAST_RETURN(
        t, 2, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
    );
    struct vector_field const a = fields[0];
    struct vector_field const b = fields[1];

    struct vector_field dump = VECTOR_FIELD_COPY(executor, a);
    if (lerp_recurse(executor, a, &dump, b, (float) t.value.doub) !=
        MC_STATUS_SUCCESS) {
        VECTOR_FIELD_FREE(executor, dump);
        return VECTOR_FIELD_NULL;
    }

    return dump;
}

void
lib_mc_lerp(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    executor->return_register = general_lerp(executor, fields);
}

void
lib_mc_keyframe_lerp(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(t, 2, VECTOR_FIELD_TYPE_DOUBLE);
    mc_bool_t finished;
    executor->return_register =
        parse_src_keyframe(executor, t.value.doub, fields, &finished);
}

void
lib_mc_smooth(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(t, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register =
        double_init(executor, anim_smooth(t.value.doub));
}

void
lib_mc_smooth_in(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(t, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register =
        double_init(executor, 2 * anim_smooth(0.5 * t.value.doub));
}

void
lib_mc_smooth_out(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(t, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register =
        double_init(executor, 2 * anim_smooth(0.5 * (t.value.doub + 1)) - 1);
}
