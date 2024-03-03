//
//  anim_show.c
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include "anim_show.h"
#include "mc_anims.h"
#include "mc_memory.h"
#include "mc_meshes.h"
#include "mesh_util.h"

#define WRITE_LAG_RATIO 0.075
#define WRITE_SUBCONTOUR_LAG_RATIO 0.1
/* 0.2 overlap */
#define WRITE_BOUNDARY_HEADSTART 0.3

#define PREFIX                                                                 \
    ANIM_TIME(_raw_t, 2);                                                      \
    LIBMC_FULL_CAST(reverse, 6, VECTOR_FIELD_TYPE_DOUBLE);                     \
    float const t = reverse.value.doub != 0 ? 1 - _raw_t : _raw_t;             \
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
    executor->return_register = double_init(executor, _raw_t >= 1)

static int
hash_compare(void const *x, void const *y)
{
    struct tetramesh *const *a = x;
    struct tetramesh *const *b = y;
    struct vector_field av = tetramesh_init_ptr(NULL, *a);
    struct vector_field bv = tetramesh_init_ptr(NULL, *b);
    mc_hash_t const a_hash = tetramesh_hash(NULL, av);
    mc_hash_t const b_hash = tetramesh_hash(NULL, bv);

    if (a_hash > b_hash) {
        return 1;
    }
    else if (a_hash < b_hash) {
        return -1;
    }
    else {
        return (int) (*a)->payload - (int) (*b)->payload;
    }
}

static int
index_compare(void const *x, void const *y)
{
    struct vector_field const *amesh = x;
    struct vector_field const *bmesh = y;
    struct tetramesh *const a = amesh->value.pointer;
    struct tetramesh *const b = bmesh->value.pointer;
    return (int) a->payload - (int) b->payload;
}

static void
reorder(struct vector_field vector)
{
    struct vector *vec = vector.value.pointer;
    qsort(
        vec->fields, vec->field_count, sizeof(struct vector_field),
        index_compare
    );
}

/* a is previous b is current */
static struct vector_field
separate(
    struct timeline_execution_context *executor, struct mesh_tag_subset a,
    struct mesh_tag_subset b
)
{
    /* based on hashes of first */
    /* and hashes of second, find reads and writes, and unchanged */
    for (mc_ind_t i = 0; i < a.subset_count; ++i) {
        a.meshes[i]->payload = i;
        /* initiate hash cache */
        VECTOR_FIELD_HASH(executor, a.sources[i]);
    }
    qsort(a.meshes, a.subset_count, sizeof(a.meshes[0]), hash_compare);
    for (mc_ind_t i = 0; i < b.subset_count; ++i) {
        b.meshes[i]->payload = i;
        VECTOR_FIELD_HASH(executor, b.sources[i]);
    }
    qsort(b.meshes, b.subset_count, sizeof(b.meshes[0]), hash_compare);

    struct vector_field insert = vector_init(executor);
    struct vector_field delete = vector_init(executor);
    struct vector_field constants = vector_init(executor);
    mc_ind_t i = 0, j = 0;
    while (i < a.subset_count || j < b.subset_count) {
        if (i < a.subset_count && j < b.subset_count &&
            a.meshes[i]->hash_cache == b.meshes[j]->hash_cache) {
            /* constant */
            struct vector_field join = VECTOR_FIELD_COPY(
                executor, tetramesh_init_ptr(executor, a.meshes[i])
            );
            vector_plus(executor, constants, &join);

            ++i;
            ++j;
        }
        else if (i == a.subset_count || (j < b.subset_count && a.meshes[i]->hash_cache > b.meshes[j]->hash_cache)) {
            struct vector_field join = VECTOR_FIELD_COPY(
                executor, tetramesh_init_ptr(executor, b.meshes[j])
            );
            vector_plus(executor, insert, &join);
            j++;
        }
        else {
            struct vector_field join = VECTOR_FIELD_COPY(
                executor, tetramesh_init_ptr(executor, a.meshes[i])
            );
            vector_plus(executor, delete, &join);
            i++;
        }
    }

    reorder(insert);
    reorder(delete);
    reorder(constants);

    struct vector_field out = vector_init(executor);
    vector_plus(executor, out, &insert);
    vector_plus(executor, out, &delete);
    vector_plus(executor, out, &constants);

    return out;
}

void
lib_mc_showhide_decomp(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct mesh_tag_subset follower = mesh_fullset(executor, fields[0]);
    if (follower.total_count == SIZE_MAX) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct mesh_tag_subset iterator = mesh_fullset(executor, fields[1]);
    if (iterator.total_count == SIZE_MAX) {
        mesh_subset_free(follower);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    executor->return_register = separate(executor, follower, iterator);
    mesh_subset_free(follower);
    mesh_subset_free(iterator);
}

void
lib_mc_grow(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    PREFIX;

    double const x = (mesh_direction(targ, (struct vec3){ 1, 0, 0 }) -
                      mesh_direction(targ, (struct vec3){ -1, 0, 0 })) /
                     2;
    double const y = (mesh_direction(targ, (struct vec3){ 0, 1, 0 }) -
                      mesh_direction(targ, (struct vec3){ 0, -1, 0 })) /
                     2;
    double const z = (mesh_direction(targ, (struct vec3){ 0, 0, 1 }) -
                      mesh_direction(targ, (struct vec3){ 0, 0, -1 })) /
                     2;

    struct vec3 const center = (struct vec3){ (float) x, (float) y, (float) z };

    for (mc_ind_t i = 0; i < curr.subset_count; ++i) {
        struct tetramesh *const tag = curr.meshes[i];
        struct tetramesh *const src = targ.meshes[i];

        for (mc_ind_t j = 0; j < tag->tri_count; ++j) {
            tag->tris[j].a.pos = vec3_add(
                center, vec3_mul_scalar(t, vec3_sub(src->tris[j].a.pos, center))
            );
            tag->tris[j].b.pos = vec3_add(
                center, vec3_mul_scalar(t, vec3_sub(src->tris[j].b.pos, center))
            );
            tag->tris[j].c.pos = vec3_add(
                center, vec3_mul_scalar(t, vec3_sub(src->tris[j].c.pos, center))
            );
        }

        for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
            tag->lins[j].a.pos = vec3_add(
                center, vec3_mul_scalar(t, vec3_sub(src->lins[j].a.pos, center))
            );
            tag->lins[j].b.pos = vec3_add(
                center, vec3_mul_scalar(t, vec3_sub(src->lins[j].b.pos, center))
            );
        }

        for (mc_ind_t j = 0; j < tag->dot_count; ++j) {
            tag->dots[j].pos = vec3_add(
                center, vec3_mul_scalar(t, vec3_sub(src->dots[j].pos, center))
            );
        }

        tag->modded = tag->dirty_hash_cache = 1;
    }

    SUFFIX;
}

// native fade_in(mesh_tree, pull, subtags, push, t, config, time, unit_map!,
// delta)
void
lib_mc_fade(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    PREFIX;

    LIBMC_FULL_CAST(config_ind, 3, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 delta = (struct vec3){ 0, 0, 0 };
    /* optional parameter */
    if (config_ind.value.doub == 1) {
        LIBMC_VEC3_RETURN(delta, 7, mesh_subset_free(curr);
                          mesh_subset_free(targ); return);
    }

    for (mc_ind_t i = 0; i < curr.subset_count; ++i) {
        struct tetramesh *const tag = curr.meshes[i];
        struct tetramesh *const src = targ.meshes[i];

        for (mc_ind_t j = 0; j < tag->tri_count; ++j) {
            tag->tris[j].a.pos =
                vec3_add(src->tris[j].a.pos, vec3_mul_scalar(t - 1, delta));
            tag->tris[j].b.pos =
                vec3_add(src->tris[j].b.pos, vec3_mul_scalar(t - 1, delta));
            tag->tris[j].c.pos =
                vec3_add(src->tris[j].c.pos, vec3_mul_scalar(t - 1, delta));
        }

        for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
            tag->lins[j].a.pos =
                vec3_add(src->lins[j].a.pos, vec3_mul_scalar(t - 1, delta));
            tag->lins[j].b.pos =
                vec3_add(src->lins[j].b.pos, vec3_mul_scalar(t - 1, delta));
        }

        for (mc_ind_t j = 0; j < tag->dot_count; ++j) {
            tag->dots[j].pos =
                vec3_add(src->dots[j].pos, vec3_mul_scalar(t - 1, delta));
        }

        tag->uniform.opacity = t * src->uniform.opacity;
        tag->modded = tag->dirty_hash_cache = 1;
    }

    SUFFIX;
}

#pragma message("TODO, inconsistent topologies during interpolation...")
void
write_interpolate(
    mc_ind_t i, mc_count_t subset, struct tetramesh *src, struct tetramesh *tag,
    float u, float v
)
{
    /* offset * (n - 1) + unit_length = 1 */
    /* offset = LAG * unit_length */
    /* unit * lag * (n - 1) + unit = 1 */
    /* unit *  = 1 / (lag * (n - 1) + 1) */
    double const anim_length = 1 / (WRITE_LAG_RATIO * (subset - 1) + 1);
    double sub_start = i * anim_length * WRITE_LAG_RATIO;
    double sub_end = sub_start + anim_length;

    if (tag->dot_count) {
        /* introduce directly */

        if (tag->lin_count) {
            sub_end -= WRITE_BOUNDARY_HEADSTART * anim_length;
        }

        if (v < sub_start || u > sub_end) {
            tag->uniform.dot_radius = 0;
        }
        else {
            tag->uniform.dot_radius = src->uniform.dot_radius;

            double sub_u = (u - sub_start) / (sub_end - sub_start);
            if (sub_u < 0) {
                sub_u = 0;
            }
            double sub_v = (v - sub_start) / (sub_end - sub_start);
            if (sub_v > 1) {
                sub_v = 1;
            }
            double const sub_t = sub_v - sub_u;

            double const unit_length =
                1 / (WRITE_SUBCONTOUR_LAG_RATIO * (tag->dot_count - 1) + 1);
            /* based on dot count... */
            for (mc_ind_t j = 0; j < tag->dot_count; ++j) {
                double full_t =
                    (sub_t - j * WRITE_SUBCONTOUR_LAG_RATIO * unit_length) /
                    unit_length;
                if (full_t < 0) {
                    full_t = 0;
                }
                else if (full_t > 1) {
                    full_t = 1;
                }
                tag->dots[j].col.w = (float) full_t * src->dots[j].col.w;
            }
        }
    }

    sub_start = i * anim_length * WRITE_LAG_RATIO;
    sub_end = sub_start + anim_length;

    if (tag->lin_count) {
        double const raw_end = sub_end;

        if (tag->dot_count) {
            sub_start += WRITE_BOUNDARY_HEADSTART * anim_length;
        }
        if (tag->tri_count) {
            sub_end -= WRITE_BOUNDARY_HEADSTART * anim_length;
        }

        if (v < sub_start || u > sub_end) {
            tag->uniform.stroke_radius = 0;
        }
        else if (u < sub_end) {
            double sub_u = (u - sub_start) / (sub_end - sub_start);
            if (sub_u < 0) {
                sub_u = 0;
            }
            double sub_v = (v - sub_start) / (sub_end - sub_start);
            if (sub_v > 1) {
                sub_v = 1;
            }

            tag->uniform.stroke_radius = src->uniform.stroke_radius;

            /* contour count */
            mc_graph_color_t colors = 0;
            mc_graph_color_t *visited =
                mc_calloc(tag->lin_count, sizeof(mc_graph_color_t));

            if (!tag->tri_count) {
                for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
                    visited[j] = 1;
                }
                colors = 1;
            }
            else {
                for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
                    if (visited[j]) {
                        continue;
                    }

                    ++colors;

                    /* since it's a boundary lin, we can be assured that it's a
                     * closed loop */
                    /* therefore, we drop unnecessary endpoint checks */
                    int32_t k = (int32_t) j;
                    do {
                        struct tetra_lin const lin = src->lins[k];
                        visited[k] = colors;
                        visited[lin.antinorm] = colors;
                        if (lin.inverse >= 0) {
                            visited[lin.inverse] = colors;
                            visited[src->lins[lin.inverse].antinorm] = colors;
                        }
                        k = lin.next;
                    } while (!visited[k]);
                }
            }

            double const unit_length =
                1 / (WRITE_SUBCONTOUR_LAG_RATIO * (colors - 1) + 1);

            for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
                if (visited[j] < 0) {
                    continue;
                }

                int color = visited[j];
                double full_u =
                    (sub_u -
                     (color - 1) * WRITE_SUBCONTOUR_LAG_RATIO * unit_length) /
                    unit_length;
                double full_v =
                    (sub_v -
                     (color - 1) * WRITE_SUBCONTOUR_LAG_RATIO * unit_length) /
                    unit_length;
                if (full_u < 0) {
                    full_u = 0;
                }
                else if (full_u > 1) {
                    full_u = 1;
                }
                if (full_v < 0) {
                    full_v = 0;
                }
                else if (full_v > 1) {
                    full_v = 1;
                }

                int32_t k = (int32_t) j;
                while (src->lins[k].prev >= 0 &&
                       src->lins[k].prev != (int32_t) j) {
                    k = src->lins[k].prev;
                }
                int32_t const save = k;

                mc_ind_t ind = 0;
                do {
                    visited[k] = -1;
                    k = src->lins[k].next;
                    ++ind;

                } while (k >= 0 && visited[k] != -1);
                k = save;
                mc_count_t const count = ind;

                ind = 0;
                do {
#define LINE_LERP(k, a, b, next, prev)                                         \
    if (v < raw_end) {                                                         \
        tag->lins[k].a.col.w = 1;                                              \
        tag->lins[k].b.col.w = 1;                                              \
    }                                                                          \
    if ((float) (ind + 1) / count < full_v) {                                  \
        tag->lins[k].b.pos = src->lins[k].b.pos;                               \
        tag->lins[k].next = src->lins[k].next;                                 \
    }                                                                          \
    else if ((float) ind / count > full_v) {                                   \
        tag->lins[k].b.pos = src->lins[k].a.pos;                               \
        tag->lins[k].next = k;                                                 \
    }                                                                          \
    else {                                                                     \
        tag->lins[k].b.pos = vec3_lerp(                                        \
            src->lins[k].a.pos,                                                \
            (float) (full_v - (float) ind / count) / (1.0f / count),           \
            src->lins[k].b.pos                                                 \
        );                                                                     \
        tag->lins[k].next = k;                                                 \
    }                                                                          \
    if ((float) ind / count > full_u) {                                        \
        tag->lins[k].a.pos = src->lins[k].a.pos;                               \
        tag->lins[k].prev = src->lins[k].prev;                                 \
    }                                                                          \
    else if ((float) (ind + 1) / count < full_u) {                             \
        tag->lins[k].a.pos = src->lins[k].b.pos;                               \
        tag->lins[k].prev = k;                                                 \
    }                                                                          \
    else {                                                                     \
        tag->lins[k].a.pos = vec3_lerp(                                        \
            src->lins[k].a.pos,                                                \
            (float) (full_u - (float) ind / count) / (1.0f / count),           \
            src->lins[k].b.pos                                                 \
        );                                                                     \
        tag->lins[k].prev = k;                                                 \
    }                                                                          \
    visited[k] = -2

                    struct tetra_lin const lin = tag->lins[k];
                    LINE_LERP(k, a, b, next, prev);
                    LINE_LERP(lin.antinorm, b, a, prev, next);

                    if (lin.inverse >= 0) {
                        LINE_LERP(lin.inverse, b, a, prev, next);
                        int32_t const lin_anti =
                            tag->lins[lin.inverse].antinorm;
                        LINE_LERP(lin_anti, a, b, next, prev);
                    }

#undef LINE_LERP
                    k = src->lins[k].next;
                    ++ind;
                } while (k >= 0 && visited[k] != -2);
            }

            mc_free(visited);
        }

        if (v > sub_end && v < raw_end) {
            float const sub_t = (float) ((v - sub_end) / (raw_end - sub_end));
            /* interpolate back to source opacity if necessary */
            for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
                tag->lins[j].a.col.w =
                    (1 - sub_t) + sub_t * src->lins[j].a.col.w;
                tag->lins[j].b.col.w =
                    (1 - sub_t) + sub_t * src->lins[j].b.col.w;

                tag->lins[j].a.pos = src->lins[j].a.pos;
                tag->lins[j].b.pos = src->lins[j].b.pos;

                tag->lins[j].next = src->lins[j].next;
                tag->lins[j].prev = src->lins[j].prev;
            }
        }
    }

    sub_start = i * anim_length * WRITE_LAG_RATIO;
    sub_end = sub_start + anim_length;

    if (tag->tri_count) {
        if (tag->lin_count) {
            sub_start += WRITE_BOUNDARY_HEADSTART * anim_length;
        }

        if (v < sub_start || u > sub_end) {
            for (mc_ind_t j = 0; j < tag->tri_count; ++j) {
                tag->tris[j].a.col.w = 0;
                tag->tris[j].b.col.w = 0;
                tag->tris[j].c.col.w = 0;
            }
        }
        else {
            double sub_u = (u - sub_start) / (sub_end - sub_start);
            if (sub_u < 0) {
                sub_u = 0;
            }
            double sub_v = (v - sub_start) / (sub_end - sub_start);
            if (sub_v > 1) {
                sub_v = 1;
            }
            double const sub_t = sub_v - sub_u;

            for (mc_ind_t j = 0; j < tag->tri_count; ++j) {
                tag->tris[j].a.col.w = (float) (src->tris[j].a.col.w * sub_t);
                tag->tris[j].b.col.w = (float) (src->tris[j].a.col.w * sub_t);
                tag->tris[j].c.col.w = (float) (src->tris[j].a.col.w * sub_t);
            }
        }
    }

    tag->modded = tag->dirty_hash_cache = 1;
}

static void
write_like(float t, struct mesh_tag_subset curr, struct mesh_tag_subset targ)
{
    for (mc_ind_t i = 0; i < curr.subset_count; ++i) {
        struct tetramesh *const tag = curr.meshes[i];
        struct tetramesh *const src = targ.meshes[i];

        write_interpolate(i, curr.subset_count, src, tag, 0, t);
    }
}

void
lib_mc_write(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    PREFIX;

    write_like(t, curr, targ);

    SUFFIX;
}
