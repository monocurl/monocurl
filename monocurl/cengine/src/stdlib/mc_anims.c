//
//  mc_anims.c
//  Monocurl
//
//  Created by Manu Bhat on 2/19/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include "mc_anims.h"
#include "anim_util.h"
#include "animation.h"
#include "lvalue.h"

/* Anim(pull&, push&, sentinel(t, dt, pull&, push&), sticky)*/
void
lib_mc_animation(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    executor->return_register =
        animation_init(executor, fields[0], fields[1], fields[2]);
}

// in form of t, config, time, unit_map!
// returns 0 for initial frame, 1 for past target
double
anim_current_time(
    struct timeline_execution_context *executor, struct vector_field *fields,
    double (*default_lerp)(double t)
)
{
    LIBMC_FULL_CAST_RETURN(t, 0, VECTOR_FIELD_TYPE_DOUBLE, return -1);
    LIBMC_FULL_CAST_RETURN(config, 1, VECTOR_FIELD_TYPE_DOUBLE, return -1);
    LIBMC_FULL_CAST_RETURN(time, 2, VECTOR_FIELD_TYPE_DOUBLE, return -1);

    if (t.value.doub < 0) {
        return -FLT_EPSILON;
    }
    else if (t.value.doub == 0) {
        return 0;
    }
    else if (t.value.doub >= time.value.doub) {
        return 1;
    }

    double raw_t = (float) t.value.doub / time.value.doub;

    if (config.value.doub >= 1) {
        LIBMC_FULL_CAST_RETURN(func, 3, VECTOR_FIELD_TYPE_FUNCTION, return -1);

        struct vector_field in = double_init(executor, raw_t);

        function_call(executor, func, 1, &in);
        struct vector_field const curr = vector_field_extract_type(
            executor, &executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!curr.vtable) {
            return NAN;
        }
        else {
            raw_t = curr.value.doub;
        }
    }
    else {
        raw_t = default_lerp(raw_t);
    }

    return raw_t;
}

void
mesh_mapped_subset_free(
    struct timeline_execution_context *executor,
    struct mesh_mapped_subset mapped
)
{
    mesh_subset_free(mapped.subset);
    mesh_subset_free(mapped.invert);
}

static struct vector_field
owned_dfs(struct timeline_execution_context *executor, struct vector_field curr)
{
    struct vector_field cast = vector_field_nocopy_extract_type(
        executor, curr, VECTOR_FIELD_TYPE_MESH | VECTOR_FIELD_TYPE_VECTOR
    );

    if (cast.vtable->type & VECTOR_FIELD_TYPE_MESH) {
        return tetramesh_owned(executor, cast);
    }
    else if (cast.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *vec = cast.value.pointer;
        struct vector_field ret = vector_init(executor);
        for (mc_ind_t i = 0; i < vec->field_count; ++i) {
            struct vector_field sub = owned_dfs(executor, vec->fields[i]);
            if (!sub.vtable) {
                VECTOR_FIELD_FREE(executor, ret);
                return VECTOR_FIELD_NULL;
            }
            vector_plus(executor, ret, &sub);
        }

        return ret;
    }
    else {
        return VECTOR_FIELD_NULL;
    }
}

mc_status_t
owned_mesh_tree(
    struct timeline_execution_context *executor, struct vector_field lvalue
)
{
    struct vector_field copy = owned_dfs(executor, lvalue);
    if (!copy.vtable) {
        return MC_STATUS_FAIL;
    }
    else {
        return VECTOR_FIELD_BINARY(executor, lvalue, assign, &copy).vtable
                   ? MC_STATUS_SUCCESS
                   : MC_STATUS_FAIL;
    }
}
