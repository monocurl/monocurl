//
//  anim_indication.c
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include "anim_indication.h"
#include "mc_meshes.h"
#include "mesh_util.h"

#define PREFIX                                                                 \
    if (owned_mesh_tree(executor, fields[0]) != MC_STATUS_SUCCESS) {           \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        return;                                                                \
    }                                                                          \
    struct mesh_tag_subset const curr = mesh_fullset(executor, fields[0]);     \
    struct mesh_tag_subset const targ = mesh_fullset(executor, fields[1]);     \
    if (timeline_mesh_hide(executor, fields[0]) != MC_STATUS_SUCCESS) {        \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        return;                                                                \
    }                                                                          \
    do {                                                                       \
    } while (0)

#define SUFFIX                                                                 \
    if (timeline_mesh_show(executor, fields[0]) != MC_STATUS_SUCCESS) {        \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        return;                                                                \
    }                                                                          \
    mesh_subset_free(curr);                                                    \
    mesh_subset_free(targ);                                                    \
    executor->return_register = double_init(executor, t >= 1)

void
lib_mc_highlight(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec4 color;
    LIBMC_VEC4(color, 6);

    ANIM_TIME(t, 2);
    PREFIX;

    float const real_t = 1 - 2 * (float) fabs(t - 0.5);

    for (mc_ind_t i = 0; i < curr.subset_count; ++i) {
        struct tetramesh *const tag = curr.meshes[i];
        struct tetramesh *const src = targ.meshes[i];

        for (mc_ind_t j = 0; j < tag->tri_count; ++j) {
            tag->tris[j].a.col = vec4_lerp(src->tris[j].a.col, real_t, color);
            tag->tris[j].b.col = vec4_lerp(src->tris[j].b.col, real_t, color);
            tag->tris[j].c.col = vec4_lerp(src->tris[j].c.col, real_t, color);
        }

        for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
            tag->lins[j].a.col = vec4_lerp(src->lins[j].a.col, real_t, color);
            tag->lins[j].b.col = vec4_lerp(src->lins[j].b.col, real_t, color);
        }

        for (mc_ind_t j = 0; j < tag->dot_count; ++j) {
            tag->dots[j].col = vec4_lerp(src->dots[j].col, real_t, color);
        }

        tag->modded = tag->dirty_hash_cache = 1;
    }

    SUFFIX;
}

static double
default_lead(double t)
{
    double const u = anim_smooth(t);
    double ret = u * (1 + 0.35);
    if (ret >= 1) {
        ret = 1;
    }
    return ret;
}

static double
default_trail(double t)
{
    double const u = anim_smooth(t);
    double ret = u * (1 + 0.35) - 0.35;
    if (ret < 0) {
        ret = 0;
    }
    return ret;
}

void
lib_mc_flash(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    float t = (float) anim_current_time(executor, &fields[2], &default_lead);
    if (t != t) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    fields[5] = fields[6];
    float v = (float) anim_current_time(executor, &fields[2], &default_trail);
    if (v != v) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    if (v > t) {
        VECTOR_FIELD_ERROR(executor, "Expected lead to be >= to trail");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    PREFIX;

    for (mc_ind_t i = 0; i < curr.subset_count; ++i) {
        struct tetramesh *const tag = curr.meshes[i];
        struct tetramesh *const src = targ.meshes[i];

        write_interpolate(i, curr.subset_count, src, tag, v, t);
    }

    // only exit if the leading node reaches 1
    t = v;
    SUFFIX;
}
