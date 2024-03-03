//
//  mesh_util.c
//  Monocurl
//
//  Created by Manu Bhat on 2/22/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//
#include <string.h>

#include "anim_transform.h"
#include "lvalue.h"
#include "mesh_util.h"

#define MATCH_PREFIX                                                           \
    LIBMC_FULL_CAST(tp, 2, VECTOR_FIELD_TYPE_DOUBLE);                          \
    float t = (float) tp.value.doub;                                           \
    if (t < 0) {                                                               \
        t = 0;                                                                 \
    }                                                                          \
    else if (t > 1) {                                                          \
        t = 1;                                                                 \
    }                                                                          \
    struct vector_field buffer[3];                                             \
    buffer[0] = double_init(executor, 0);                                      \
    buffer[1] = fields[0];                                                     \
    buffer[2] = fields[1];                                                     \
    fields = buffer;                                                           \
    LIBMC_SELECT(curr, 0);                                                     \
                                                                               \
    buffer[1] = buffer[2];                                                     \
    LIBMC_SELECT_RETURN(targ, 0, mesh_subset_free(curr); return);              \
    /* contour separate */                                                     \
    struct vector_field a_src = vector_init(executor);                         \
    for (mc_ind_t i = 0; i < curr.subset_count; ++i) {                         \
        struct vector_field tmp =                                              \
            VECTOR_FIELD_COPY(executor, curr.sources[i]);                      \
        vector_plus(executor, a_src, &tmp);                                    \
    }                                                                          \
                                                                               \
    struct vector_field b_src = vector_init(executor);                         \
    for (mc_ind_t i = 0; i < targ.subset_count; ++i) {                         \
        struct vector_field tmp =                                              \
            VECTOR_FIELD_COPY(executor, targ.sources[i]);                      \
        vector_plus(executor, b_src, &tmp);                                    \
    }                                                                          \
    if (mesh_contour_separate(executor, &a_src) != MC_STATUS_SUCCESS ||        \
        mesh_contour_separate(executor, &b_src) != MC_STATUS_SUCCESS) {        \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        VECTOR_FIELD_FREE(executor, a_src);                                    \
        VECTOR_FIELD_FREE(executor, b_src);                                    \
        mesh_subset_free(curr);                                                \
        mesh_subset_free(targ);                                                \
        return;                                                                \
    }                                                                          \
    struct vector *const a_src_v = a_src.value.pointer;                        \
    struct vector *const b_src_v = b_src.value.pointer;                        \
    if (!a_src_v->field_count || !b_src_v->field_count) {                      \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        VECTOR_FIELD_ERROR(                                                    \
            executor, "Both source and destination meshes should have at "     \
                      "least one contour"                                      \
        );                                                                     \
        VECTOR_FIELD_FREE(executor, a_src);                                    \
        VECTOR_FIELD_FREE(executor, b_src);                                    \
        mesh_subset_free(curr);                                                \
        mesh_subset_free(targ);                                                \
        return;                                                                \
    }                                                                          \
                                                                               \
    struct vector_field a_dmp = vector_init(executor);                         \
    struct vector_field dmp = vector_init(executor);                           \
    struct vector_field b_dmp = vector_init(executor);                         \
    match_group(                                                               \
        executor, a_src_v->fields, b_src_v->fields, a_dmp, dmp, b_dmp,         \
        curr.subset_count, targ.subset_count                                   \
    );                                                                         \
    struct vector *const prev_vec = a_dmp.value.pointer;                       \
    struct vector *const lerp_vec = dmp.value.pointer;                         \
    struct vector *const next_vec = b_dmp.value.pointer

#define MATCH_SUFFIX                                                           \
    executor->return_register = dmp;                                           \
    VECTOR_FIELD_FREE(executor, a_dmp);                                        \
    VECTOR_FIELD_FREE(executor, b_dmp);                                        \
    VECTOR_FIELD_FREE(executor, a_src);                                        \
    VECTOR_FIELD_FREE(executor, b_src);                                        \
    mesh_subset_free(curr);                                                    \
    mesh_subset_free(targ);

void
lib_mc_mesh_lerp(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    MATCH_PREFIX;

    for (mc_ind_t i = 0; i < lerp_vec->field_count; ++i) {
        struct tetramesh *const prv = prev_vec->fields[i].value.pointer;
        struct tetramesh *const tag = lerp_vec->fields[i].value.pointer;
        struct tetramesh *const nxt = next_vec->fields[i].value.pointer;

        if (tag->payload && !prv->payload && !nxt->payload) {
            tag->tri_count = 0;

            tag->lin_count = prv->lin_count;
            tag->lins = mc_reallocf(
                tag->lins, sizeof(struct tetra_lin) * prv->lin_count
            );
            memcpy(
                tag->lins, prv->lins, sizeof(struct tetra_lin) * prv->lin_count
            );

            mesh_patharc_lerp(prv, tag, nxt, t, VEC3_0);
            if (tetramesh_uprank(tag, 1) != MC_STATUS_SUCCESS) {
                VECTOR_FIELD_ERROR(executor, "Error upranking!");
                executor->return_register = VECTOR_FIELD_NULL;
                return;
            }

            int32_t const j =
                prv->lins[0].is_dominant_sibling ? prv->lins[0].inverse : 0;
            struct vec4 const a_col = prv->lins[j].a.col;
            int32_t const k =
                nxt->lins[0].is_dominant_sibling ? nxt->lins[0].inverse : 0;
            struct vec4 const b_col = nxt->lins[k].a.col;
            struct vec4 const col = vec4_lerp(a_col, t, b_col);

            for (mc_ind_t q = 0; q < tag->tri_count; ++q) {
                tag->tris[q].a.col = tag->tris[q].b.col = tag->tris[q].c.col =
                    col;
            }
        }
        else {
            mesh_patharc_lerp(prv, tag, nxt, t, (struct vec3){ 0, 0, 0 });
        }

        tag->modded = tag->dirty_hash_cache = 1;
    }

    MATCH_SUFFIX;
}

void
lib_mc_mesh_bend(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    MATCH_PREFIX;

    mc_bool_t failed = 0;
    for (mc_ind_t i = 0; i < lerp_vec->field_count; ++i) {
        struct tetramesh *const prv = prev_vec->fields[i].value.pointer;
        struct tetramesh *const tag = lerp_vec->fields[i].value.pointer;
        struct tetramesh *const nxt = next_vec->fields[i].value.pointer;

        if (!tag->lin_count || tag->tri_count) {
            VECTOR_FIELD_ERROR(
                executor, "Cannot currently apply bend to meshes with "
                          "triangles; line meshes only"
            );
            failed = 1;
            break;
        }

        mesh_bend_lerp(prv, tag, nxt, t);

        tag->modded = tag->dirty_hash_cache = 1;
    }

    MATCH_SUFFIX;

    if (failed) {
        VECTOR_FIELD_FREE(executor, executor->return_register);
        executor->return_register = VECTOR_FIELD_NULL;
    }
}

static struct vector_field
_sample_data(
    struct timeline_execution_context *executor, struct vector_field *fields,
    int sample_type
)
{
    LIBMC_FULL_CAST_RETURN(
        tp, 1, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
    );
    float t = (float) fmod(tp.value.doub, 1);
    if (t < 0) {
        t += 1;
    }
    LIBMC_FULL_CAST_RETURN(
        meshp, 0, VECTOR_FIELD_TYPE_MESH, return VECTOR_FIELD_NULL
    );

    struct tetramesh *const mesh = meshp.value.pointer;
    if (!mesh->lin_count) {
        VECTOR_FIELD_ERROR(executor, "Can only sample on lin meshes");
        return VECTOR_FIELD_NULL;
    }

    int32_t j = 0;
    if (!mesh->lins[j].is_dominant_sibling) {
        j = mesh->lins[j].inverse;
    }
    int32_t j_org = j;
    while (mesh->lins[j].prev >= 0 && mesh->lins[j].prev != j_org) {
        j = mesh->lins[j].prev;
    }
    j_org = j;

    float length = 0;
    do {
        length += vec3_norm(vec3_sub(mesh->lins[j].b.pos, mesh->lins[j].a.pos));
        j = mesh->lins[j].next;
    } while (j >= 0 && j != j_org);

    float const targ = length * t;
    float delta_len = 0;
    length = 0;
    j = j_org;

    while (1) {
        length +=
            (delta_len =
                 vec3_norm(vec3_sub(mesh->lins[j].b.pos, mesh->lins[j].a.pos)));
        if (length - targ >= -GEOMETRIC_EPSILON) {
            break;
        }
        j = mesh->lins[j].next;
    }

    struct vec3 ret;
    if (sample_type == 0) {
        // sampel
        float const lp = length - delta_len;
        float const u =
            delta_len < GEOMETRIC_EPSILON ? 0 : (targ - lp) / (delta_len);
        ret = vec3_lerp(mesh->lins[j].a.pos, u, mesh->lins[j].b.pos);
    }
    else if (sample_type == 1) {
        // normal
        struct vec3 const delta =
            vec3_sub(mesh->lins[j].b.pos, mesh->lins[j].a.pos);
        ret = vec3_unit(vec3_cross(delta, mesh->lins[j].norm));
    }
    else {
        // tangent
        ret = vec3_unit(vec3_sub(mesh->lins[j].b.pos, mesh->lins[j].a.pos));
    }

    struct vector_field out = vector_init(executor);
    struct vector_field aux = double_init(executor, ret.x);
    vector_plus(executor, out, &aux);
    aux = double_init(executor, ret.y);
    vector_plus(executor, out, &aux);
    aux = double_init(executor, ret.z);
    vector_plus(executor, out, &aux);

    return out;
}

void
lib_mc_mesh_sample(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    executor->return_register = _sample_data(executor, fields, 0);
}

void
lib_mc_mesh_normal(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    executor->return_register = _sample_data(executor, fields, 1);
}

void
lib_mc_mesh_tangent(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    executor->return_register = _sample_data(executor, fields, 2);
}

void
lib_mc_mesh_rank(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);

    int max_rank = -1;
    for (mc_ind_t i = 0; i < tags.subset_count; ++i) {
        int const curr_rank = tetramesh_rank(tags.meshes[i]);
        if (curr_rank > max_rank) {
            max_rank = curr_rank;
        }
    }

    executor->return_register = double_init(executor, max_rank);
    mesh_subset_free(tags);
}

static float
mesh_cast(struct tetramesh const *mesh, struct vec3 src, struct vec3 out)
{
    float ret = FLT_MAX;

    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        struct vec3 const flat_norm = vec3_unit(triangle_cross_product(
            mesh->tris[i].a.pos, mesh->tris[i].b.pos, mesh->tris[i].c.pos
        ));

        float const base =
            vec3_dot(flat_norm, vec3_sub(mesh->tris[i].a.pos, src));
        float const denom = vec3_dot(flat_norm, out);

        if (fabsf(denom) < GEOMETRIC_EPSILON) {
            continue;
        }

        float const t = base / denom;
        if (t < 0 || t > ret) {
            continue;
        }

        struct vec3 const point = vec3_add(src, vec3_mul_scalar(t, out));

        float const cmp = triangle_area(
            mesh->tris[i].a.pos, mesh->tris[i].b.pos, mesh->tris[i].c.pos
        );
        float const a =
            triangle_area(mesh->tris[i].b.pos, mesh->tris[i].c.pos, point);
        float const b =
            triangle_area(mesh->tris[i].a.pos, point, mesh->tris[i].c.pos);
        float const c =
            triangle_area(mesh->tris[i].a.pos, mesh->tris[i].b.pos, point);

        if (a + b + c - cmp < GEOMETRIC_EPSILON) {
            ret = t;
        }
    }

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        struct vec3 const delta =
            vec3_unit(vec3_sub(mesh->lins[i].b.pos, mesh->lins[i].a.pos));
        struct vec3 const to_source = vec3_sub(src, mesh->lins[i].a.pos);
        if (vec3_dot(vec3_cross(delta, to_source), out) > GEOMETRIC_EPSILON) {
            // not co planar
            continue;
        }

        float const along = vec3_dot(to_source, delta);
        struct vec3 const at =
            vec3_add(mesh->lins[i].a.pos, vec3_mul_scalar(along, delta));
        struct vec3 const direc = vec3_sub(at, src);
        float const direc_norm = vec3_norm(direc);

        float const denom = vec3_dot(direc, out);
        float t;
        if (denom < GEOMETRIC_EPSILON ||
            (t = direc_norm * direc_norm / denom) >= ret) {
            continue;
        }

        struct vec3 const point = vec3_add(src, vec3_mul_scalar(t, out));

        float const dist =
            vec3_norm(vec3_sub(mesh->lins[i].b.pos, mesh->lins[i].a.pos));

        float const u = vec3_dot(vec3_sub(point, mesh->lins[i].a.pos), delta);

        if (u >= 0 && u < dist) {
            ret = t;
        }
    }

    for (mc_ind_t i = 0; i < mesh->dot_count; ++i) {
        float const dist = vec3_norm(vec3_sub(mesh->dots[i].pos, src));

        if (dist > ret) {
            continue;
        }

        float const t = vec3_dot(vec3_sub(mesh->dots[i].pos, src), out);

        if (fabsf(t - dist) > GEOMETRIC_EPSILON) {
            continue;
        }

        ret = t;
    }

    return ret;
}

static float
mesh_subset_cast(struct mesh_tag_subset tags, struct vec3 src, struct vec3 out)
{
    float t = FLT_MAX;
    for (mc_ind_t i = 0; i < tags.subset_count; ++i) {
        float const res = mesh_cast(tags.meshes[i], src, out);
        if (res < t) {
            t = res;
        }
    }

    return t;
}

struct vec3
mesh_subset_full_cast(
    struct mesh_tag_subset tags, struct vec3 src, struct vec3 out
)
{
    float t = FLT_MAX;
    for (mc_ind_t i = 0; i < tags.subset_count; ++i) {
        float const res = mesh_cast(tags.meshes[i], src, out);
        if (res < t) {
            t = res;
        }
    }

    if (t == FLT_MAX) {
        return src;
    }

    return vec3_add(src, vec3_mul_scalar(t, out));
}

void
lib_mc_mesh_raycast(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 src, out;
    LIBMC_VEC3(src, 3);
    LIBMC_VEC3(out, 4);
    LIBMC_NONNULLVEC3(out);
    float const norm = vec3_norm(out);
    out = vec3_unit(out);
    LIBMC_SELECT(tags, 0);

    float t = mesh_subset_cast(tags, src, out);

    executor->return_register =
        double_init(executor, t == FLT_MAX ? -1 : t / norm);
    mesh_subset_free(tags);
}

static mc_bool_t
mesh_contains(struct tetramesh *mesh, struct vec3 point)
{
    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        float const cmp = triangle_area(
            mesh->tris[i].a.pos, mesh->tris[i].b.pos, mesh->tris[i].c.pos
        );
        float const a =
            triangle_area(mesh->tris[i].b.pos, mesh->tris[i].c.pos, point);
        float const b =
            triangle_area(mesh->tris[i].a.pos, point, mesh->tris[i].c.pos);
        float const c =
            triangle_area(mesh->tris[i].a.pos, mesh->tris[i].b.pos, point);

        if (a + b + c - cmp < GEOMETRIC_EPSILON) {
            return 1;
        }
    }

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        float const dist =
            vec3_norm(vec3_sub(mesh->lins[i].b.pos, mesh->lins[i].a.pos));

        float const a = vec3_norm(vec3_sub(mesh->lins[i].a.pos, point));
        float const b = vec3_norm(vec3_sub(mesh->lins[i].b.pos, point));

        if (a + b - dist < GEOMETRIC_EPSILON) {
            return 1;
        }
    }

    for (mc_ind_t i = 0; i < mesh->dot_count; ++i) {
        float const dist = vec3_norm(vec3_sub(mesh->dots[i].pos, point));

        if (dist < GEOMETRIC_EPSILON) {
            return 1;
        }
    }

    return 0;
}

void
lib_mc_mesh_contains(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 point;
    LIBMC_VEC3(point, 3);
    LIBMC_SELECT(tags, 0);

    for (mc_ind_t i = 0; i < tags.subset_count; ++i) {
        if (mesh_contains(tags.meshes[i], point)) {
            executor->return_register = double_init(executor, 1);
            return;
        }
    }

    executor->return_register = double_init(executor, 0);
    mesh_subset_free(tags);
}

void
lib_mc_mesh_contour_count(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);

    mc_count_t ret = 0;
    for (mc_ind_t i = 0; i < tags.subset_count; ++i) {
        ret += tetramesh_contour_count(executor, tags.meshes[i]);
    }

    executor->return_register = double_init(executor, (double) ret);
    mesh_subset_free(tags);
}

void
lib_mc_mesh_contour_separated(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);

    struct vector_field dump = vector_init(executor);
    for (mc_ind_t i = 0; i < tags.subset_count; ++i) {
        struct vector_field copy = VECTOR_FIELD_COPY(executor, tags.sources[i]);
        vector_plus(executor, dump, &copy);
    }

    if (mesh_contour_separate(executor, &dump) != MC_STATUS_SUCCESS) {
        VECTOR_FIELD_FREE(executor, dump);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    else {
        executor->return_register = dump;
    }

    mesh_subset_free(tags);
}

void
lib_mc_mesh_matched(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(mapping_index, 2, VECTOR_FIELD_TYPE_DOUBLE);

    if (mesh_contour_separate(executor, &fields[0]) != MC_STATUS_SUCCESS) {
        VECTOR_FIELD_ERROR(executor, "Improper mesh-tree of origin");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    if (mesh_contour_separate(executor, &fields[1]) != MC_STATUS_SUCCESS) {
        VECTOR_FIELD_ERROR(executor, "Improper mesh-tree of target");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vector_field a_dmp = vector_init(executor);
    struct vector_field dmp_dmp = vector_init(executor);
    struct vector_field b_dmp = vector_init(executor);
    if (match_tree(
            executor, fields[0], fields[1], a_dmp, dmp_dmp, b_dmp,
            (mc_ind_t) mapping_index.value.doub, fields[3]
        ) != MC_STATUS_SUCCESS) {
        VECTOR_FIELD_FREE(executor, a_dmp);
        VECTOR_FIELD_FREE(executor, b_dmp);
        VECTOR_FIELD_FREE(executor, dmp_dmp);
        return;
    }

    struct vector_field ret = vector_init(executor);
    vector_plus(executor, ret, &a_dmp);
    vector_plus(executor, ret, &dmp_dmp);
    vector_plus(executor, ret, &b_dmp);

    executor->return_register = ret;
}

static float
lin_dist(struct vec3 a, struct vec3 b, struct vec3 test)
{
    struct vec3 const delt = vec3_unit(vec3_sub(b, a));
    float const norm = vec3_norm(vec3_sub(b, a));

    float t = vec3_dot(vec3_sub(test, a), delt);
    if (t < 0) {
        t = 0;
    }
    else if (t > norm) {
        t = norm;
    }

    float const prime =
        vec3_norm(vec3_sub(vec3_add(a, vec3_mul_scalar(t, delt)), test));

    return prime;
}

static float
mesh_dist(struct tetramesh const *mesh, struct vec3 test_point)
{
    float dist = FLT_MAX;

    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        // closest point on the plane
        struct vec3 const flat_norm = vec3_unit(triangle_cross_product(
            mesh->tris[i].a.pos, mesh->tris[i].b.pos, mesh->tris[i].c.pos
        ));

        float const plane_dist =
            vec3_dot(flat_norm, vec3_sub(test_point, mesh->tris[i].a.pos));
        struct vec3 point =
            vec3_sub(test_point, vec3_mul_scalar(-plane_dist, flat_norm));

        float const cmp = 2 * triangle_area(
                                  mesh->tris[i].a.pos, mesh->tris[i].b.pos,
                                  mesh->tris[i].c.pos
                              );
        struct vec3 const a = triangle_cross_product(
            mesh->tris[i].b.pos, mesh->tris[i].c.pos, point
        );
        float const alpha = vec3_dot(a, flat_norm) / cmp;

        struct vec3 const b = triangle_cross_product(
            mesh->tris[i].a.pos, point, mesh->tris[i].c.pos
        );
        float const beta = vec3_dot(b, flat_norm) / cmp;

        struct vec3 const c = triangle_cross_product(
            mesh->tris[i].a.pos, mesh->tris[i].b.pos, point
        );
        float const gamma = vec3_dot(c, flat_norm) / cmp;

        float prime = FLT_MAX;
        if (alpha > 0 && beta > 0 && gamma > 0) {
            prime = vec3_norm(vec3_sub(test_point, point));
        }
        else if (alpha > 0 && beta > 0) {
            prime =
                lin_dist(mesh->tris[i].a.pos, mesh->tris[i].b.pos, test_point);
        }
        else if (alpha > 0 && gamma > 0) {
            prime =
                lin_dist(mesh->tris[i].c.pos, mesh->tris[i].a.pos, test_point);
        }
        else if (beta > 0 && gamma > 0) {
            prime =
                lin_dist(mesh->tris[i].b.pos, mesh->tris[i].c.pos, test_point);
        }
        else if (alpha > 0) {
            prime = vec3_norm(vec3_sub(mesh->tris[i].a.pos, test_point));
        }
        else if (beta > 0) {
            prime = vec3_norm(vec3_sub(mesh->tris[i].b.pos, test_point));
        }
        else if (gamma > 0) {
            /* else if is redundant, guaranteed condition */
            prime = vec3_norm(vec3_sub(mesh->tris[i].c.pos, test_point));
        }

        if (prime < dist) {
            dist = prime;
        }
    }

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        float const prime =
            lin_dist(mesh->lins[i].a.pos, mesh->lins[i].b.pos, test_point);
        if (prime < dist) {
            dist = prime;
        }
    }

    for (mc_ind_t i = 0; i < mesh->dot_count; ++i) {
        float const prime = vec3_norm(vec3_sub(test_point, mesh->dots[i].pos));
        if (prime < dist) {
            dist = prime;
        }
    }

    return dist;
}

void
lib_mc_mesh_dist(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 test_point;
    LIBMC_VEC3(test_point, 3);
    LIBMC_SELECT(tags, 0);

    float dist = FLT_MAX;
    for (mc_ind_t i = 0; i < tags.subset_count; ++i) {
        float const prime = mesh_dist(tags.meshes[i], test_point);
        if (prime < dist) {
            dist = prime;
        }
    }

    executor->return_register = double_init(executor, dist);
    mesh_subset_free(tags);
}

void
lib_mc_mesh_select_tags(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);

    struct vector_field const ret = vector_init(executor);

    for (mc_ind_t i = 0; i < tags.subset_count; ++i) {
        struct vector_field curr = VECTOR_FIELD_COPY(executor, tags.sources[i]);
        vector_plus(executor, ret, &curr);
    }

    executor->return_register = ret;
    mesh_subset_free(tags);
}

float
mesh_direction(struct mesh_tag_subset subset, struct vec3 direction)
{
    direction = vec3_unit(direction);

    float max = -FLT_MAX;
    for (mc_ind_t i = 0; i < subset.subset_count; ++i) {
        struct tetramesh *const curr = subset.meshes[i];

        for (mc_ind_t j = 0; j < curr->tri_count; ++j) {
            float const comp_a = vec3_dot(curr->tris[j].a.pos, direction);
            float const comp_b = vec3_dot(curr->tris[j].b.pos, direction);
            float const comp_c = vec3_dot(curr->tris[j].c.pos, direction);

            if (comp_a > max) {
                max = comp_a;
            }
            if (comp_b > max) {
                max = comp_b;
            }
            if (comp_c > max) {
                max = comp_c;
            }
        }

        for (mc_ind_t j = 0; j < curr->lin_count; ++j) {
            float const comp_a = vec3_dot(curr->lins[j].a.pos, direction);
            float const comp_b = vec3_dot(curr->lins[j].b.pos, direction);

            if (comp_a > max) {
                max = comp_a;
            }
            if (comp_b > max) {
                max = comp_b;
            }
        }

        for (mc_ind_t j = 0; j < curr->dot_count; ++j) {
            float const comp_a = vec3_dot(curr->dots[j].pos, direction);

            if (comp_a > max) {
                max = comp_a;
            }
        }
    }

    if (max == -FLT_MAX) {
        return 0;
    }

    return max;
}

void
lib_mc_mesh_left(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);
    executor->return_register =
        double_init(executor, -mesh_direction(tags, (struct vec3){ -1, 0, 0 }));
    mesh_subset_free(tags);
}

void
lib_mc_mesh_right(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);
    executor->return_register =
        double_init(executor, mesh_direction(tags, (struct vec3){ +1, 0, 0 }));
    mesh_subset_free(tags);
}

void
lib_mc_mesh_up(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);
    executor->return_register =
        double_init(executor, mesh_direction(tags, (struct vec3){ 0, 1, 0 }));
    mesh_subset_free(tags);
}

void
lib_mc_mesh_down(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);
    executor->return_register =
        double_init(executor, -mesh_direction(tags, (struct vec3){ 0, -1, 0 }));
    mesh_subset_free(tags);
}

void
lib_mc_mesh_forward(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);
    executor->return_register =
        double_init(executor, mesh_direction(tags, (struct vec3){ 0, 0, 1 }));
    mesh_subset_free(tags);
}

void
lib_mc_mesh_backward(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);
    executor->return_register =
        double_init(executor, -mesh_direction(tags, (struct vec3){ 0, 0, -1 }));
    mesh_subset_free(tags);
}

void
lib_mc_mesh_direc(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 direc;
    LIBMC_VEC3(direc, 3);

    LIBMC_SELECT(tags, 0);
    executor->return_register =
        double_init(executor, mesh_direction(tags, direc));
    mesh_subset_free(tags);
}

struct vec3
lib_mc_mesh_vec3_center(
    struct timeline_execution_context *executor, struct mesh_tag_subset tags
)
{
    struct vec3 const ret = {
        (mesh_direction(tags, (struct vec3){ 1, 0, 0 }) -
         mesh_direction(tags, (struct vec3){ -1, 0, 0 })) /
            2,
        (mesh_direction(tags, (struct vec3){ 0, 1, 0 }) -
         mesh_direction(tags, (struct vec3){ 0, -1, 0 })) /
            2,
        (mesh_direction(tags, (struct vec3){ 0, 0, 1 }) -
         mesh_direction(tags, (struct vec3){ 0, 0, -1 })) /
            2,
    };
    return ret;
}

struct vec3
lib_mc_mesh_vec3_center_fields(
    struct timeline_execution_context *executor, struct vector_field *fields
)
{
    struct vec3 const err = { FP_NAN, FP_NAN, FP_NAN };
    LIBMC_SELECT_RETURN(tags, 0, return err);
    struct vec3 const ret = lib_mc_mesh_vec3_center(executor, tags);
    mesh_subset_free(tags);
    return ret;
}

void
lib_mc_mesh_center(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(tags, 0);

    struct vec3 const v = lib_mc_mesh_vec3_center(executor, tags);

    struct vector_field const ret = vector_init(executor);
    struct vector_field x = double_init(executor, v.x);
    vector_plus(executor, ret, &x);
    x = double_init(executor, v.y);
    vector_plus(executor, ret, &x);
    x = double_init(executor, v.z);
    vector_plus(executor, ret, &x);

    executor->return_register = ret;
    mesh_subset_free(tags);
}

static inline void
vector_push_vec3(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vec3 v
)
{
    struct vector_field sub = vector_init(executor);
    struct vector_field aux = double_init(executor, v.x);
    vector_plus(executor, sub, &aux);
    aux = double_init(executor, v.y);
    vector_plus(executor, sub, &aux);
    aux = double_init(executor, v.z);
    vector_plus(executor, sub, &aux);
}

void
lib_mc_mesh_vertex_set(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(mesh, 0);

    struct vector_field ret = vector_init(executor);

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *tag = mesh.meshes[i];
        for (int32_t j = 0; j < (int32_t) tag->dot_count; ++j) {
            if (j < tag->dots[j].inverse && j < tag->dots[j].antinorm) {
                vector_push_vec3(executor, ret, tag->dots[j].pos);
            }
        }

        for (int32_t j = 0; j < (int32_t) tag->lin_count; ++j) {
            if (tag->lins[j].is_dominant_sibling && j < tag->lins[j].antinorm &&
                tag->lins[j].inverse >= 0) {
                if (j > tag->lins[j].next) {
                    vector_push_vec3(executor, ret, tag->lins[j].b.pos);
                }
                if (j > tag->lins[j].prev) {
                    vector_push_vec3(executor, ret, tag->lins[j].a.pos);
                }
            }
        }

        // some double count for this one...
        for (int32_t j = 0; j < (int32_t) tag->tri_count; ++j) {
            if (tag->tris[j].is_dominant_sibling) {
                if (j > tag->tris[j].ab || j > tag->tris[j].ca) {
                    vector_push_vec3(executor, ret, tag->tris[j].a.pos);
                }
                if (j > tag->tris[j].ab || j > tag->tris[j].bc) {
                    vector_push_vec3(executor, ret, tag->tris[j].b.pos);
                }
                if (j > tag->tris[j].bc || j > tag->tris[j].ca) {
                    vector_push_vec3(executor, ret, tag->tris[j].c.pos);
                }
            }
        }
    }

    executor->return_register = ret;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_edge_set(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{

    LIBMC_SELECT(mesh, 0);

    struct vector_field ret = vector_init(executor);

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *tag = mesh.meshes[i];

        for (int32_t j = 0; j < (int32_t) tag->lin_count; ++j) {
            if (tag->lins[j].is_dominant_sibling && j < tag->lins[j].antinorm &&
                tag->lins[j].inverse >= 0) {
                vector_push_vec3(executor, ret, tag->lins[j].a.pos);
                vector_push_vec3(executor, ret, tag->lins[j].b.pos);
            }
        }

        for (int32_t j = 0; j < (int32_t) tag->tri_count; ++j) {
            if (tag->tris[j].is_dominant_sibling) {
                if (j > tag->tris[j].ab) {
                    vector_push_vec3(executor, ret, tag->tris[j].a.pos);
                    vector_push_vec3(executor, ret, tag->tris[j].b.pos);
                }
                if (j > tag->tris[j].bc) {
                    vector_push_vec3(executor, ret, tag->tris[j].b.pos);
                    vector_push_vec3(executor, ret, tag->tris[j].c.pos);
                }
                if (j > tag->tris[j].ca) {
                    vector_push_vec3(executor, ret, tag->tris[j].c.pos);
                    vector_push_vec3(executor, ret, tag->tris[j].a.pos);
                }
            }
        }
    }

    executor->return_register = ret;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_triangle_set(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(mesh, 0);

    struct vector_field ret = vector_init(executor);

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *tag = mesh.meshes[i];

        // some double count for this one...
        for (int32_t j = 0; j < (int32_t) tag->tri_count; ++j) {
            if (tag->tris[j].is_dominant_sibling) {
                vector_push_vec3(executor, ret, tag->tris[j].a.pos);
                vector_push_vec3(executor, ret, tag->tris[j].b.pos);
                vector_push_vec3(executor, ret, tag->tris[j].c.pos);
            }
        }
    }

    executor->return_register = ret;
    mesh_subset_free(mesh);
}

static void
write_lin(
    struct tetra_lin **lin_p, struct tetra_dot **dot_p, mc_count_t *lin_count_p,
    struct vec3 start, struct vec3 end, struct vec3 norm
)
{
    struct vec3 const antinorm = vec3_mul_scalar(-1, norm);

    int32_t const lin_count = (int32_t) *lin_count_p;
    int32_t const dot_count = lin_count;

    MC_MEM_RESERVEN(*lin_p, *lin_count_p, 4);
    MC_MEM_RESERVEN(*dot_p, *lin_count_p, 4);

    struct tetra_lin *lin = *lin_p;
    struct tetra_dot *dot = *dot_p;

    norm = (struct vec3){ 0, 0, 1 };

    dot[dot_count] = (struct tetra_dot){
        .pos = start,
        .col = VEC4_0,
        .norm = norm,
        .inverse = -1 - lin_count,
        .antinorm = 2 + dot_count,
        .is_dominant_sibling = 1,
    };
    dot[dot_count + 1] = (struct tetra_dot){
        .pos = end,
        .col = VEC4_0,
        .norm = norm,
        .inverse = -1 - lin_count,
        .antinorm = 3 + dot_count,
        .is_dominant_sibling = 0,
    };
    dot[dot_count + 2] = (struct tetra_dot){
        .pos = start,
        .col = VEC4_0,
        .norm = antinorm,
        .inverse = -3 - lin_count,
        .antinorm = 0 + dot_count,
        .is_dominant_sibling = 1,
    };
    dot[dot_count + 3] = (struct tetra_dot){
        .pos = end,
        .col = VEC4_0,
        .norm = antinorm,
        .inverse = -3 - lin_count,
        .antinorm = 1 + dot_count,
        .is_dominant_sibling = 0,
    };
    lin[lin_count + 0] = (struct tetra_lin){
        .a = { start, VEC4_1 },
        .b = { end, VEC4_1 },
        norm,
        .prev = -1 - dot_count,
        .next = -2 - dot_count,
        .inverse = 1 + lin_count,
        .antinorm = 2 + lin_count,
        .is_dominant_sibling = 1,
    };
    lin[lin_count + 1] = (struct tetra_lin){
        .a = { end, VEC4_1 },
        .b = { start, VEC4_1 },
        norm,
        .prev = -2 - dot_count,
        .next = -1 - dot_count,
        .inverse = 0 + lin_count,
        .antinorm = 3 + lin_count,
        .is_dominant_sibling = 0,
    };
    lin[lin_count + 2] = (struct tetra_lin){
        .a = { end, VEC4_1 },
        .b = { start, VEC4_1 },
        antinorm,
        .prev = -4 - dot_count,
        .next = -3 - dot_count,
        .inverse = 3 + lin_count,
        .antinorm = 0 + lin_count,
        .is_dominant_sibling = 1,
    };
    lin[lin_count + 3] = (struct tetra_lin){
        .a = { start, VEC4_1 },
        .b = { end, VEC4_1 },
        antinorm,
        .prev = -3 - dot_count,
        .next = -2 - dot_count,
        .inverse = 2 + lin_count,
        .antinorm = 1 + lin_count,
        .is_dominant_sibling = 0,
    };

    *lin_count_p += 4;
}

void
lib_mc_mesh_wireframe(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_SELECT(mesh, 0);

    mc_count_t lin_count = 0;
    struct tetra_lin *lins = NULL;
    struct tetra_dot *dots = NULL;
    // doing an accurate geometry / half edge structure
    // would involve creating meshes that some algorithms do not like
    // so it is not performed

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *tag = mesh.meshes[i];

        // add in all non bordering lines
        for (int32_t j = 0; j < (int32_t) tag->lin_count; ++j) {
            if (tag->lins[j].is_dominant_sibling && tag->lins[j].inverse >= 0 &&
                j < tag->lins[j].antinorm) {
                write_lin(
                    &lins, &dots, &lin_count, tag->lins[j].a.pos,
                    tag->lins[j].b.pos, tag->lins[j].norm
                );
            }
        }

        // add in from triangles
        for (int32_t j = 0; j < (int32_t) tag->tri_count; ++j) {
            if (tag->tris[j].is_dominant_sibling) {
                if (j > tag->tris[j].ab) {
                    write_lin(
                        &lins, &dots, &lin_count, tag->tris[j].a.pos,
                        tag->tris[j].b.pos, tag->tris[j].a.norm
                    );
                }
                if (j > tag->tris[j].bc) {
                    write_lin(
                        &lins, &dots, &lin_count, tag->tris[j].b.pos,
                        tag->tris[j].c.pos, tag->tris[j].b.norm
                    );
                }
                if (j > tag->tris[j].ca) {
                    write_lin(
                        &lins, &dots, &lin_count, tag->tris[j].c.pos,
                        tag->tris[j].a.pos, tag->tris[j].c.norm
                    );
                }
            }
        }
    }

    struct vector_field const out = tetramesh_init(executor);
    struct tetramesh *tag = out.value.pointer;
    tag->uniform = STANDARD_UNIFORM;
    tag->dots = dots;
    tag->dot_count = lin_count;
    tag->lins = lins;
    tag->lin_count = lin_count;

    executor->return_register = out;

    mesh_subset_free(mesh);
}

static void
lerp_uniform_and_colors(
    struct tetramesh const *a, struct tetramesh *dump,
    struct tetramesh *const b, float t
)
{
    float const t_prime = 1 - t;

    for (mc_ind_t i = 0; i < a->tri_count; ++i) {
        dump->tris[i].a.norm =
            vec3_norm_lerp(a->tris[i].a.norm, t, b->tris[i].a.norm);
        dump->tris[i].a.col = vec4_lerp(a->tris[i].a.col, t, b->tris[i].a.col);
        dump->tris[i].a.uv = vec2_lerp(a->tris[i].a.uv, t, b->tris[i].a.uv);

        dump->tris[i].b.norm =
            vec3_norm_lerp(a->tris[i].b.norm, t, b->tris[i].b.norm);
        dump->tris[i].b.col = vec4_lerp(a->tris[i].b.col, t, b->tris[i].b.col);
        dump->tris[i].b.uv = vec2_lerp(a->tris[i].b.uv, t, b->tris[i].b.uv);

        dump->tris[i].c.norm =
            vec3_norm_lerp(a->tris[i].c.norm, t, b->tris[i].c.norm);
        dump->tris[i].c.col = vec4_lerp(a->tris[i].c.col, t, b->tris[i].c.col);
        dump->tris[i].c.uv = vec2_lerp(a->tris[i].c.uv, t, b->tris[i].c.uv);
    }

    for (mc_ind_t i = 0; i < a->lin_count; ++i) {
        dump->lins[i].a.col = vec4_lerp(a->lins[i].a.col, t, b->lins[i].a.col);
        dump->lins[i].b.col = vec4_lerp(a->lins[i].b.col, t, b->lins[i].b.col);
        dump->lins[i].norm =
            vec3_norm_lerp(a->lins[i].norm, t, b->lins[i].norm);
    }

    for (mc_ind_t i = 0; i < a->dot_count; ++i) {
        dump->dots[i].col = vec4_lerp(a->dots[i].col, t, b->dots[i].col);
        dump->dots[i].norm =
            vec3_norm_lerp(a->dots[i].norm, t, b->dots[i].norm);
    }

    /* uniform */
    dump->uniform.opacity =
        t_prime * a->uniform.opacity + t * b->uniform.opacity;
    dump->uniform.dot_radius =
        t_prime * a->uniform.dot_radius + t * b->uniform.dot_radius;
    dump->uniform.dot_vertex_count =
        (unsigned short) (t_prime * a->uniform.dot_vertex_count +
                          t * b->uniform.dot_vertex_count);
    dump->uniform.gloss = t_prime * a->uniform.gloss + t * b->uniform.gloss;
    dump->uniform.smooth = t < 0.5 ? a->uniform.smooth : b->uniform.smooth;
    dump->uniform.stroke_miter_radius_scale =
        t_prime * a->uniform.stroke_miter_radius_scale +
        t * b->uniform.stroke_miter_radius_scale;
    dump->uniform.stroke_radius =
        t_prime * a->uniform.stroke_radius + t * b->uniform.stroke_radius;
    dump->uniform.z_class =
        t_prime * a->uniform.z_class + t * b->uniform.z_class;

    dump->texture_handle = t < 0.5 ? a->texture_handle : b->texture_handle;
}

void
mesh_patharc_lerp(
    struct tetramesh const *a, struct tetramesh *dump,
    struct tetramesh *const b, float t, struct vec3 path_arc
)
{
    if (t < 0) {
        t = 0;
    }
    lerp_uniform_and_colors(a, dump, b, t);

    // ok i think best way is to have each vertex map to a vertex in output?,
    // then patharcing is easy as fuck
    //  honestly yeah
    for (mc_ind_t i = 0; i < a->tri_count; ++i) {
        dump->tris[i].a.pos =
            vec3_patharc_lerp(a->tris[i].a.pos, t, b->tris[i].a.pos, path_arc);
        dump->tris[i].b.pos =
            vec3_patharc_lerp(a->tris[i].b.pos, t, b->tris[i].b.pos, path_arc);
        dump->tris[i].c.pos =
            vec3_patharc_lerp(a->tris[i].c.pos, t, b->tris[i].c.pos, path_arc);
    }

    for (mc_ind_t i = 0; i < a->lin_count; ++i) {
        dump->lins[i].a.pos =
            vec3_patharc_lerp(a->lins[i].a.pos, t, b->lins[i].a.pos, path_arc);

        dump->lins[i].b.pos =
            vec3_patharc_lerp(a->lins[i].b.pos, t, b->lins[i].b.pos, path_arc);
    }

    for (mc_ind_t i = 0; i < a->dot_count; ++i) {
        dump->dots[i].pos =
            vec3_patharc_lerp(a->dots[i].pos, t, b->dots[i].pos, path_arc);
    }
}

static inline float
clamp(float f)
{
    if (f < -1) {
        return -1;
    }
    else if (f > 1) {
        return 1;
    }
    else {
        return f;
    }
}

static struct vec3
apply_matrix(
    struct vec3 ihat, struct vec3 jhat, struct vec3 khat, struct vec3 targ
)
{
    return vec3_add(
        vec3_mul_scalar(targ.x, ihat),
        vec3_add(vec3_mul_scalar(targ.y, jhat), vec3_mul_scalar(targ.z, khat))
    );
}

void
mesh_apply_matrix(
    struct mesh_tag_subset mesh, struct vec3 ihat, struct vec3 jhat,
    struct vec3 khat
)
{
    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *targ = mesh.meshes[i];
        for (mc_ind_t j = 0; j < targ->dot_count; ++j) {
            targ->dots[j].pos =
                apply_matrix(ihat, jhat, khat, targ->dots[j].pos);
        }
        for (mc_ind_t j = 0; j < targ->lin_count; ++j) {
            targ->lins[j].a.pos =
                apply_matrix(ihat, jhat, khat, targ->lins[j].a.pos);
            targ->lins[j].b.pos =
                apply_matrix(ihat, jhat, khat, targ->lins[j].b.pos);
        }
        for (mc_ind_t j = 0; j < targ->tri_count; ++j) {
            targ->tris[j].a.pos =
                apply_matrix(ihat, jhat, khat, targ->tris[j].a.pos);
            targ->tris[j].b.pos =
                apply_matrix(ihat, jhat, khat, targ->tris[j].b.pos);
            targ->tris[j].c.pos =
                apply_matrix(ihat, jhat, khat, targ->tris[j].c.pos);
        }
    }
}

void
mesh_bend_lerp(
    struct tetramesh const *a, struct tetramesh *dump,
    struct tetramesh *const b, float t
)
{
    if (t < 0) {
        t = 0;
    }

    lerp_uniform_and_colors(a, dump, b, t);
    // for now, only 1d to 1d...
    // use index 0 as pivot
    // then find starting and ending angles (if applicable) of each edge
    // interpolate length as well
    // interpolate ccw in terms of normal
    int32_t j = 0;
    while (a->lins[j].prev > 0) {
        j = a->lins[j].prev;
    }

    int32_t const anti_half = j;
    // halfway point
    for (mc_ind_t i = 0; i < a->lin_count / 8; ++i) {
        j = a->lins[j].next;
    }

    // ok we have to make the theta apply everywhere
    // and then the delta theta is just difference between first and last angles
    // and then final thing is when sin theta is super small, just go in
    // direction of normal lets just do that for now, should work in majority of
    // cases...
    struct vec3 rot = { 0, 0, 0 };

    // ok last thing is to have two of these. that'll make it super nice
    mc_count_t count = 0;
    for (int32_t k = j;; k = a->lins[k].next) {
        int32_t const prev = a->lins[k].prev;
        int32_t const next = a->lins[k].next;

        struct vec3 prev_a, prev_b;

        count++;

        if (k == j) {
            /* interpolate start */
            dump->lins[k].a.pos =
                vec3_lerp(a->lins[k].a.pos, t, b->lins[k].a.pos);
            prev_a = prev_b =
                vec3_unit(vec3_sub(a->lins[k].b.pos, a->lins[k].a.pos));
        }
        else {
            prev_a =
                vec3_unit(vec3_sub(a->lins[prev].b.pos, a->lins[prev].a.pos));
            prev_b =
                vec3_unit(vec3_sub(b->lins[prev].b.pos, b->lins[prev].a.pos));
        }

        struct vec3 const a_delta =
            vec3_sub(a->lins[k].b.pos, a->lins[k].a.pos);
        struct vec3 const b_delta =
            vec3_sub(b->lins[k].b.pos, b->lins[k].a.pos);

        struct vec3 const a_unit = vec3_unit(a_delta);
        struct vec3 const b_unit = vec3_unit(b_delta);

        struct vec3 const a_cross = vec3_cross(prev_a, a_unit);
        struct vec3 const b_cross = vec3_cross(prev_b, b_unit);

        float const a_dot = clamp(vec3_dot(a_unit, prev_a));
        float const b_dot = clamp(vec3_dot(b_unit, prev_b));

        float a_alpha = acosf(a_dot);
        float b_alpha = acosf(b_dot);

        /* do we have to go left or right? */
        if (vec3_dot(a->lins[k].norm, a_cross) < 0) {
            a_alpha = -a_alpha;
        }
        if (vec3_dot(a->lins[k].norm, b_cross) < 0) {
            b_alpha = -b_alpha;
        }

        struct vec3 const my_rot =
            vec3_mul_scalar(t * (b_alpha - a_alpha), a->lins[k].norm);

        /* technically if theres multiple normals this does not work, but i
           think if the entire mesh has the same normal, it's fine?
         */
        rot = vec3_add(rot, my_rot);

        struct vec3 const rot_axis = vec3_unit(rot);
        float const rot_alpha = vec3_norm(rot);

        /* rotate by rot */
        struct vec3 const unit_delta =
            vec3_rotate_about_axis(a_unit, rot_axis, rot_alpha);

        float const norm =
            (1 - t) * vec3_norm(a_delta) + t * vec3_norm(b_delta);

        dump->lins[k].b.pos =
            vec3_add(dump->lins[k].a.pos, vec3_mul_scalar(norm, unit_delta));

        if (next >= 0) {
            dump->lins[next].a.pos = dump->lins[k].b.pos;
        }
        if (next < 0 || next == anti_half) {
            break;
        }
    }

    rot = (struct vec3){ 0, 0, 0 };

    for (int32_t k = a->lins[j].prev; k >= 0; k = a->lins[k].prev) {
        int32_t const next = a->lins[k].next >= 0 ? a->lins[k].next : k;

        count++;

        struct vec3 next_a, next_b;
        if (k == next || j == next) {
            next_a = next_b =
                vec3_unit(vec3_sub(a->lins[k].a.pos, a->lins[k].b.pos));
        }
        else {
            next_a =
                vec3_unit(vec3_sub(a->lins[next].a.pos, a->lins[next].b.pos));
            next_b =
                vec3_unit(vec3_sub(b->lins[next].a.pos, b->lins[next].b.pos));
        }

        struct vec3 const a_delta =
            vec3_sub(a->lins[k].a.pos, a->lins[k].b.pos);
        struct vec3 const b_delta =
            vec3_sub(b->lins[k].a.pos, b->lins[k].b.pos);

        struct vec3 const a_unit = vec3_unit(a_delta);
        struct vec3 const b_unit = vec3_unit(b_delta);

        struct vec3 const a_cross = vec3_cross(next_a, a_unit);
        struct vec3 const b_cross = vec3_cross(next_b, b_unit);

        float const a_dot = clamp(vec3_dot(a_unit, next_a));
        float const b_dot = clamp(vec3_dot(b_unit, next_b));

        float a_alpha = acosf(a_dot);
        float b_alpha = acosf(b_dot);

        /* do we have to go left or right? */
        if (vec3_dot(a->lins[k].norm, a_cross) < 0) {
            a_alpha = -a_alpha;
        }
        if (vec3_dot(a->lins[k].norm, b_cross) < 0) {
            b_alpha = -b_alpha;
        }

        struct vec3 const my_rot =
            vec3_mul_scalar(t * (b_alpha - a_alpha), a->lins[k].norm);

        rot = vec3_add(rot, my_rot);

        struct vec3 const rot_axis = vec3_unit(rot);
        float const rot_alpha = vec3_norm(rot);

        /* rotate by rot */
        struct vec3 const unit_delta =
            vec3_rotate_about_axis(a_unit, rot_axis, rot_alpha);

        float const norm =
            (1 - t) * vec3_norm(a_delta) + t * vec3_norm(b_delta);

        dump->lins[k].b.pos = dump->lins[next].a.pos;
        dump->lins[k].a.pos =
            vec3_add(dump->lins[k].b.pos, vec3_mul_scalar(norm, unit_delta));

        if (k == anti_half) {
            break;
        }
    }

    /* copy information to peers */
    for (mc_ind_t k = (mc_ind_t) j;; k = (mc_ind_t) a->lins[k].next) {
        struct tetra_lin const curr = dump->lins[k];

        dump->lins[curr.antinorm].a.pos = curr.b.pos;
        dump->lins[curr.antinorm].b.pos = curr.a.pos;
        count++;

        if (curr.inverse >= 0) {
            dump->lins[curr.inverse].a.pos = curr.b.pos;
            dump->lins[curr.inverse].b.pos = curr.a.pos;
            dump->lins[dump->lins[curr.inverse].antinorm].a.pos = curr.a.pos;
            dump->lins[dump->lins[curr.inverse].antinorm].b.pos = curr.b.pos;
            count += 2;
        }

        if (dump->lins[k].next < 0 || dump->lins[k].next == anti_half) {
            break;
        }
    }

    for (int32_t k = a->lins[j].prev; k >= 0; k = a->lins[k].prev) {
        struct tetra_lin const curr = dump->lins[k];

        dump->lins[curr.antinorm].a.pos = curr.b.pos;
        dump->lins[curr.antinorm].b.pos = curr.a.pos;
        count++;

        if (curr.inverse >= 0) {
            dump->lins[curr.inverse].a.pos = curr.b.pos;
            dump->lins[curr.inverse].b.pos = curr.a.pos;
            dump->lins[dump->lins[curr.inverse].antinorm].a.pos = curr.a.pos;
            dump->lins[dump->lins[curr.inverse].antinorm].b.pos = curr.b.pos;
            count += 2;
        }

        if (k == anti_half) {
            break;
        }
    }

    for (mc_ind_t i = 0; i < dump->dot_count; ++i) {
        struct tetra_lin const pair = dump->lins[-1 - dump->dots[i].inverse];
        if (-1 - pair.next == (int32_t) i) {
            dump->dots[i].pos = pair.b.pos;
        }
        else {
            dump->dots[i].pos = pair.a.pos;
        }
    }
}

void
mesh_rotate(
    struct tetramesh *dump, struct tetramesh *curr, struct vec3 com,
    struct vec3 rotation, float alpha
)
{
    for (mc_ind_t j = 0; j < curr->dot_count; ++j) {
        dump->dots[j].pos = vec3_add(
            vec3_rotate_about_axis(
                vec3_sub(curr->dots[j].pos, com), rotation, alpha
            ),
            com
        );
    }

    for (mc_ind_t j = 0; j < curr->lin_count; ++j) {
        dump->lins[j].a.pos = vec3_add(
            vec3_rotate_about_axis(
                vec3_sub(curr->lins[j].a.pos, com), rotation, alpha
            ),
            com
        );
        dump->lins[j].b.pos = vec3_add(
            vec3_rotate_about_axis(
                vec3_sub(curr->lins[j].b.pos, com), rotation, alpha
            ),
            com
        );
    }

    for (mc_ind_t j = 0; j < curr->tri_count; ++j) {
        dump->tris[j].a.pos = vec3_add(
            vec3_rotate_about_axis(
                vec3_sub(curr->tris[j].a.pos, com), rotation, alpha
            ),
            com
        );
        dump->tris[j].b.pos = vec3_add(
            vec3_rotate_about_axis(
                vec3_sub(curr->tris[j].b.pos, com), rotation, alpha
            ),
            com
        );
        dump->tris[j].c.pos = vec3_add(
            vec3_rotate_about_axis(
                vec3_sub(curr->tris[j].c.pos, com), rotation, alpha
            ),
            com
        );
    }
}

void
lib_mc_mesh_tag_apply(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(apply, 3, VECTOR_FIELD_TYPE_FUNCTION);
    LIBMC_SELECT(mesh, 0);

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        function_call(executor, apply, 1, &mesh.sources[i]);
        if (!executor->return_register.vtable) {
            mesh_subset_free(mesh);
            return;
        }
        VECTOR_FIELD_FREE(executor, executor->return_register);
    }

    executor->return_register = fields[1];
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_bounding_box(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(buff_index, 3, VECTOR_FIELD_TYPE_DOUBLE);
    float buff = 0.05f;
    if (buff_index.value.doub == 1) {
        LIBMC_FULL_CAST(buffer, 4, VECTOR_FIELD_TYPE_DOUBLE);
        buff = (float) buffer.value.doub;
    }
    LIBMC_SELECT(mesh, 0);

    float const right = mesh_direction(mesh, (struct vec3){ 1, 0, 0 }) + buff;
    float const left = -mesh_direction(mesh, (struct vec3){ -1, 0, 0 }) - buff;
    float const up = mesh_direction(mesh, (struct vec3){ 0, 1, 0 }) + buff;
    float const down = -mesh_direction(mesh, (struct vec3){ 0, -1, 0 }) - buff;
    float const z = (mesh_direction(mesh, (struct vec3){ 0, 0, 1 }) -
                     mesh_direction(mesh, (struct vec3){ 0, 0, -1 })) /
                    2;

    mesh_subset_free(mesh);

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const ret = field.value.pointer;
    ret->uniform = STANDARD_UNIFORM;

    struct vec3 const a = {
        right,
        up,
        z,
    };
    struct vec3 const b = {
        left,
        up,
        z,
    };
    struct vec3 const c = {
        left,
        down,
        z,
    };
    struct vec3 const d = {
        right,
        down,
        z,
    };

    tetramesh_line(ret, a, b, (struct vec3){ 0, 0, 1 });
    tetramesh_line_to(ret, c);
    tetramesh_line_to(ret, d);
    tetramesh_line_to(ret, a);
    tetramesh_line_close(ret);

    if (libmc_tag_and_color2(executor, ret, &fields[5]) != 0) {
        return;
    }

    executor->return_register = field;
}

/* for checksums in transfers */
void
lib_mc_mesh_hash(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(index, 0, VECTOR_FIELD_TYPE_DOUBLE);

    if (index.value.doub == 0) {
        struct vector_field ret = double_init(executor, 0);
        ret.value.hash = VECTOR_FIELD_HASH(executor, fields[1]);
        executor->return_register = ret;
    }
    else {
        LIBMC_SELECT(mesh, 0);

        struct vector_field flat = vector_init(executor);
        for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
            vector_literal_plus(executor, flat, &mesh.sources[i]);
        }

        struct vector_field ret = double_init(executor, 0);
        ret.value.hash = VECTOR_FIELD_HASH(executor, flat);
        executor->return_register = ret;

        VECTOR_FIELD_FREE(executor, flat);

        mesh_subset_free(mesh);
    }

    if (!executor->return_register.value.hash) {
        executor->return_register = VECTOR_FIELD_NULL;
    }
}
