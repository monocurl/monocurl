//
//  anim_invariants.c
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <string.h>

#include "anim_invariants.h"
#include "anim_util.h"
#include "lvalue.h"
#include "mc_anims.h"
#include "mc_macro_util.h"
#include "mc_meshes.h"
#include "mc_util.h"
#include "mesh_util.h"
#include "vector.h"

#pragma message(                                                               \
    "OPTIMIZATION: Possibly way too many traversals to select subsets "        \
)

//[mesh_tree] {[root] {dst&}, [tag_pred] {dst_root&, tag_predicate(tag),
// leaf_subfield(ref, tag)}, [subfield] {dst&, subfield(ref)}}
static struct mesh_mapped_subset
parse_dst(
    struct timeline_execution_context *executor, struct vector_field *fields,
    mc_bool_t is_scene_like
)
{
    struct mesh_mapped_subset const err = {
        { 0, 0, 0, SIZE_MAX },
        { 0, 0, 0, SIZE_MAX },
    };
    LIBMC_FULL_CAST_RETURN(index, 0, VECTOR_FIELD_TYPE_DOUBLE, return err);

    /* possibly a scene variable */
    if (is_scene_like) {
        struct mesh_mapped_subset ret;
        ret.subset.total_count = ret.subset.subset_count = 1;
        ret.subset.sources = mc_malloc(sizeof(struct vector_field));
        ret.subset.meshes = NULL;
        *ret.subset.sources = fields[1];

        ret.invert.total_count = 0;
        ret.invert.subset_count = 0;
        ret.invert.sources = NULL;
        ret.invert.meshes = NULL;

        return ret;
    }
    else {
        struct mesh_tag_subset subset = mesh_subset(executor, &fields[0], 0);
        if (subset.total_count == SIZE_MAX) {
            return err;
        }
        struct mesh_tag_subset invert = mesh_subset(executor, &fields[0], 1);
        if (invert.total_count == SIZE_MAX) {
            mesh_subset_free(subset);
            return err;
        }

        struct mesh_mapped_subset const ret = { subset, invert };
        return ret;
    }
}

/* negative = sentinel_force */
// native keyframe(mesh_tree, pull, tag_predicate!, leaf_subfield!, t, source,
// time_to_value, unit_map!)
struct vector_field
parse_src_keyframe(
    struct timeline_execution_context *executor, double t,
    struct vector_field *fields, mc_bool_t *finished
)
{
    LIBMC_FULL_CAST_RETURN(
        time_map, 0, VECTOR_FIELD_TYPE_MAP, return VECTOR_FIELD_NULL
    );
    LIBMC_FULL_CAST_RETURN(
        unit_map, 1, VECTOR_FIELD_TYPE_FUNCTION, return VECTOR_FIELD_NULL
    );

    struct map *const map = time_map.value.pointer;
    double prev_t = -FLT_EPSILON, next_t = 0;
    struct vector_field val[2] = { VECTOR_FIELD_NULL, VECTOR_FIELD_NULL };

    mc_ternary_status_t kill = MC_TERNARY_STATUS_FINISH;

    // technically doesn't work for negatives i suppose
    // but i guess that isn't really a valid use case so
    mc_bool_t flag = 0;
    for (struct map_node *head = map->head.next_ins; head;
         head = head->next_ins) {
        struct vector_field const time = vector_field_nocopy_extract_type(
            executor, head->field, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!time.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return VECTOR_FIELD_NULL;
        }

        next_t = time.value.doub;
        val[1] = head->value;

        if (!flag) {
            val[0] = val[1];
            flag = 1;
        }

        if ((t >= prev_t && t < next_t) || t < prev_t) {
            kill = MC_TERNARY_STATUS_CONTINUE;
            break;
        }

        prev_t = next_t;
        val[0] = val[1];
    }

    double const unit_t =
        (next_t - prev_t < DBL_EPSILON) ? 0 : (t - prev_t) / (next_t - prev_t);
    struct vector_field t_field = double_init(executor, unit_t);

    if (!val[0].vtable || !val[1].vtable) {
        VECTOR_FIELD_ERROR(executor, "Expected non empty keyframe map");
        executor->return_register = VECTOR_FIELD_NULL;
        return VECTOR_FIELD_NULL;
    }

    function_call(executor, unit_map, 1, &t_field);

    struct vector_field target_t = vector_field_extract_type(
        executor, &executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
    );

    if (!target_t.vtable) {
        executor->return_register = VECTOR_FIELD_NULL;
        return VECTOR_FIELD_NULL;
    }

    struct vector_field args[3] = { val[0], val[1], target_t };
    struct vector_field lerp = general_lerp(executor, args);

    if (kill == MC_TERNARY_STATUS_FINISH) {
        *finished = 1;
    }

    return lerp;
}

void
lib_mc_set(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    if (timeline_mesh_hide(executor, fields[0]) != MC_STATUS_SUCCESS) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vector_field const ret =
        VECTOR_FIELD_BINARY(executor, fields[0], assign, &fields[1]);
    if (!ret.vtable) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    if (timeline_mesh_show(executor, fields[0]) != MC_STATUS_SUCCESS) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    executor->return_register = double_init(executor, 1);
}

static struct vector_field
transfer_all(
    struct timeline_execution_context *executor, struct vector_field curr,
    mc_bool_t copy
)
{
    if (curr.vtable == &reference_vtable) {
        return transfer_all(
            executor, *(struct vector_field *) curr.value.pointer, copy
        );
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        struct vector_field ret = VECTOR_FIELD_COPY(executor, curr);
        if (!copy) {
            struct vector_field sub = vector_init(executor);
            VECTOR_FIELD_BINARY(executor, curr, assign, &sub);
        }
        return ret;
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vec = curr.value.pointer;
        struct vector_field ret = vector_init(executor);
        for (mc_ind_t i = 0; i < vec->field_count; ++i) {
            struct vector_field sub =
                transfer_all(executor, vec->fields[i], copy);

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

static mc_status_t
transfer_subset(
    struct timeline_execution_context *executor, struct vector_field *fields,
    struct vector_field curr, struct vector_field dst_dump, mc_bool_t copy
)
{
    if (curr.vtable == &reference_vtable) {
        return transfer_subset(
            executor, fields, *(struct vector_field *) curr.value.pointer,
            dst_dump, copy
        );
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        fields[1] = curr;
        struct mesh_mapped_subset dst = parse_dst(executor, fields, 0);
        if (dst.subset.total_count == SIZE_MAX) {
            return MC_STATUS_FAIL;
        }

        for (mc_ind_t i = 0; i < dst.subset.subset_count; ++i) {
            vector_plus(executor, dst_dump, &dst.subset.sources[i]);
        }

        if (!copy) {
            struct vector_field src = vector_init(executor);
            for (mc_ind_t i = 0; i < dst.invert.subset_count; ++i) {
                vector_plus(executor, src, &dst.invert.sources[i]);
            }

            VECTOR_FIELD_BINARY(executor, curr, assign, &src);
        }

        mesh_mapped_subset_free(executor, dst);

        return MC_STATUS_SUCCESS;
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vec = curr.value.pointer;
        for (mc_ind_t i = 0; i < vec->field_count; ++i) {
            if (transfer_subset(
                    executor, fields, vec->fields[i], dst_dump, copy
                ) != MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }
        }

        return MC_STATUS_FAIL;
    }
    else {
        return MC_STATUS_FAIL;
    }
}

static mc_bool_t
verify_consistency(
    struct timeline_execution_context *executor, struct vector_field curr
)
{
    if (curr.vtable == &reference_vtable) {
        return verify_consistency(
            executor, *(struct vector_field *) curr.value.pointer
        );
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        struct vector_field *stack = curr.value.pointer;
        mc_ind_t const i = (mc_ind_t) (stack - executor->stack);
        struct vector_field comp = executor->creation_follower_stack[i];
        mc_hash_t const it = VECTOR_FIELD_HASH(executor, *stack);
        if (!it) {
            return 0;
        }
        mc_hash_t const follow = VECTOR_FIELD_HASH(executor, comp);
        if (it != follow) {
            VECTOR_FIELD_ERROR(
                executor, "To make semantics clear, the destination iterator "
                          "should not have any modifications since its last "
                          "animation. You can fix this by calling Set: on the "
                          "`into` iterator right before the Transfer."
            );
            return 0;
        }
        return 1;
    }
    else {
        struct vector *vec = curr.value.pointer;
        for (mc_ind_t i = 0; i < vec->field_count; ++i) {
            if (!verify_consistency(executor, vec->fields[i])) {
                return 0;
            }
        }

        return 1;
    }
}
void
lib_mc_transfer(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(dst_type, 1, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(copy, 4, VECTOR_FIELD_TYPE_DOUBLE);

    if (timeline_is_reference_var_a_vector(executor, fields[0])) {
        VECTOR_FIELD_ERROR(
            executor, "Expected a single variable for destination!"
        );
        VECTOR_FIELD_NULL;
        return;
    }

    if (!verify_consistency(executor, fields[0])) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vector_field transfer;
    if (dst_type.value.doub == 0) {
        transfer = transfer_all(executor, fields[2], copy.value.doub != 0);
        if (!transfer.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }
    else {
        transfer = vector_init(executor);
        if (transfer_subset(
                executor, &fields[1], fields[2], transfer, copy.value.doub != 0
            ) != MC_STATUS_SUCCESS) {
            VECTOR_FIELD_FREE(executor, transfer);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }

    struct vector_field cast = vector_field_safe_extract_type(
        executor, fields[0], VECTOR_FIELD_TYPE_VECTOR
    );

    struct vector_field build;
    if (cast.vtable->type & VECTOR_FIELD_TYPE_VECTOR &&
        !((struct vector *) cast.value.pointer)->field_count) {
        build = VECTOR_FIELD_COPY(executor, transfer);
    }
    else {
        build = VECTOR_FIELD_COPY(executor, transfer);
        struct vector_field new = vector_init(executor);
        struct vector_field aux = fields[0];
        vector_plus(executor, new, &aux);
        vector_plus(executor, new, &build);
        build = new;
    }

    /* guaranteed */
    VECTOR_FIELD_BINARY(executor, fields[0], assign, &build);

    executor->return_register = transfer;
}

void
lib_mc_transfer_runtime(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(dst_type, 1, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(copy, 4, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(expected_hash, 6, VECTOR_FIELD_TYPE_DOUBLE);

    if (timeline_is_reference_var_a_vector(executor, fields[0])) {
        VECTOR_FIELD_ERROR(
            executor, "Expected a single variable for destination!"
        );
        VECTOR_FIELD_NULL;
        return;
    }

    if (timeline_mesh_hide(executor, fields[0]) != MC_STATUS_SUCCESS ||
        timeline_mesh_hide(executor, fields[2]) != MC_STATUS_SUCCESS) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vector_field transfer;
    if (dst_type.value.doub == 0) {
        transfer = transfer_all(executor, fields[2], copy.value.doub != 0);
        if (!transfer.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }
    else {
        transfer = vector_init(executor);
        if (transfer_subset(
                executor, &fields[1], fields[2], transfer, copy.value.doub != 0
            ) != MC_STATUS_SUCCESS) {
            VECTOR_FIELD_FREE(executor, transfer);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }
    mc_hash_t comp = VECTOR_FIELD_HASH(executor, transfer);
    if (!comp) {
        VECTOR_FIELD_FREE(executor, transfer);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    else if (comp != expected_hash.value.hash) {
        VECTOR_FIELD_FREE(executor, transfer);
        VECTOR_FIELD_ERROR(
            executor,
            "Concurrent Modification! Two or more animations are fighting for "
            "the same subset of a tree variable (Or you might have created an "
            "animation but not added it to the play list, this is not allowed)"
        );
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    VECTOR_FIELD_FREE(executor, transfer);

    transfer = fields[5];

    struct vector_field cast = vector_field_safe_extract_type(
        executor, fields[0], VECTOR_FIELD_TYPE_VECTOR
    );

    struct vector_field build;
    if (!cast.vtable) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    else if (cast.vtable->type & VECTOR_FIELD_TYPE_VECTOR && !((struct vector *) cast.value.pointer)->field_count) {
        build = VECTOR_FIELD_COPY(executor, transfer);
    }
    else {
        build = VECTOR_FIELD_COPY(executor, transfer);
        struct vector_field new = vector_init(executor);
        struct vector_field aux = fields[0];
        vector_plus(executor, new, &aux);
        vector_plus(executor, new, &build);
        build = new;
    }

    /* guaranteed */
    VECTOR_FIELD_BINARY(executor, fields[0], assign, &build);

    if (timeline_mesh_show(executor, fields[0]) != MC_STATUS_SUCCESS ||
        timeline_mesh_show(executor, fields[2]) != MC_STATUS_SUCCESS) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    executor->return_register = double_init(executor, 1);
}

void
lib_mc_lerp_anim(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    if (timeline_mesh_hide(executor, fields[0]) != MC_STATUS_SUCCESS) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    ANIM_TIME(t, 3);

    struct vector_field args[3];
    args[0] = fields[1];
    args[1] = fields[2];
    args[2] = double_init(executor, t);
    struct vector_field mapped = general_lerp(executor, args);
    if (!mapped.vtable) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    if (!VECTOR_FIELD_BINARY(executor, fields[0], assign, &mapped).vtable) {
        VECTOR_FIELD_FREE(executor, mapped);
        return;
    }

    if (timeline_mesh_show(executor, fields[0]) != MC_STATUS_SUCCESS) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    executor->return_register = double_init(executor, t >= 1);
}
