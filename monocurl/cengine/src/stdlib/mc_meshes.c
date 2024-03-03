//
//  mc_meshes.c
//  Monocurl
//
//  Created by Manu Bhat on 2/19/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <limits.h>
/* for windows */
#define _USE_MATH_DEFINES
#include <math.h>
#include <string.h>
#include "tesselator.h"

#include "lvalue.h"
#include "mc_memory.h"
#include "mc_meshes.h"
#include "vector.h"

#define MC_LOG_TAG "mc_meshes"
#include "mc_log.h"

#define STANDARD_STEP_RATE 0.5
#define MIN_STEP_RATE 0.01

/* something in the form of [points] {[main] {x_min, x_max, y_min, y_max},
 * [step] {x_min, x_max, y_min, y_max, x_step, y_step}, [mask] {x_min, x_max,
 * y_min, y_max, x_step, y_step, predicate(x,y)}, [domain] {domain,
 * resample_rate}} */
struct vec3_plane_covering
tetramesh_planar2d_sample(
    struct timeline_execution_context *executor, struct vector_field *fields
)
{
    struct vec3_plane_covering ret = { 0 };
    ret.count = SIZE_MAX;

    LIBMC_FULL_CAST_RETURN(
        points_index, 0, VECTOR_FIELD_TYPE_DOUBLE, return ret
    );

    if (points_index.value.doub <= 2) {
        double x_min, x_step, x_max;
        double y_min, y_step, y_max;

        LIBMC_FULL_CAST_RETURN(
            x_min_field, 1, VECTOR_FIELD_TYPE_DOUBLE, return ret
        );
        LIBMC_FULL_CAST_RETURN(
            x_max_field, 2, VECTOR_FIELD_TYPE_DOUBLE, return ret
        );
        LIBMC_FULL_CAST_RETURN(
            y_min_field, 3, VECTOR_FIELD_TYPE_DOUBLE, return ret
        );
        LIBMC_FULL_CAST_RETURN(
            y_max_field, 4, VECTOR_FIELD_TYPE_DOUBLE, return ret
        );

        x_min = x_min_field.value.doub;
        x_max = x_max_field.value.doub;
        y_min = y_min_field.value.doub;
        y_max = y_max_field.value.doub;

        if (points_index.value.doub >= 1) {
            LIBMC_FULL_CAST_RETURN(
                x_step_field, 5, VECTOR_FIELD_TYPE_DOUBLE, return ret
            );
            LIBMC_FULL_CAST_RETURN(
                y_step_field, 6, VECTOR_FIELD_TYPE_DOUBLE, return ret
            );
            x_step = x_step_field.value.doub;
            y_step = y_step_field.value.doub;
        }
        else {
            x_step = y_step = STANDARD_STEP_RATE;
        }

        if (x_min > x_max) {
            VECTOR_FIELD_ERROR(executor, "x_min should be <= x_max");
            executor->return_register = VECTOR_FIELD_NULL;
            return ret;
        }
        else if (y_min > y_max) {
            VECTOR_FIELD_ERROR(executor, "y_min should be <= y_max");
            executor->return_register = VECTOR_FIELD_NULL;
            return ret;
        }
        else if (x_step <= MIN_STEP_RATE || y_step <= MIN_STEP_RATE) {
            VECTOR_FIELD_ERROR(
                executor, "x_step and y_step should be >= %f", MIN_STEP_RATE
            );
            executor->return_register = VECTOR_FIELD_NULL;
            return ret;
        }

        ret.rows = (mc_count_t) ((y_max - y_min) / y_step) + 1;
        ret.cols = (mc_count_t) ((x_max - x_min) / x_step) + 1;
        ret.count = ret.rows * ret.cols;
        ret.points = mc_malloc(sizeof(struct vec3) * ret.count);
        ret.enabled_points = mc_malloc(sizeof(mc_bool_t) * ret.count);

        for (mc_ind_t r = 0; r < ret.rows; ++r) {
            for (mc_ind_t c = 0; c < ret.cols; ++c) {
                ret.points[r * ret.cols + c] =
                    (struct vec3){ (float) (x_min + c * x_step),
                                   (float) (y_min + r * y_step), 0 };
                ret.enabled_points[r * ret.cols + c] =
                    1; // can memset since it's a char, but i'd rather not
                       // assume that
            }
        }

        if (points_index.value.doub == 2) {
            /* guaranteed to succeed */
            struct vector_field const func = vector_field_nocopy_extract_type(
                executor, fields[7], VECTOR_FIELD_TYPE_FUNCTION
            );

            struct vector_field in_vector = vector_init(executor);
            struct vector *const in_vector_pointer = in_vector.value.pointer;
            for (int i = 0; i < 3; ++i) {
                struct vector_field zero_element = double_init(executor, 0);
                vector_plus(executor, in_vector, &zero_element);
            }

            for (mc_ind_t r = 0; r < ret.rows; ++r) {
                for (mc_ind_t c = 0; c < ret.cols; ++c) {
                    in_vector_pointer->fields[0] =
                        double_init(executor, x_min + c * x_step);
                    in_vector_pointer->fields[1] =
                        double_init(executor, y_min + r * y_step);
                    in_vector_pointer->fields[2] = double_init(executor, 0);
                    function_call(executor, func, 1, &in_vector);

                    if (!vector_field_extract_type(
                             executor, &executor->return_register,
                             VECTOR_FIELD_TYPE_DOUBLE
                        )
                             .vtable) {
                        VECTOR_FIELD_FREE(executor, in_vector);

                        mc_free(ret.points);
                        mc_free(ret.enabled_points);
                        ret = (struct vec3_plane_covering){ 0 };
                        ret.count = SIZE_MAX;
                        return ret;
                    }
                    ret.enabled_points[r * ret.cols + c] =
                        executor->return_register.value.doub != 0;
                    executor->return_register = VECTOR_FIELD_NULL;
                }
            }

            VECTOR_FIELD_FREE(executor, in_vector);
        }

        return ret;
    }
    else {
#pragma message(                                                               \
    "TODO a lot of mesh queries and geometry stuff still need to be done"      \
)
        /* domain */
        //        LIBMC_FULL_CAST_RETURN(mesh, 1, VECTOR_FIELD_TYPE_MESH, return
        //        ret); LIBMC_FULL_CAST_RETURN(resample_rate, 2,
        //        VECTOR_FIELD_TYPE_DOUBLE, return ret);
        //
        //        struct tetramesh *const mesh_p = mesh.value.pointer;
        //
        //        if (!tetramesh_is_planar2d(mesh_p)) {
        //            VECTOR_FIELD_ERROR(executor, "Expected 2D planar mesh!");
        //            executor->return_register = VECTOR_FIELD_NULL;
        //            return ret;
        //        }
        //
        //        struct vec3 norm

        return ret;
    }
}

void
vec3_plane_covering_free(struct vec3_plane_covering covering)
{
    mc_free(covering.enabled_points);
    mc_free(covering.points);
}

int
tetramesh_rank(struct tetramesh const *tetramesh)
{
    if (tetramesh->tri_count) {
        return 2;
    }
    else if (tetramesh->lin_count) {
        return 1;
    }
    else if (tetramesh->dot_count) {
        return 0;
    }
    else {
        return -1;
    }
}

void
tetramesh_downrank(
    struct timeline_execution_context *executor, struct tetramesh *tetramesh
)
{
    /* only keep it if it's a boundary, and then add a replacment node */
    mc_count_t out_count = 0;
    struct tetra_dot *out_dot = NULL;
    for (mc_ind_t i = 0; i < tetramesh->dot_count; ++i) {
        if (tetramesh->dots[i].inverse < 0 &&
            tetramesh->dots[i].antinorm > (int32_t) i) {
            struct tetra_dot curr = tetramesh->dots[i];
            struct tetra_dot inv = curr;
            struct tetra_dot curr_an = tetramesh->dots[curr.antinorm];
            struct tetra_dot inv_an = curr_an;

            curr.is_dominant_sibling = 1;
            curr.inverse = (int32_t) out_count + 1;
            curr.antinorm = (int32_t) out_count + 2;

            inv.is_dominant_sibling = 0;
            inv.inverse = (int32_t) out_count;
            inv.antinorm = (int32_t) out_count + 3;

            curr_an.is_dominant_sibling = 1;
            curr_an.inverse = (int32_t) out_count + 3;
            curr_an.antinorm = (int32_t) out_count;

            inv_an.is_dominant_sibling = 0;
            inv_an.inverse = (int32_t) out_count + 2;
            inv_an.antinorm = (int32_t) out_count + 1;

            MC_MEM_RESERVEN(out_dot, out_count, 4);
            out_dot[out_count++] = curr;
            out_dot[out_count++] = inv;
            out_dot[out_count++] = curr_an;
            out_dot[out_count++] = inv_an;
        }
    }
    mc_free(tetramesh->dots);
    tetramesh->dots = out_dot;
    tetramesh->dot_count = out_count;

    mc_count_t tracking_count = tetramesh->lin_count > tetramesh->tri_count
                                    ? tetramesh->lin_count
                                    : tetramesh->tri_count;
    int32_t *tracking_map = mc_malloc(sizeof(int32_t) * tracking_count);
    memset(tracking_map, -1, sizeof(int32_t) * tracking_count);

    out_count = 0;
    struct tetra_lin *out_lin = NULL;
    for (mc_ind_t i = 0; i < tetramesh->lin_count; ++i) {
        if (tetramesh->lins[i].inverse < 0 &&
            tetramesh->lins[i].antinorm > (int32_t) i) {
            struct tetra_lin curr = tetramesh->lins[i];
            struct tetra_lin inv = curr;
            struct tetra_lin curr_an = tetramesh->lins[curr.antinorm];
            struct tetra_lin inv_an = curr_an;

            tracking_map[i] = (int32_t) out_count;
            tracking_map[curr.antinorm] = (int32_t) out_count + 2;

            curr.is_dominant_sibling = 1;
            curr.inverse = (int32_t) out_count + 1;
            curr.antinorm = (int32_t) out_count + 2;

            inv.is_dominant_sibling = 0;
            inv.inverse = (int32_t) out_count;
            inv.antinorm = (int32_t) out_count + 3;
            struct tetra_lin_vertex swap = inv.a;
            inv.a = inv.b;
            inv.b = swap;

            curr_an.is_dominant_sibling = 1;
            curr_an.inverse = (int32_t) out_count + 3;
            curr_an.antinorm = (int32_t) out_count;

            inv_an.is_dominant_sibling = 0;
            inv_an.inverse = (int32_t) out_count + 2;
            inv_an.antinorm = (int32_t) out_count + 1;
            swap = inv_an.a;
            inv_an.a = inv_an.b;
            inv_an.b = swap;

            MC_MEM_RESERVEN(out_lin, out_count, 4);
            out_lin[out_count++] = curr;
            out_lin[out_count++] = inv;
            out_lin[out_count++] = curr_an;
            out_lin[out_count++] = inv_an;
        }
    }
    for (mc_ind_t i = 0; i < out_count; ++i) {
        if (out_lin[i].is_dominant_sibling) {
            /* untranslated addresses */
            int32_t n = out_lin[i].next, p = out_lin[i].prev;

            out_lin[i].next = tracking_map[n];
            out_lin[i].prev = tracking_map[p];

            struct tetra_lin const curr = out_lin[i];

            out_lin[curr.inverse].next = out_lin[curr.prev].inverse;
            out_lin[curr.inverse].prev = out_lin[curr.next].inverse;
        }
    }
    mc_free(tetramesh->lins);
    tetramesh->lins = out_lin;
    tetramesh->lin_count = out_count;

    memset(tracking_map, -1, sizeof(int32_t) * tracking_count);

    out_count = 0;
    struct tetra_tri *out_tri = NULL;
    for (mc_ind_t i = 0; i < tetramesh->tri_count; ++i) {
        if (tetramesh->tris[i].antinorm < 0) {
            struct tetra_tri curr = tetramesh->tris[i];
            curr.is_dominant_sibling = 1;
            curr.antinorm = (int32_t) out_count + 1;

            struct tetra_tri curr_an = curr;
            curr_an.is_dominant_sibling = 0;
            curr_an.antinorm = (int32_t) out_count;

            struct tetra_tri_vertex swap = curr_an.a;
            curr_an.a = curr_an.b;
            curr_an.b = swap;
            curr_an.a.norm = vec3_mul_scalar(-1, curr_an.a.norm);
            curr_an.b.norm = vec3_mul_scalar(-1, curr_an.b.norm);
            curr_an.c.norm = vec3_mul_scalar(-1, curr_an.c.norm);

            tracking_map[i] = (int32_t) out_count;

            MC_MEM_RESERVEN(out_tri, out_count, 2);
            out_tri[out_count++] = curr;
            out_tri[out_count++] = curr_an;
        }
    }

    for (mc_ind_t i = 0; i < out_count; ++i) {
        if (out_tri[i].is_dominant_sibling) {
            /* untranslated addresses */
            int32_t ab = out_tri[i].ab, bc = out_tri[i].bc, ca = out_tri[i].ca;

            /* it's not a boundary */
            while (tracking_map[ab] < 0) {
                ab = tetramesh->tris[tetramesh->tris[ab].antinorm].ab;
            }
            while (tracking_map[bc] < 0) {
                bc = tetramesh->tris[tetramesh->tris[bc].antinorm].ca;
            }
            while (tracking_map[ca] < 0) {
                ca = tetramesh->tris[tetramesh->tris[ca].antinorm].bc;
            }

            out_tri[i].ab = tracking_map[ab];
            out_tri[i].bc = tracking_map[bc];
            out_tri[i].ca = tracking_map[ca];

            struct tetra_tri const curr = out_tri[i];

            out_tri[curr.antinorm].ab = out_tri[curr.ab].antinorm;
            out_tri[curr.antinorm].bc = out_tri[curr.ca].antinorm;
            out_tri[curr.antinorm].ca = out_tri[curr.bc].antinorm;
        }
    }
    mc_free(tetramesh->tris);
    tetramesh->tris = out_tri;
    tetramesh->tri_count = out_count;

    mc_free(tracking_map);
}

// static void
// uprank_caps(void)
//{
/* pretty simple */

#pragma message("TODO: dot upranking not super useful anyways")
//
//    for (mc_ind_t i = 0; i < dot_count; ++i) {
//        if (old_dot[i].inverse >= 0) {
//            if (i < old_dot[i].inverse &&
//                i < old_dot[i].antinorm &&
//                i < old_dot[tetramesh->dots[i].inverse].antinorm) {
//                lin_translation[lin_count + tetramesh->lin_count] =
//                tetramesh->lin_count;
//
//                MC_MEM_RESERVE(tetramesh->lins, tetramesh->lin_count);
//                tetramesh->lins[tetramesh->lin_count] = (struct tetra_lin) {
//                    .a = {old_dot[i].pos, old_dot[i].col},
//                    .b = {old_dot[old_dot[i].inverse].pos,
//                    old_dot[old_dot[i].inverse].col}, .norm = old_dot[i].norm,
//                    .prev = -1 - old_dot[i].inverse, .next = -1 - i,
//                    .antinorm = (int32_t) tetramesh->lin_count + 2,
//                    .inverse = (int32_t) tetramesh->lin_count + 1,
//                    .is_dominant_sibling = old_dot[i].is_dominant_sibling
//                };
//                ++tetramesh->lin_count;
//
//                lin_translation[lin_count + tetramesh->lin_count] =
//                tetramesh->lin_count;
//
//                MC_MEM_RESERVE(tetramesh->lins, tetramesh->lin_count);
//                tetramesh->lins[tetramesh->lin_count] = struct tetra_lin) {
//                    .a = {old_dot[old_dot[i].inverse].pos,
//                    old_dot[old_dot[i].inverse].col}, .b = {old_dot[i].pos,
//                    old_dot[i].col}, .norm = old_dot[i].norm, .prev = -1 - i,
//                    .next = -1 - old_dot[i].inverse, .antinorm = (int32_t)
//                    tetramesh->lin_count + 2, .inverse = (int32_t)
//                    tetramesh->lin_count - 1, .is_dominant_sibling =
//                    old_dot[i].is_dominant_sibling
//                };
//
//                ++tetramesh->lin_count;
//
//
//                tetramesh->dots[i].inverse = 0;
//            }
//        }
//
//        MC_MEM_RESERVE(tetramesh->dots, tetramesh->dot_count);
//        tetramesh->dots[tetramesh->dot_count++] = old_dot[i];
//    }
//}

// some positive determinant operation transformation
// helps with consistent triangulations
#pragma message("TODO need a better fix long term")

static void
swap_lin(struct tetra_lin *a)
{
    struct tetra_lin_vertex const x = a->a;
    a->a = a->b;
    a->b = x;
}

static struct vec3
comp_norm(struct tetra_lin const *lin, int32_t r, mc_count_t cycle_len)
{
    /* heuristic approach, generally should (?) work */
    struct vec3 norm = { 0 };
    for (mc_ind_t d = 1; d * 2 < cycle_len; d *= 2) {
        int32_t j = r;
        int32_t k = j;
        for (mc_ind_t g = 0; g < d; ++g) {
            k = lin[k].next;
        }
        int32_t q = k;
        for (mc_ind_t g = 0; g < d; ++g) {
            k = lin[k].next;
        }

        do {
            norm = vec3_add(
                norm, vec3_cross(
                          vec3_sub(lin[k].a.pos, lin[j].a.pos),
                          vec3_sub(lin[q].a.pos, lin[j].a.pos)
                      )
            );

            j = lin[j].next;
            k = lin[j].next;
            q = lin[q].next;
        } while (j != r);
    }

    return norm;
}

static mc_status_t
uprank_loop(
    struct tetramesh *mesh, mc_count_t old_lin_count,
    struct tetra_lin const *old_lin
)
{
    /* libtess2 library call*/

    mc_status_t ret = 0;
    struct integer_map edge_map = integer_map_init();
    struct integer_map forw_map = integer_map_init();

    mc_bool_t *visited = mc_calloc(old_lin_count, sizeof(mc_bool_t));
    struct vec3 prev_norm = { 0, 0, 0 };

#define EDGE(a, b)                                                             \
    (uint64_t)(a) < (uint64_t) (b) ? ((uint64_t) (a) << 32) | (uint64_t) (b)   \
                                   : ((uint64_t) (b) << 32) | (uint64_t) (a)

    MC_MEM_RESERVEN(mesh->lins, mesh->lin_count, old_lin_count);
    memcpy(mesh->lins, old_lin, old_lin_count * sizeof(struct tetra_lin));
    mesh->lin_count = old_lin_count;

    mc_count_t contour_count = 0;
    float **vertices = NULL;
    mc_count_t *sub_counts = NULL;
    int32_t *roots = NULL;
    struct vec3 *norms = NULL;
    mc_bool_t has_different_norms = 0;

    for (mc_ind_t i = 0; i < old_lin_count; ++i) {
        if (!old_lin[i].is_dominant_sibling || visited[i] ||
            vec3_dot(prev_norm, old_lin[i].norm) < 0) {
            continue;
        }
        prev_norm = old_lin[i].norm;

        int32_t j = (int32_t) i;
        mc_count_t sub_count = 0;
        do {
            sub_count++;

            visited[j] = 1;
            visited[old_lin[j].antinorm] = 1;

            j = old_lin[j].next;
        } while (j != (int32_t) i);

        MC_MEM_RESERVE(vertices, contour_count);
        MC_MEM_RESERVE(sub_counts, contour_count);
        MC_MEM_RESERVE(roots, contour_count);
        MC_MEM_RESERVE(norms, contour_count);
        vertices[contour_count] = mc_malloc(sizeof(float) * sub_count * 3);
        sub_counts[contour_count] = sub_count;
        roots[contour_count] = (int32_t) i;
        norms[contour_count] = comp_norm(old_lin, j, sub_count);

        for (mc_ind_t k = 0; k < sub_count; ++k, j = old_lin[j].next) {
            struct vec3 const apply = old_lin[j].a.pos;
            vertices[contour_count][3 * k] = apply.x;
            vertices[contour_count][3 * k + 1] = apply.y;
            vertices[contour_count][3 * k + 2] = apply.z;
        }

        contour_count++;
    }

    /* norm checking is somewhat unreliable */
    has_different_norms = 1;

    /* offset each one ever so slightly to avoid triangulation issues */
    
    for (mc_ind_t q = 1; q < contour_count; ++q) {
        float const norm = 2e-3f;
        struct plane3 const plane = vec3_plane_basis(prev_norm);
        float const theta =
            2 * (float) M_PI * q * (contour_count - 1) / contour_count;
        float const c = (float) cos(theta) * norm;
        float const s = (float) sin(theta) * norm;
        struct vec3 const delta = vec3_add(
            vec3_mul_scalar(c, plane.a), vec3_mul_scalar(s, plane.b)
        );

        float const x = delta.x;
        float const y = delta.y;
        float const z = delta.z;

        for (mc_ind_t k = 0; k < sub_counts[q]; ++k) {
            vertices[q][3 * k] += x;
            vertices[q][3 * k + 1] += y;
            vertices[q][3 * k + 2] += z;
        }
    }
    
    for (mc_ind_t i = 0; i < contour_count; ++i) {
        if (sub_counts[i] == 0) {
            continue;
        }

        TESStesselator *const tesselator = tessNewTess(NULL);
        tessSetOption(tesselator, TESS_CONSTRAINED_DELAUNAY_TRIANGULATION, 1);

        integer_map_clear(&forw_map);
        integer_map_clear(&edge_map);

        mc_count_t count = 0;
        uint32_t *raw_index_to_lin = NULL;

        for (mc_ind_t q = i;
             q == i || (has_different_norms && q < contour_count); ++q) {
            int32_t j = roots[q];

            if (sub_counts[q] == 0) {
                continue;
            }

            do {
                MC_MEM_RESERVE(raw_index_to_lin, count);
                raw_index_to_lin[count++] = (uint32_t) j;

                uint64_t const a = (uint64_t) j;
                uint64_t const b = (uint64_t) old_lin[j].next;

                uint64_t const edge = EDGE(a, b);
                integer_map_set(
                    &forw_map, edge, (union ptr_int64){ .int_value = -1 - j }
                );

                j = old_lin[j].next;
            } while (j != roots[q]);

            tessAddContour(
                tesselator, 3, vertices[q], sizeof(float) * 3,
                (int) sub_counts[q]
            );
        }

        /* set this as last iteration */
        if (has_different_norms) {
            i = contour_count;
        }

        TESSreal const *out_vertices = NULL;
        TESSindex const *out_indices = NULL, *un_mapping = NULL;

        if (!tessTesselate(
                tesselator, TESS_WINDING_NONZERO, TESS_POLYGONS, 3, 3, NULL
            )) {
            /* likely degenerate */
            mc_free(raw_index_to_lin);
            tessDeleteTess(tesselator);
            ret = MC_STATUS_FAIL;
            mc_log_errorn_static("uprank_loop", "errno %d", ret);
            goto free;
        }

        mc_count_t out_indices_count =
            (mc_count_t) tessGetElementCount(tesselator);

        out_indices = tessGetElements(tesselator);
        out_vertices = tessGetVertices(tesselator);
        un_mapping = tessGetVertexIndices(tesselator);

        /* create edge map start list */
        for (mc_ind_t k = 0; k < out_indices_count; ++k) {
            TESSindex a = out_indices[3 * k], b = out_indices[3 * k + 1],
                      c = out_indices[3 * k + 2];

            uint64_t const aind = un_mapping[a] == TESS_UNDEF
                                      ? ~0U
                                      : raw_index_to_lin[un_mapping[a]];
            uint64_t const bind = un_mapping[b] == TESS_UNDEF
                                      ? ~0U
                                      : raw_index_to_lin[un_mapping[b]];
            uint64_t const cind = un_mapping[c] == TESS_UNDEF
                                      ? ~0U
                                      : raw_index_to_lin[un_mapping[c]];

            uint64_t const ab = EDGE(aind, bind);
            uint64_t const bc = EDGE(bind, cind);
            uint64_t const ca = EDGE(cind, aind);

            mc_bool_t const had_ab = integer_map_has(&forw_map, ab);
            mc_bool_t const had_bc = integer_map_has(&forw_map, bc);
            mc_bool_t const had_ca = integer_map_has(&forw_map, ca);

            if (had_ab) {
                integer_map_set(
                    &edge_map, EDGE(a, b), integer_map_get(&forw_map, ab)
                );
            }
            if (had_bc) {
                integer_map_set(
                    &edge_map, EDGE(b, c), integer_map_get(&forw_map, bc)
                );
            }
            if (had_ca) {
                integer_map_set(
                    &edge_map, EDGE(c, a), integer_map_get(&forw_map, ca)
                );
            }
        }

        struct vec4 const default_color = old_lin[0].a.col;

        // offset it perfectly
        mc_count_t const max = mesh->tri_count / 2;
        int32_t const maxi = (int32_t) mesh->tri_count / 2;
        for (mc_ind_t k = max; k < max + out_indices_count; ++k) {
            mc_ind_t const q = k - max;

            TESSindex const aind = out_indices[3 * q];
            TESSindex const bind = out_indices[3 * q + 1];
            TESSindex const cind = out_indices[3 * q + 2];

            struct vec3 const a = {
                out_vertices[3 * aind],
                out_vertices[3 * aind + 1],
                out_vertices[3 * aind + 2],
            };
            struct vec3 const b = {
                out_vertices[3 * bind],
                out_vertices[3 * bind + 1],
                out_vertices[3 * bind + 2],
            };
            struct vec3 const c = {
                out_vertices[3 * cind],
                out_vertices[3 * cind + 1],
                out_vertices[3 * cind + 2],
            };

            struct vec3 const norm = vec3_cross(vec3_sub(b, a), vec3_sub(c, a));
            struct vec3 const antinorm = vec3_mul_scalar(-1, norm);

            uint64_t ab = EDGE(aind, bind);
            uint64_t bc = EDGE(bind, cind);
            uint64_t ca = EDGE(cind, aind);

            mc_bool_t const had_ab = integer_map_has(&edge_map, ab);
            mc_bool_t const had_bc = integer_map_has(&edge_map, bc);
            mc_bool_t const had_ca = integer_map_has(&edge_map, ca);

            int32_t ab_n = (int32_t
            ) integer_map_set(&edge_map, ab,
                              (union ptr_int64){ .int_value = 2 * (int64_t) k, })
                               .int_value;
            int32_t bc_n = (int32_t
            ) integer_map_set(&edge_map, bc,
                              (union ptr_int64){ .int_value = 2 * (int64_t) k, })
                               .int_value;
            int32_t ca_n = (int32_t
            ) integer_map_set(&edge_map, ca,
                              (union ptr_int64){ .int_value = 2 * (int64_t) k, })
                               .int_value;

            int32_t ab_an, bc_an, ca_an;

            if (had_ab) {
                /* relink */
                if (ab_n >= 0) {
                    struct tetra_tri *const tri = &mesh->tris[ab_n];
                    struct tetra_tri *const atri = &mesh->tris[tri->antinorm];

                    for (int e = 0; e < 3; ++e) {
                        uint64_t const sub_edge = EDGE(
                            out_indices[3 * (ab_n / 2 - maxi) + e],
                            out_indices[3 * (ab_n / 2 - maxi) + (e + 1) % 3]
                        );
                        if (sub_edge == ab) {
                            //                        assert(tetramesh_tri_edge(tri,
                            //                        e) == INT32_MAX);
                            tetramesh_tri_set_edge(tri, e, (int32_t) (2 * k));
                            tetramesh_tri_set_edge(
                                atri, (3 - e) % 3, (int32_t) (2 * k + 1)
                            );
                            break;
                        }
                    }

                    ab_an = tri->antinorm;
                }
                else {
                    struct tetra_lin *const lin = &mesh->lins[-1 - ab_n];
                    struct tetra_lin *const alin = &mesh->lins[lin->antinorm];
                    if (vec3_dot(lin->norm, norm) < 0) {
                        swap_lin(lin);
                        swap_lin(alin);
                    }
                    lin->a.pos = a;
                    lin->b.pos = b;
                    alin->a.pos = b;
                    alin->b.pos = a;
                    lin->inverse = -1 - (int32_t) (2 * k);
                    alin->inverse = -1 - (int32_t) (2 * k + 1);

                    ab_an = -1 - lin->antinorm;
                }
            }
            else {
                ab_n = INT32_MAX;
                ab_an = INT32_MAX;
            }

            if (had_bc) {
                /* relink */
                if (bc_n >= 0) {
                    struct tetra_tri *const tri = &mesh->tris[bc_n];
                    struct tetra_tri *const atri = &mesh->tris[tri->antinorm];

                    for (int e = 0; e < 3; ++e) {
                        uint64_t const sub_edge = EDGE(
                            out_indices[3 * (bc_n / 2 - maxi) + e],
                            out_indices[3 * (bc_n / 2 - maxi) + (e + 1) % 3]
                        );
                        if (sub_edge == bc) {
                            //                        assert(tetramesh_tri_edge(tri,
                            //                        e) == INT32_MAX);
                            tetramesh_tri_set_edge(tri, e, (int32_t) (2 * k));
                            tetramesh_tri_set_edge(
                                atri, (3 - e) % 3, (int32_t) (2 * k + 1)
                            );
                            break;
                        }
                    }

                    ca_an = tri->antinorm;
                }
                else {
                    struct tetra_lin *const lin = &mesh->lins[-1 - bc_n];
                    struct tetra_lin *const alin = &mesh->lins[lin->antinorm];
                    if (vec3_dot(lin->norm, norm) < 0) {
                        swap_lin(lin);
                        swap_lin(alin);
                    }
                    lin->a.pos = b;
                    lin->b.pos = c;
                    alin->a.pos = c;
                    alin->b.pos = b;
                    lin->inverse = -1 - (int32_t) (2 * k);
                    alin->inverse = -1 - (int32_t) (2 * k + 1);

                    ca_an = -1 - lin->antinorm;
                }
            }
            else {
                bc_n = INT32_MAX;  /* temporary used for remapping */
                ca_an = INT32_MAX; /* garbage */
            }

            if (had_ca) {
                /* relink */
                if (ca_n >= 0) {
                    struct tetra_tri *const tri = &mesh->tris[ca_n];
                    struct tetra_tri *const atri = &mesh->tris[tri->antinorm];

                    for (int e = 0; e < 3; ++e) {
                        uint64_t const sub_edge = EDGE(
                            out_indices[3 * (ca_n / 2 - maxi) + e],
                            out_indices[3 * (ca_n / 2 - maxi) + (e + 1) % 3]
                        );
                        if (sub_edge == ca) {
                            //                        assert(tetramesh_tri_edge(tri,
                            //                        e) == INT32_MAX);
                            tetramesh_tri_set_edge(tri, e, (int32_t) (2 * k));
                            tetramesh_tri_set_edge(
                                atri, (3 - e) % 3, (int32_t) (2 * k + 1)
                            );
                            break;
                        }
                    }

                    bc_an = tri->antinorm;
                }
                else {
                    struct tetra_lin *const lin = &mesh->lins[-1 - ca_n];
                    struct tetra_lin *const alin = &mesh->lins[lin->antinorm];
                    if (vec3_dot(lin->norm, norm) < 0) {
                        swap_lin(lin);
                        swap_lin(alin);
                    }
                    lin->a.pos = c;
                    lin->b.pos = a;
                    alin->a.pos = a;
                    alin->b.pos = c;
                    lin->inverse = -1 - (int32_t) (2 * k);
                    alin->inverse = -1 - (int32_t) (2 * k + 1);

                    bc_an = -1 - lin->antinorm;
                }
            }
            else {
                ca_n = INT32_MAX;
                bc_an = INT32_MAX;
            }

            MC_MEM_RESERVEN(mesh->tris, mesh->tri_count, 2);
            mesh->tris[mesh->tri_count] = (struct tetra_tri){
                .a = { .pos = a,
                       .norm = norm,
                       .uv = { 0, 0 },
                       .col = default_color },
                .b = { .pos = b,
                       .norm = norm,
                       .uv = { 0, 0 },
                       .col = default_color },
                .c = { .pos = c,
                       .norm = norm,
                       .uv = { 0, 0 },
                       .col = default_color },

                .ab = ab_n,
                .bc = bc_n,
                .ca = ca_n,

                .antinorm = (int32_t) (2 * k + 1),
                .is_dominant_sibling = 1,
            };

            mesh->tris[mesh->tri_count + 1] = (struct tetra_tri){
                .a = { .pos = b,
                       .norm = antinorm,
                       .uv = { 0, 0 },
                       .col = default_color },
                .b = { .pos = a,
                       .norm = antinorm,
                       .uv = { 0, 0 },
                       .col = default_color },
                .c = { .pos = c,
                       .norm = antinorm,
                       .uv = { 0, 0 },
                       .col = default_color },

                .ab = ab_an,
                .bc = bc_an,
                .ca = ca_an,

                .antinorm = (int32_t) (2 * k),
                .is_dominant_sibling = 0,
            };

            mesh->tri_count += 2;
        }

        tessDeleteTess(tesselator);
        mc_free(raw_index_to_lin);
    }

    for (mc_ind_t i = 0; i < old_lin_count; ++i) {
        if (mesh->lins[i].inverse < 0 &&
            tetramesh_tri_edge_for(
                &mesh->tris[-1 - mesh->lins[i].inverse], -1 - (int32_t) i
            ) == -1) {
            mesh->lins[i].inverse = old_lin[i].inverse;
        }
    }

    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        for (int e = 0; e < 3; ++e) {
            int32_t const nbr = tetramesh_tri_edge(&mesh->tris[i], e);
            if (0 <= nbr && nbr < (int32_t) mesh->tri_count &&
                tetramesh_tri_edge_for(&mesh->tris[nbr], (int32_t) i) == -1) {
                tetramesh_tri_set_edge(&mesh->tris[i], e, INT32_MAX);
            }
        }
    }

    /* add in boundaries to anyone that doesn't currently have a boundary */
    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        if (mesh->tris[i].is_dominant_sibling) {
            continue;
        }

        for (int e = 0; e < 3; ++e) {
            if (tetramesh_tri_edge(&mesh->tris[i], e) == INT32_MAX) {
                struct tetra_tri_vertex buff[2];
                tetramesh_tri_read_edge(&mesh->tris[i], e, buff);
                struct tetra_lin_vertex a = { buff[1].pos, buff[1].col };
                struct tetra_lin_vertex b = { buff[0].pos, buff[0].col };

                struct tetra_lin main = {
                    .a = a,
                    .b = b,
                    .norm = buff[0].norm,
                    .prev = -1,
                    .next = -1,
                    .inverse = -1 - (int32_t) i,
                    .antinorm = (int32_t) mesh->lin_count + 1,
                    .is_dominant_sibling = 1,
                };
                struct tetra_lin anti = {
                    .a = b,
                    .b = a,
                    .norm = vec3_mul_scalar(-1, buff[0].norm),
                    .prev = -1,
                    .next = -1,
                    .inverse = -1 - (int32_t) mesh->tris[i].antinorm,
                    .antinorm = (int32_t) mesh->lin_count,
                    .is_dominant_sibling = 1,
                };

                tetramesh_tri_set_edge(
                    &mesh->tris[i], e, -1 - (int32_t) mesh->lin_count
                );
                tetramesh_tri_set_edge(
                    &mesh->tris[mesh->tris[i].antinorm], (3 - e) % 3,
                    -2 - (int32_t) mesh->lin_count
                );

                MC_MEM_RESERVE(mesh->lins, mesh->lin_count);
                mesh->lins[mesh->lin_count++] = main;
                MC_MEM_RESERVE(mesh->lins, mesh->lin_count);
                mesh->lins[mesh->lin_count++] = anti;
            }
        }
    }

    /* fill in prev and next appropriately */
    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        if (mesh->lins[i].inverse < 0) {
            int32_t prev = -1 - (int32_t) i;
            int32_t tri = -1 - mesh->lins[i].inverse;
            while (tri >= 0) {
                int const e = tetramesh_tri_edge_for(&mesh->tris[tri], prev);
                assert(e != -1);
                prev = tri;
                tri = tetramesh_tri_edge(&mesh->tris[tri], (e + 1) % 3);
            }
            mesh->lins[i].next = -1 - tri;
            mesh->lins[-1 - tri].prev = (int32_t) i;
            //            assert(vec3_equals(mesh->lins[i].b.pos, mesh->lins[-1
            //            - tri].a.pos));
        }
    }

    /* for all contigous set of contours that are not visited */
    /* if closed, add inversese */
    /* otherwise, drop them */
    /* not perfect by any means, but does a decent job */
    visited = mc_reallocf(visited, mesh->lin_count * sizeof(mc_bool_t));
    memset(visited, 0, mesh->lin_count * sizeof(mc_bool_t));
    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        /* tentatively delete */
        if (!mesh->lins[i].is_dominant_sibling) {
            visited[i] = 2;
        }

        if (mesh->lins[i].inverse < 0) {
            mesh->lins[i].norm = mesh->tris[-1 - mesh->lins[i].inverse].a.norm;
        }
    }

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        /* unmatched, unvisited contour */
        if (!visited[i] && mesh->lins[i].inverse >= 0 &&
            mesh->lins[i].is_dominant_sibling) {
            int32_t j = (int32_t) i;
            while (mesh->lins[j].prev >= 0 &&
                   mesh->lins[j].prev != (int32_t) i &&
                   mesh->lins[mesh->lins[j].prev].inverse >= 0) {
                visited[j] = 1;
                j = mesh->lins[j].prev;
            }

            int32_t k = (int32_t) i;
            while (mesh->lins[k].next >= 0 &&
                   mesh->lins[mesh->lins[k].next].inverse >= 0) {
                k = mesh->lins[k].next;
                visited[k] = 1;
                if (k == (int32_t) i) {
                    break;
                }
            }

            if (vec3_equals(mesh->lins[k].b.pos, mesh->lins[j].a.pos)) {
                mesh->lins[k].next = j;
                mesh->lins[mesh->lins[k].inverse].prev = mesh->lins[j].inverse;
                mesh->lins[mesh->lins[k].antinorm].prev =
                    mesh->lins[j].antinorm;
                mesh->lins[mesh->lins[mesh->lins[k].antinorm].inverse].next =
                    mesh->lins[mesh->lins[j].antinorm].inverse;
                mesh->lins[j].prev = k;
                mesh->lins[mesh->lins[j].inverse].next = mesh->lins[k].inverse;
                mesh->lins[mesh->lins[j].antinorm].prev =
                    mesh->lins[k].antinorm;
                mesh->lins[mesh->lins[mesh->lins[j].antinorm].inverse].next =
                    mesh->lins[mesh->lins[k].antinorm].inverse;
                /* dont delete */
                for (int32_t q = j;; q = mesh->lins[q].next) {
                    //                    visited[q] = 2;
                    visited[q] = 1;
                    visited[mesh->lins[q].antinorm] = 1;
                    visited[mesh->lins[q].inverse] = 1;
                    visited[mesh->lins[mesh->lins[q].antinorm].inverse] = 1;
                    if (q == k) {
                        break;
                    }
                }
            }
            else {
                /* mark as deleted, necessarily not a loop */
                for (int32_t q = j;; q = mesh->lins[q].next) {
                    visited[q] = 2;
                    visited[mesh->lins[q].antinorm] = 2;
                    visited[mesh->lins[q].inverse] = 2;
                    visited[mesh->lins[mesh->lins[q].antinorm].inverse] = 2;
                    if (q == k) {
                        break;
                    }
                }
            }
        }
    }

    /* translate lins */
    int32_t encountered = 0;
    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        if (mesh->lins[i].inverse < 0 &&
            (i >= old_lin_count || visited[i] != 2)) {
            /* keep */
            if (mesh->lins[i].inverse >= 0) {
                mesh->lins[mesh->lins[i].inverse].inverse = encountered;
            }
            else {
                int const e = tetramesh_tri_edge_for(
                    &mesh->tris[-1 - mesh->lins[i].inverse], -1 - (int32_t) i
                );
                tetramesh_tri_set_edge(
                    &mesh->tris[-1 - mesh->lins[i].inverse], e, -1 - encountered
                );
            }

            mesh->lins[mesh->lins[i].next].prev = encountered;
            mesh->lins[mesh->lins[i].prev].next = encountered;
            mesh->lins[mesh->lins[i].antinorm].antinorm = encountered;

            mesh->lins[encountered++] = mesh->lins[i];
        }
    }
    mesh->lin_count = (mc_count_t) encountered;

#undef EDGE
free:
    mc_free(visited);

    for (mc_ind_t k = 0; k < contour_count; ++k) {
        mc_free(vertices[k]);
    }
    mc_free(vertices);
    mc_free(roots);
    mc_free(norms);
    mc_free(sub_counts);

    integer_map_free(edge_map);
    integer_map_free(forw_map);

    return ret;
}

// static void
// uprank_gaussian(void){
#pragma message("TODO")
//}

/* upranks all */
mc_status_t
tetramesh_uprank(struct tetramesh *tetramesh, mc_bool_t allow_double)
{
    if (tetramesh->tri_count || tetramesh->dot_count || !tetramesh->lin_count) {
        return MC_STATUS_FAIL;
    }

    /* idea: */
    // uprank as we are doing right now
    // need to handle: degen points, lines not being mentioned
    // add boundaries manually, and see which lines are not mentioned
    // if a continous set is not mentioned, then see if it's a loop
    // if so, then close it
    // otherwise, ignore them

    mc_status_t ret = MC_STATUS_SUCCESS;

    mc_count_t const lin_count = tetramesh->lin_count;
    struct tetra_lin *const old_lin = tetramesh->lins;

    tetramesh->lins = NULL;
    tetramesh->lin_count = 0;

    if ((ret = uprank_loop(tetramesh, lin_count, old_lin))) {
        goto free;
    }

    /* terrible fix */
    if (allow_double || tetramesh->tri_count == 0) {
        int32_t const delta = (int32_t) tetramesh->lin_count;
        tetramesh->lin_count += lin_count;
        tetramesh->lins = mc_reallocf(
            tetramesh->lins, tetramesh->lin_count * sizeof(struct tetra_lin)
        );
        memcpy(
            tetramesh->lins + delta, old_lin,
            lin_count * sizeof(struct tetra_lin)
        );
        for (mc_ind_t i = 0; i < lin_count; ++i) {
            tetramesh->lins[delta + (int32_t) i].next += delta;
            tetramesh->lins[delta + (int32_t) i].prev += delta;
            tetramesh->lins[delta + (int32_t) i].inverse += delta;
            tetramesh->lins[delta + (int32_t) i].antinorm += delta;
        }
    }

    tetramesh_assert_invariants(tetramesh);

free:
    mc_free(old_lin);

    return ret;
}

#pragma message(                                                               \
    "ORGANIZATION: maybe make a dedicated stack data structure in future"      \
)
static inline int *
traversal_push(int *stack, mc_count_t *count, mc_count_t *capacity, int data)
{
    if (*count == *capacity) {
        *capacity = MC_MEM_NEXT_CAPACITY(*count);
        stack = mc_reallocf(stack, sizeof(int) * *capacity);
    }

    stack[(*count)++] = data;

    return stack;
}

static inline int
traversal_pop(int *stack, mc_count_t *count)
{
    return stack[--*count];
}

/* stack dfs since timeline_thread stack is relatively small */
static int *
tri_dfs(
    struct tetramesh const *mesh, mc_ind_t pos, int color, int *tri_visited,
    int *lin_visited, int *traversal, mc_count_t *traversal_capacity,
    mc_count_t *traversal_count
)
{
    traversal = traversal_push(
        traversal, traversal_count, traversal_capacity, (int) pos
    );

    while (*traversal_count) {
        int const pop = traversal_pop(traversal, traversal_count);
        tri_visited[pop] = color;

        struct tetra_tri const tri = mesh->tris[pop];

        if (tri.ab >= 0) {
            if (!tri_visited[tri.ab]) {
                traversal = traversal_push(
                    traversal, traversal_count, traversal_capacity, tri.ab
                );
            }
        }
        else {
            lin_visited[-1 - tri.ab] = color;
        }

        if (tri.bc >= 0) {
            if (!tri_visited[tri.bc]) {
                traversal = traversal_push(
                    traversal, traversal_count, traversal_capacity, tri.bc
                );
            }
        }
        else {
            lin_visited[-1 - tri.bc] = color;
        }

        if (tri.ca >= 0) {
            if (!tri_visited[tri.ca]) {
                traversal = traversal_push(
                    traversal, traversal_count, traversal_capacity, tri.ca
                );
            }
        }
        else {
            lin_visited[-1 - tri.ca] = color;
        }

        /* necessarily greater than or equal to zero since otherwise we would've
         * been searched already*/
        if (!tri_visited[tri.antinorm]) {
            traversal = traversal_push(
                traversal, traversal_count, traversal_capacity, tri.antinorm
            );
        }
    }

    return traversal;
}

#pragma message(                                                                               \
    "TODO does not cover weird contours (trees are fine, cycles are fine but not tree cycles)" \
)
static void
lin_dfs(
    struct tetramesh const *mesh, mc_ind_t pos, int color, int *lin_visited,
    int *dot_visited
)
{
    int32_t const org = (int32_t) pos;
    int32_t it = org;

    do {
        struct tetra_lin const lin = mesh->lins[it];
        lin_visited[it] = color;
        lin_visited[lin.antinorm] = color;

        if (lin.inverse >= 0) {
            lin_visited[lin.inverse] = color;
            lin_visited[mesh->lins[lin.inverse].antinorm] = color;
        }

        if (lin.prev >= 0) {
            it = lin.prev;
        }
        else {
            dot_visited[-1 - lin.prev] = color;
            dot_visited[mesh->dots[-1 - lin.prev].antinorm] = color;
            it = lin.inverse;
        }
    } while (it != org);
}

/* true dfs since maximum depth of 4 */
static void
dot_dfs(struct tetramesh const *mesh, mc_ind_t pos, int color, int *visited)
{
    struct tetra_dot const dot = mesh->dots[pos];

    visited[pos] = color;
    visited[dot.antinorm] = color;

    if (dot.inverse >= 0) {
        visited[dot.inverse] = color;
        visited[mesh->dots[dot.inverse].antinorm] = color;
    }
}

static struct vector_field *
tetramesh_contours(
    struct timeline_execution_context *executor,
    struct tetramesh const *tetramesh, struct vector_field *dump,
    mc_count_t *count
)
{
#pragma message(                                                                     \
    "OPTIMIZATION: reuse same buffer instead of freeing and mc_callocing every time" \
)
    int *visited_tri = tetramesh->tri_count
                           ? mc_calloc(tetramesh->tri_count, sizeof(int))
                           : NULL;
    int *visited_lin = tetramesh->lin_count
                           ? mc_calloc(tetramesh->lin_count, sizeof(int))
                           : NULL;
    int *visited_dot = tetramesh->dot_count
                           ? mc_calloc(tetramesh->dot_count, sizeof(int))
                           : NULL;

    int32_t *pos_tri = tetramesh->tri_count
                           ? mc_calloc(tetramesh->tri_count, sizeof(int32_t))
                           : NULL;
    int32_t *pos_lin = tetramesh->lin_count
                           ? mc_calloc(tetramesh->lin_count, sizeof(int32_t))
                           : NULL;
    int32_t *pos_dot = tetramesh->dot_count
                           ? mc_calloc(tetramesh->dot_count, sizeof(int32_t))
                           : NULL;

    mc_count_t traversal_count = 0, traversal_capacity = 0;
    int *traversal_stack = NULL;

    int color = 0;
    for (mc_ind_t i = 0; i < tetramesh->tri_count; ++i) {
        if (visited_tri[i]) {
            continue;
        }
        traversal_stack = tri_dfs(
            tetramesh, i, ++color, visited_tri, visited_lin, traversal_stack,
            &traversal_count, &traversal_capacity
        );
    }

    for (mc_ind_t i = 0; i < tetramesh->lin_count; ++i) {
        if (visited_lin[i]) {
            continue;
        }
        lin_dfs(tetramesh, i, ++color, visited_lin, visited_dot);
    }

    for (mc_ind_t i = 0; i < tetramesh->dot_count; ++i) {
        if (visited_dot[i]) {
            continue;
        }
        dot_dfs(tetramesh, i, ++color, visited_dot);
    }

    for (mc_ind_t i = 0; i < (mc_ind_t) color; ++i) {
        struct vector_field ret = tetramesh_init(executor);
        struct tetramesh *const mesh = ret.value.pointer;
        mesh->uniform = tetramesh->uniform;
        mesh->texture_handle = tetramesh->texture_handle;
        mesh->modded = mesh->dirty_hash_cache = 1;
        mesh->tag_count = tetramesh->tag_count;
        mesh->tags = mc_malloc(sizeof(mc_tag_t) * mesh->tag_count);
        for (mc_ind_t j = 0; j < mesh->tag_count; ++j) {
            mesh->tags[j] = tetramesh->tags[j];
        }

        tetramesh_assert_invariants(mesh);
        MC_MEM_RESERVE(dump, *count + i);
        dump[*count + i] = ret;
    }

    for (mc_ind_t i = 0; i < tetramesh->tri_count; ++i) {
        struct tetramesh *const curr =
            dump[*count + (mc_count_t) visited_tri[i] - 1].value.pointer;
        pos_tri[i] = (int32_t) curr->tri_count;

        MC_MEM_RESERVE(curr->tris, curr->tri_count);
        curr->tris[curr->tri_count++] = tetramesh->tris[i];
    }

    for (mc_ind_t i = 0; i < tetramesh->lin_count; ++i) {
        struct tetramesh *const curr =
            dump[*count + (mc_count_t) visited_lin[i] - 1].value.pointer;
        pos_lin[i] = (int32_t) curr->lin_count;

        MC_MEM_RESERVE(curr->lins, curr->lin_count);
        curr->lins[curr->lin_count++] = tetramesh->lins[i];
    }

    for (mc_ind_t i = 0; i < tetramesh->dot_count; ++i) {
        struct tetramesh *const curr =
            dump[*count + (mc_count_t) visited_dot[i] - 1].value.pointer;
        pos_dot[i] = (int32_t) curr->dot_count;

        MC_MEM_RESERVE(curr->dots, curr->dot_count);
        curr->dots[curr->dot_count++] = tetramesh->dots[i];
    }

    for (mc_ind_t i = 0; i < tetramesh->tri_count; ++i) {
        int32_t const ab = tetramesh->tris[i].ab;
        int32_t const bc = tetramesh->tris[i].bc;
        int32_t const ca = tetramesh->tris[i].ca;
        int32_t const an = tetramesh->tris[i].antinorm;

        struct tetramesh *const curr =
            dump[*count + (mc_count_t) visited_tri[i] - 1].value.pointer;

        curr->tris[pos_tri[i]].ab =
            ab >= 0 ? pos_tri[ab] : -1 - pos_lin[-1 - ab];
        curr->tris[pos_tri[i]].bc =
            bc >= 0 ? pos_tri[bc] : -1 - pos_lin[-1 - bc];
        curr->tris[pos_tri[i]].ca =
            ca >= 0 ? pos_tri[ca] : -1 - pos_lin[-1 - ca];

        curr->tris[pos_tri[i]].antinorm = pos_tri[an];
    }

    for (mc_ind_t i = 0; i < tetramesh->lin_count; ++i) {
        int32_t const prev = tetramesh->lins[i].prev;
        int32_t const next = tetramesh->lins[i].next;
        int32_t const twin = tetramesh->lins[i].inverse;
        int32_t const anti = tetramesh->lins[i].antinorm;

        struct tetramesh *const curr =
            dump[*count + (mc_count_t) visited_lin[i] - 1].value.pointer;

        curr->lins[pos_lin[i]].prev =
            prev >= 0 ? pos_lin[prev] : -1 - pos_dot[-1 - prev];
        curr->lins[pos_lin[i]].next =
            next >= 0 ? pos_lin[next] : -1 - pos_dot[-1 - next];

        curr->lins[pos_lin[i]].inverse =
            twin >= 0 ? pos_lin[twin] : -1 - pos_tri[-1 - twin];
        curr->lins[pos_lin[i]].antinorm =
            pos_lin[anti]; /* antinorm for lines is always a line */
    }

    for (mc_ind_t i = 0; i < tetramesh->dot_count; ++i) {
        int32_t const twin = tetramesh->dots[i].inverse;
        int32_t const anti = tetramesh->dots[i].antinorm;

        struct tetramesh *const curr =
            dump[*count + (mc_count_t) visited_dot[i] - 1].value.pointer;

        curr->dots[pos_dot[i]].inverse =
            twin >= 0 ? pos_dot[twin] : -1 - pos_lin[-1 - twin];
        curr->dots[pos_dot[i]].antinorm = pos_dot[anti]; /* always a dot */
    }

    *count += (mc_count_t) color;

    mc_free(traversal_stack);

    mc_free(visited_tri);
    mc_free(visited_lin);
    mc_free(visited_dot);

    mc_free(pos_tri);
    mc_free(pos_lin);
    mc_free(pos_dot);

    return dump;
}

mc_count_t
tetramesh_contour_count(
    struct timeline_execution_context *executor,
    struct tetramesh const *tetramesh
)
{
    /* might not want to have a useless alloc? */
    struct vector_field *dump = NULL;
    mc_count_t count = 0;

    dump = tetramesh_contours(executor, tetramesh, dump, &count);

    for (mc_ind_t i = 0; i < count; ++i) {
        VECTOR_FIELD_FREE(executor, dump[i]);
    }
    mc_free(dump);

    return count;
}

static struct vector_field *
contour_separate_dfs(
    struct timeline_execution_context *executor, struct vector_field const curr,
    struct vector_field *dump, mc_count_t *count
)
{
    struct vector_field const extrude = vector_field_nocopy_extract_type(
        executor, curr, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_MESH
    );

    if (!extrude.vtable) {
        for (mc_ind_t i = 0; i < *count; ++i) {
            VECTOR_FIELD_FREE(executor, dump[i]);
        }
        *count = SIZE_MAX;
        mc_free(dump);
        return NULL;
    }
    else if (extrude.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vector = extrude.value.pointer;
        for (mc_ind_t i = 0; i < vector->field_count; ++i) {
            dump =
                contour_separate_dfs(executor, vector->fields[i], dump, count);
            if (*count == SIZE_MAX) {
                return dump;
            }
        }

        return dump;
    }
    else {
        struct tetramesh *const mesh = extrude.value.pointer;
        return tetramesh_contours(executor, mesh, dump, count);
    }
}

mc_status_t
mesh_contour_separate(
    struct timeline_execution_context *executor, struct vector_field *value
)
{
    struct vector_field *dump = NULL;
    mc_count_t count = 0;

    dump = contour_separate_dfs(executor, *value, dump, &count);

    if (count == SIZE_MAX) {
        return MC_STATUS_FAIL;
    }

    struct vector_field ret = vector_init(executor);
    struct vector *const vector = ret.value.pointer;
    vector->fields = dump;
    vector->field_count = count;
    executor->byte_alloc += count;

    VECTOR_FIELD_FREE(executor, *value);
    *value = ret;

    return MC_STATUS_SUCCESS;
}

static void
_change_edge(struct tetramesh *mesh, int32_t curr, int edge, int32_t to)
{
    int32_t const nbr = tetramesh_tri_edge(&mesh->tris[curr], edge);
    if (nbr < 0) {
        mesh->lins[-1 - nbr].inverse = -1 - to;
    }
    else {
        int const out = tetramesh_tri_edge_for(&mesh->tris[nbr], curr);
        tetramesh_tri_set_edge(&mesh->tris[nbr], out, to);
    }
}

void
tetramesh_tesselate(struct tetramesh *mesh, mc_count_t const target_tri_count)
{

    mc_count_t mem = mc_memory_upsize(mesh->tri_count);
    mesh->tris = _mc_reallocf(mesh->tris, mem * sizeof(struct tetra_tri));

    size_t ind = 0;
    while (mesh->tri_count + 4 <= target_tri_count) {
        while (!mesh->tris[ind].is_dominant_sibling) {
            if (++ind == mesh->tri_count) {
                ind = 0;
            }
        }

        struct tetra_tri const curr = mesh->tris[ind];
        struct tetra_tri const anti = mesh->tris[curr.antinorm];
        struct vec3 const pivot = {
            (curr.a.pos.x + curr.b.pos.x + curr.c.pos.x) / 3,
            (curr.a.pos.y + curr.b.pos.y + curr.c.pos.y) / 3,
            (curr.a.pos.z + curr.b.pos.z + curr.c.pos.z) / 3,
        };

        float const total = triangle_area(curr.a.pos, curr.b.pos, curr.c.pos);
        float const a = triangle_area(curr.a.pos, curr.b.pos, pivot);
        float const b = triangle_area(curr.b.pos, curr.c.pos, pivot);
        float a_p, b_p, c_p;

        if (total < GEOMETRIC_EPSILON) {
            a_p = b_p = c_p = 1.0f / 3;
        }
        else {
            c_p = a / total;
            a_p = b / total;
            b_p = 1 - a_p - c_p;
        }

        struct tetra_tri_vertex const forward = {
            .pos = pivot,
            .norm = curr.c.norm,
            .uv = {
                curr.a.uv.x * a_p + curr.b.uv.x * b_p + curr.c.uv.x * c_p,
                curr.a.uv.y * a_p + curr.b.uv.y * b_p + curr.c.uv.y * c_p,
            },
            .col = {
                curr.a.col.x * a_p + curr.b.col.x * b_p + curr.c.col.x * c_p,
                curr.a.col.y * a_p + curr.b.col.y * b_p + curr.c.col.y * c_p,
                curr.a.col.z * a_p + curr.b.col.z * b_p + curr.c.col.z * c_p,
                curr.a.col.w * a_p + curr.b.col.w * b_p + curr.c.col.w * c_p,
            }
        };
        struct tetra_tri_vertex const reverse = {
            .pos = pivot,
            .norm = anti.c.norm,
            .uv = {
                anti.a.uv.x * b_p + anti.b.uv.x * a_p + anti.c.uv.x * c_p,
                anti.a.uv.y * b_p + anti.b.uv.y * a_p + anti.c.uv.y * c_p,
            },
            .col = {
                anti.a.col.x * b_p + anti.b.col.x * a_p + anti.c.col.x * c_p,
                anti.a.col.y * b_p + anti.b.col.y * a_p + anti.c.col.y * c_p,
                anti.a.col.z * b_p + anti.b.col.z * a_p + anti.c.col.z * c_p,
                anti.a.col.w * b_p + anti.b.col.w * a_p + anti.c.col.w * c_p,
            }
        };

        // move first two
        _change_edge(mesh, (int32_t) ind, 1, (int32_t) mesh->tri_count);
        _change_edge(mesh, (int32_t) ind, 2, (int32_t) mesh->tri_count + 1);
        _change_edge(mesh, curr.antinorm, 1, (int32_t) mesh->tri_count + 3);
        _change_edge(mesh, curr.antinorm, 2, (int32_t) mesh->tri_count + 2);

        mesh->tris[ind].c = forward;
        mesh->tris[ind].bc = (int32_t) mesh->tri_count;
        mesh->tris[ind].ca = (int32_t) mesh->tri_count + 1;
        mesh->tris[curr.antinorm].c = reverse;
        mesh->tris[curr.antinorm].bc = (int32_t) mesh->tri_count + 3;
        mesh->tris[curr.antinorm].ca = (int32_t) mesh->tri_count + 2;

        MC_MEM_RESERVEN(mesh->tris, mesh->tri_count, 4);

        mesh->tris[mesh->tri_count] = (struct tetra_tri){
            .a = curr.b,
            .b = curr.c,
            .c = forward,
            .ab = curr.bc,
            .bc = (int32_t) mesh->tri_count + 1,
            .ca = (int32_t) ind,
            .antinorm = (int32_t) mesh->tri_count + 2,
            .is_dominant_sibling = 1,
        };
        mesh->tris[mesh->tri_count + 1] = (struct tetra_tri){
            .a = curr.a,
            .b = forward,
            .c = curr.c,
            .ab = (int32_t) ind,
            .bc = (int32_t) mesh->tri_count,
            .ca = curr.ca,
            .antinorm = (int32_t) mesh->tri_count + 3,
            .is_dominant_sibling = 1,
        };
        mesh->tris[mesh->tri_count + 2] = (struct tetra_tri){
            .a = anti.c,
            .b = anti.a,
            .c = reverse,
            .ab = anti.ca,
            .bc = curr.antinorm,
            .ca = (int32_t) mesh->tri_count + 3,
            .antinorm = (int32_t) mesh->tri_count + 1,
            .is_dominant_sibling = 0,
        };
        mesh->tris[mesh->tri_count + 3] = (struct tetra_tri){
            .a = reverse,
            .b = anti.b,
            .c = anti.c,
            .ab = curr.antinorm,
            .bc = anti.bc,
            .ca = (int32_t) mesh->tri_count + 2,
            .antinorm = (int32_t) mesh->tri_count + 2,
            .is_dominant_sibling = 0,
        };

        mesh->tri_count += 4;

        if (++ind == mesh->tri_count) {
            ind = 0;
        }
    }
}

struct vec3
tetramesh_com(struct tetramesh const *mesh)
{
    mc_count_t count = 0;
    struct vec3 sum = { 0 };

    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        sum.x += (mesh->tris[i].a.pos.x + mesh->tris[i].b.pos.x +
                  mesh->tris[i].c.pos.x) /
                 3;
        sum.y += (mesh->tris[i].a.pos.y + mesh->tris[i].b.pos.y +
                  mesh->tris[i].c.pos.y) /
                 3;
        sum.z += (mesh->tris[i].a.pos.z + mesh->tris[i].b.pos.z +
                  mesh->tris[i].c.pos.z) /
                 3;

        count++;
    }

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        sum.x += (mesh->lins[i].a.pos.x + mesh->lins[i].b.pos.x) / 2;
        sum.y += (mesh->lins[i].a.pos.y + mesh->lins[i].b.pos.y) / 2;
        sum.z += (mesh->lins[i].a.pos.z + mesh->lins[i].b.pos.z) / 2;

        count++;
    }

    for (mc_ind_t i = 0; i < mesh->dot_count; ++i) {
        sum.x += mesh->dots[i].pos.x;
        sum.y += mesh->dots[i].pos.y;
        sum.z += mesh->dots[i].pos.z;

        count++;
    }

    return (struct vec3){ sum.x / count, sum.y / count, sum.z / count };
}

void
tetramesh_line(
    struct tetramesh *mesh, struct vec3 a, struct vec3 b, struct vec3 norm
)
{
    struct vec3 const antinorm = vec3_mul_scalar(-1, norm);

    int32_t const l = (int32_t) mesh->lin_count;
    MC_MEM_RESERVEN(mesh->lins, mesh->lin_count, 4);
    mesh->lins[l] = (struct tetra_lin){
        .a = { .pos = a, .col = VEC4_1 },
        .b = { .pos = b, .col = VEC4_1 },
        .norm = norm,
        .prev = INT32_MIN,
        .next = INT32_MIN,
        .inverse = 1 + l,
        .antinorm = 2 + l,
        .is_dominant_sibling = 1,
    };
    mesh->lins[l + 1] = (struct tetra_lin){
        .a = { .pos = b, .col = VEC4_1 },
        .b = { .pos = a, .col = VEC4_1 },
        .norm = norm,
        .prev = INT32_MIN,
        .next = INT32_MIN,
        .inverse = 0 + l,
        .antinorm = 3 + l,
        .is_dominant_sibling = 0,
    };
    mesh->lins[l + 2] = (struct tetra_lin){
        .a = { .pos = b, .col = VEC4_1 },
        .b = { .pos = a, .col = VEC4_1 },
        .norm = antinorm,
        .prev = INT32_MIN,
        .next = INT32_MIN,
        .inverse = 3 + l,
        .antinorm = 0 + l,
        .is_dominant_sibling = 1,
    };
    mesh->lins[l + 3] = (struct tetra_lin){
        .a = { .pos = a, .col = VEC4_1 },
        .b = { .pos = b, .col = VEC4_1 },
        .norm = antinorm,
        .prev = INT32_MIN,
        .next = INT32_MIN,
        .inverse = 2 + l,
        .antinorm = 1 + l,
        .is_dominant_sibling = 0,
    };

    mesh->lin_count += 4;
}

void
tetramesh_line_to(struct tetramesh *mesh, struct vec3 pos)
{
    struct vec3 const antinorm = mesh->lins[mesh->lin_count - 1].norm;
    struct vec3 const norm = mesh->lins[mesh->lin_count - 4].norm;

    struct vec3 const p = mesh->lins[mesh->lin_count - 4].b.pos;

    MC_MEM_RESERVE(mesh->lins, mesh->lin_count);
    mesh->lins[mesh->lin_count] = (struct tetra_lin){
        .a = { .pos = p, .col = VEC4_1 },
        .b = { .pos = pos, .col = VEC4_1 },
        .norm = norm,
        .prev = (int32_t) mesh->lin_count - 4,
        .next = -1,
        .inverse = (int32_t) mesh->lin_count + 1,
        .antinorm = (int32_t) mesh->lin_count + 2,
        .is_dominant_sibling = 1,
    };
    mesh->lins[mesh->lin_count + 1] = (struct tetra_lin){
        .a = { .pos = pos, .col = VEC4_1 },
        .b = { .pos = p, .col = VEC4_1 },
        .norm = norm,
        .prev = -1,
        .next = (int32_t) mesh->lin_count - 3,
        .inverse = (int32_t) mesh->lin_count,
        .antinorm = (int32_t) mesh->lin_count + 3,
        .is_dominant_sibling = 0,
    };
    mesh->lins[mesh->lin_count + 2] = (struct tetra_lin){
        .a = { .pos = pos, .col = VEC4_1 },
        .b = { .pos = p, .col = VEC4_1 },
        .norm = antinorm,
        .prev = -1,
        .next = (int32_t) mesh->lin_count - 2,
        .inverse = (int32_t) mesh->lin_count + 3,
        .antinorm = (int32_t) mesh->lin_count,
        .is_dominant_sibling = 1,
    };
    mesh->lins[mesh->lin_count + 3] = (struct tetra_lin){
        .a = { .pos = p, .col = VEC4_1 },
        .b = { .pos = pos, .col = VEC4_1 },
        .norm = antinorm,
        .prev = (int32_t) mesh->lin_count - 1,
        .next = -1,
        .inverse = (int32_t) mesh->lin_count + 2,
        .antinorm = (int32_t) mesh->lin_count + 1,
        .is_dominant_sibling = 0,
    };

    mesh->lins[mesh->lin_count - 1].next = (int32_t) mesh->lin_count + 3;
    mesh->lins[mesh->lin_count - 2].prev = (int32_t) mesh->lin_count + 2;
    mesh->lins[mesh->lin_count - 3].prev = (int32_t) mesh->lin_count + 1;
    mesh->lins[mesh->lin_count - 4].next = (int32_t) mesh->lin_count;

    mesh->lin_count += 4;
}

/* adds boundary if necessary, otherwise completes loop */
void
tetramesh_line_close(struct tetramesh *mesh)
{
    int32_t j;
    for (j = 0; j < (int32_t) mesh->lin_count; ++j) {
        if (mesh->lins[j].prev == INT32_MIN) {
            break;
        }
    }
    struct vec3 const start = mesh->lins[j].a.pos;
    struct vec3 const end = mesh->lins[mesh->lin_count - 4].b.pos;

    if (vec3_equals(start, end)) {
        /* close loop */
        mesh->lins[mesh->lin_count - 1].next = 3 + j;
        mesh->lins[mesh->lin_count - 2].prev = 2 + j;
        mesh->lins[mesh->lin_count - 3].prev = 1 + j;
        mesh->lins[mesh->lin_count - 4].next = 0 + j;

        mesh->lins[3 + j].prev = (int32_t) mesh->lin_count - 1;
        mesh->lins[2 + j].next = (int32_t) mesh->lin_count - 2;
        mesh->lins[1 + j].next = (int32_t) mesh->lin_count - 3;
        mesh->lins[0 + j].prev = (int32_t) mesh->lin_count - 4;
    }
    else {
        int32_t const d = (int32_t) mesh->dot_count;
        mesh->dot_count += 4;
        mesh->dots =
            mc_reallocf(mesh->dots, sizeof(struct tetra_dot) * mesh->dot_count);
        mesh->dots[d] = (struct tetra_dot){
            .pos = mesh->lins[j].a.pos,
            .col = VEC4_0,
            .norm = mesh->lins[j].norm,
            .inverse = -1 - j,
            .antinorm = d + 1,
            .is_dominant_sibling = 1,
        };
        mesh->dots[d + 1] = (struct tetra_dot){
            .pos = mesh->lins[j + 2].b.pos,
            .col = VEC4_0,
            .norm = mesh->lins[j + 2].norm,
            .inverse = -3 - j,
            .antinorm = d,
            .is_dominant_sibling = 1,
        };
        mesh->dots[d + 2] = (struct tetra_dot){
            .pos = mesh->lins[mesh->lin_count - 4].b.pos,
            .col = VEC4_0,
            .norm = mesh->lins[mesh->lin_count - 4].norm,
            .inverse = -1 - (int32_t) (mesh->lin_count - 4),
            .antinorm = d + 3,
            .is_dominant_sibling = 1,
        };
        mesh->dots[d + 3] = (struct tetra_dot){
            .pos = mesh->lins[mesh->lin_count - 2].a.pos,
            .col = VEC4_0,
            .norm = mesh->lins[mesh->lin_count - 2].norm,
            .inverse = -1 - (int32_t) (mesh->lin_count - 2),
            .antinorm = d + 2,
            .is_dominant_sibling = 1,
        };

        mesh->lins[mesh->lin_count - 1].next = -4 - d;
        mesh->lins[mesh->lin_count - 2].prev = -4 - d;
        mesh->lins[mesh->lin_count - 3].prev = -3 - d;
        mesh->lins[mesh->lin_count - 4].next = -3 - d;

        mesh->lins[j].prev = -1 - d;
        mesh->lins[j + 1].next = -1 - d;
        mesh->lins[j + 2].next = -2 - d;
        mesh->lins[j + 3].prev = -2 - d;
    }
}

static mc_status_t
mesh_tag_dfs(
    struct timeline_execution_context *executor, struct mesh_tag_subset *dump,
    struct vector_field curr_mesh, struct vector_field function,
    mc_bool_t invert
)
{
    /* is leaf */
    struct vector_field curr = vector_field_nocopy_extract_type_message(
        executor, curr_mesh, VECTOR_FIELD_TYPE_MESH | VECTOR_FIELD_TYPE_VECTOR,
        "Invalid mesh tree node. Received %s expected %s"
    );

    if (!curr.vtable) {
        return MC_STATUS_FAIL;
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_MESH) {
        dump->total_count++;
        mc_bool_t add = !invert;

        struct tetramesh *const mesh = curr.value.pointer;

        if (function.vtable) {
            struct vector_field in = vector_init(executor);
            for (mc_ind_t j = 0; j < mesh->tag_count; ++j) {
                struct vector_field elem =
                    double_init(executor, (double) mesh->tags[j]);
                vector_plus(executor, in, &elem);
            }
            function_call(executor, function, 1, &in);

            struct vector_field ret = vector_field_extract_type(
                executor, &executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
            );
            VECTOR_FIELD_FREE(executor, in);

            if (ret.vtable) {
                add = (mc_bool_t) (VECTOR_FIELD_DBOOL(ret) != invert);
            }
            else {
                return MC_STATUS_FAIL;
            }
        }

        if (add) {
            MC_MEM_RESERVE(dump->meshes, dump->subset_count);
            MC_MEM_RESERVE(dump->sources, dump->subset_count);
            dump->meshes[dump->subset_count] = curr.value.pointer;
            dump->sources[dump->subset_count] = curr_mesh;
            ++dump->subset_count;
        }
    }
    else {
        struct vector *const vector = curr.value.pointer;

        for (mc_ind_t i = 0; i < vector->field_count; ++i) {
            if (mesh_tag_dfs(
                    executor, dump, lvalue_init(executor, &vector->fields[i]),
                    function, invert
                ) != 0) {
                return MC_STATUS_FAIL;
            }
        }
    }

    return MC_STATUS_SUCCESS;
}

//[mesh_tree] {[root] {mesh&}, [subtags] {root&, subtags}, [predicate] {root&,
// tag_predicate(tag)}}
// invert_select inverts function result
struct mesh_tag_subset
mesh_subset(
    struct timeline_execution_context *executor, struct vector_field *fields,
    mc_bool_t invert_selection
)
{
    struct mesh_tag_subset ret = { 0 };
    struct mesh_tag_subset const err = { 0, 0, 0, SIZE_MAX };
    LIBMC_FULL_CAST_RETURN(index, 0, VECTOR_FIELD_TYPE_DOUBLE, return err);

    struct vector_field func = VECTOR_FIELD_NULL;

    if (index.value.doub == 1) {
        func = vector_field_nocopy_extract_type(
            executor, fields[2], VECTOR_FIELD_TYPE_FUNCTION
        );
    }

    if (mesh_tag_dfs(executor, &ret, fields[1], func, invert_selection) != 0) {
        mesh_subset_free(ret);
        return err;
    }

    return ret;
}

struct mesh_tag_subset
mesh_fullset(
    struct timeline_execution_context *executor, struct vector_field tree
)
{
    struct mesh_tag_subset ret = { 0 };
    struct mesh_tag_subset const err = { 0, 0, 0, SIZE_MAX };

    if (mesh_tag_dfs(executor, &ret, tree, VECTOR_FIELD_NULL, 0) !=
        MC_STATUS_SUCCESS) {
        mesh_subset_free(ret);
        return err;
    }

    return ret;
}

void
mesh_subset_free(struct mesh_tag_subset subset)
{
    mc_free(subset.meshes);
    mc_free(subset.sources);
}

extern inline struct tetra_tri
tetra_tri_flip(
    struct tetra_tri triangle, int32_t an, int32_t ab, int32_t bc, int32_t ca
);
