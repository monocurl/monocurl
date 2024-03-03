//
//  mc_lib_helpers.h
//  Monocurl
//
//  Created by Manu Bhat on 2/19/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include <float.h>
#include <stdio.h>

#include "function.h"
#include "functor.h"
#include "map.h"
#include "mc_env.h"
#include "primitives.h"
#include "tetramesh.h"
#include "timeline_execution_context.h"
#include "vector_field.h"

#if MC_INTERNAL
mc_bool_t
libmc_verify_mesh_tree(
    struct timeline_execution_context *executor, struct vector_field curr
);

#define LIBMC_MESH_TREE(arg, index)                                            \
    struct vector_field const arg = fields[index];                             \
    do {                                                                       \
        if (!libmc_verify_mesh_tree(executor, arg)) {                          \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            return;                                                            \
        }                                                                      \
    } while (0)

#define LIBMC_DEC_FUNC(func)                                                   \
    void lib_mc_##func(                                                        \
        struct timeline_execution_context *context,                            \
        struct vector_field caller, mc_count_t fc, struct vector_field *args   \
    )
#define LIBMC_FULL_CAST_RETURN(arg, index, type, ret)                          \
    struct vector_field const arg = vector_field_nocopy_extract_type_message(  \
        executor, fields[index], type,                                         \
        "Could not cast `" #arg "` to expected type. Received %s expected %s"  \
    );                                                                         \
    do {                                                                       \
        if (!arg.vtable) {                                                     \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            ret;                                                               \
        }                                                                      \
    } while (0)
#define LIBMC_FULL_CAST(arg, index, type)                                      \
    LIBMC_FULL_CAST_RETURN(arg, index, type, return)

/* arg should be declared by user */
// if no error flag, will still throw errors if it's not a vector
// somewhat of a hack...
#define LIBMC_VEC3_RETURN_ERROR(arg, index, ret, error)                        \
    do {                                                                       \
        struct vector_field const _field =                                     \
            vector_field_nocopy_extract_type_message(                          \
                executor, fields[index], VECTOR_FIELD_TYPE_VECTOR,             \
                "Could not cast `" #arg                                        \
                "` to expected type. Received %s expected %s"                  \
            );                                                                 \
        if (!_field.vtable) {                                                  \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            ret;                                                               \
        }                                                                      \
        struct vector *const _vector = _field.value.pointer;                   \
        if (_vector->field_count != 3) {                                       \
            if (error) {                                                       \
                VECTOR_FIELD_ERROR(                                            \
                    executor,                                                  \
                    "Expected `" #arg "` to be vector of 3 doubles, received " \
                    "vector of length %zu",                                    \
                    _vector->field_count                                       \
                );                                                             \
            }                                                                  \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            ret;                                                               \
        }                                                                      \
        struct vector_field const _a = vector_field_nocopy_extract_type(       \
            executor, _vector->fields[0], VECTOR_FIELD_TYPE_DOUBLE             \
        );                                                                     \
        struct vector_field const _b = vector_field_nocopy_extract_type(       \
            executor, _vector->fields[1], VECTOR_FIELD_TYPE_DOUBLE             \
        );                                                                     \
        struct vector_field const _c = vector_field_nocopy_extract_type(       \
            executor, _vector->fields[2], VECTOR_FIELD_TYPE_DOUBLE             \
        );                                                                     \
        if (!_a.vtable || !_b.vtable || !_c.vtable) {                          \
            if (error) {                                                       \
                VECTOR_FIELD_ERROR(                                            \
                    executor, "Expected `" #arg "` to be vector of 3 doubles", \
                    _vector->field_count                                       \
                );                                                             \
            }                                                                  \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            ret;                                                               \
        }                                                                      \
        arg.x = (float) _a.value.doub;                                         \
        arg.y = (float) _b.value.doub;                                         \
        arg.z = (float) _c.value.doub;                                         \
    } while (0)

#define LIBMC_VEC3_RETURN(arg, index, ret)                                     \
    LIBMC_VEC3_RETURN_ERROR(arg, index, ret, 1)
#define LIBMC_VEC3(arg, index) LIBMC_VEC3_RETURN(arg, index, return)

#define LIBMC_NONNULLVEC3(vector)                                              \
    do {                                                                       \
        if (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z <  \
            FLT_EPSILON) {                                                     \
            VECTOR_FIELD_ERROR(                                                \
                executor, "Expected `" #vector "` to not be not null"          \
            );                                                                 \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            return;                                                            \
        }                                                                      \
    } while (0)

/* arg should be declared by user */
/* maybe don't want a macro for this if we're ltierally going to return it
 * anyways... */
#define LIBMC_VEC4_RETURN(arg, index, return)                                  \
    do {                                                                       \
        struct vector_field const field =                                      \
            vector_field_nocopy_extract_type_message(                          \
                executor, fields[index], VECTOR_FIELD_TYPE_VECTOR,             \
                "Could not cast `" #arg                                        \
                "` to expected type. Received %s expected %s"                  \
            );                                                                 \
        if (!field.vtable) {                                                   \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            return;                                                            \
        }                                                                      \
        struct vector *const vector = field.value.pointer;                     \
        if (vector->field_count != 4) {                                        \
            VECTOR_FIELD_ERROR(                                                \
                executor,                                                      \
                "Expected `" #arg                                              \
                "` to be vector of 4 doubles, received vector of length %zu",  \
                vector->field_count                                            \
            );                                                                 \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            return;                                                            \
        }                                                                      \
        struct vector_field const a = vector_field_nocopy_extract_type(        \
            executor, vector->fields[0], VECTOR_FIELD_TYPE_DOUBLE              \
        );                                                                     \
        struct vector_field const b = vector_field_nocopy_extract_type(        \
            executor, vector->fields[1], VECTOR_FIELD_TYPE_DOUBLE              \
        );                                                                     \
        struct vector_field const c = vector_field_nocopy_extract_type(        \
            executor, vector->fields[2], VECTOR_FIELD_TYPE_DOUBLE              \
        );                                                                     \
        struct vector_field const d = vector_field_nocopy_extract_type(        \
            executor, vector->fields[3], VECTOR_FIELD_TYPE_DOUBLE              \
        );                                                                     \
        if (!a.vtable || !b.vtable || !c.vtable || !d.vtable) {                \
            VECTOR_FIELD_ERROR(                                                \
                executor, "Expected `" #arg "` to be vector of 4 doubles",     \
                vector->field_count                                            \
            );                                                                 \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            return;                                                            \
        }                                                                      \
        arg.x = (float) a.value.doub;                                          \
        arg.y = (float) b.value.doub;                                          \
        arg.z = (float) c.value.doub;                                          \
        arg.w = (float) d.value.doub;                                          \
    } while (0)

#define LIBMC_VEC4(arg, index) LIBMC_VEC4_RETURN(arg, index, return)

#define STANDARD_UNIFORM                                                       \
    (struct tetramesh_uniforms) { 0, 1, 1, 1, 4, 8, 0, 0 }

#define GLOSSY_UNIFORM                                                         \
    (struct tetramesh_uniforms) { 0, 1, 1, 1, 4, 8, 0, (float) 0.5 }

/* [color] {[main] {tag}, [stroke] {tag, stroke}, [solid] {tag, stroke, fill}}
 */
/* frees mesh if error */
/* dimension is highest order dimension */
mc_status_t
libmc_tag(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const tag
);

mc_status_t
libmc_tag_and_color0(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
);
mc_status_t
libmc_tag_and_color1(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
);
mc_status_t
libmc_tag_and_color2(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
);
mc_status_t
libmc_tag_and_color2_forceuprank(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
);
mc_status_t
libmc_tag_and_color3(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
);
#endif
