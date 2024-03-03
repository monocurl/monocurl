//
//  mc_meshes.h
//  Monocurl
//
//  Created by Manu Bhat on 2/19/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "callback.h"
#include "mc_env.h"
#include "mc_lib_helpers.h"
#include "tetramesh.h"

struct vec3_plane_covering {
    mc_bool_t *enabled_points;
    struct vec3 *points;
    mc_count_t count, rows, cols;
};

struct mesh_tag_subset {
    mc_count_t subset_count;
    struct tetramesh **meshes;
    struct vector_field *sources; /* unowned vector fields (no copy) */

    mc_count_t total_count;
};

#if MC_INTERNAL
mc_bool_t
tetramesh_is_single_gaussian(struct tetramesh const *tetramesh);
mc_bool_t
tetramesh_is_gaussian_set(struct tetramesh const *tetramesh);
mc_bool_t
tetramesh_is_convex(struct tetramesh const *tetramesh);
mc_bool_t
tetramesh_is_path(struct tetramesh const *tetramesh);
mc_bool_t
tetramesh_is_loop_set(struct tetramesh const *tetramesh);
mc_bool_t
tetramesh_is_single_loop(struct tetramesh const *tetramesh);
mc_bool_t
tetramesh_is_planar1d(struct tetramesh const *tetramesh);
mc_bool_t
tetramesh_is_planar2d(struct tetramesh const *tetramesh);

/* something in the form of [points] {[main] {x_min, x_max, y_min, y_max},
 * [step] {x_min, x_max, y_min, y_max, x_step, y_step}, [domain] {domain,
 * resample_rate}} */
struct vec3_plane_covering
tetramesh_planar2d_sample(
    struct timeline_execution_context *executor, struct vector_field *sampler
);
void
vec3_plane_covering_free(struct vec3_plane_covering covering);

/* only defined for 2d planar, otherwise returns vec3(0,0,0) */
struct vec3
tetramesh_normal(struct tetramesh const *tetramesh);

/* in place */
mc_status_t
tetramesh_uprank(struct tetramesh *tetramesh, mc_bool_t);

void
tetramesh_downrank(
    struct timeline_execution_context *executor, struct tetramesh *tetramesh
);

int
tetramesh_rank(struct tetramesh const *tetramesh);

mc_count_t
tetramesh_contour_count(
    struct timeline_execution_context *executor,
    struct tetramesh const *tetramesh
);
mc_status_t
mesh_contour_separate(
    struct timeline_execution_context *executor, struct vector_field *value
);

mc_bool_t
tetramesh_contains(struct tetramesh const *tetramesh, struct vec3 pos);

void
tetramesh_tesselate(struct tetramesh *mesh, mc_count_t target_count);

struct vec3
tetramesh_com(struct tetramesh const *mesh);

/* line buidling */

/* assumes empty mesh */
void
tetramesh_line(
    struct tetramesh *mesh, struct vec3 a, struct vec3 b, struct vec3 norm
);

void
tetramesh_line_to(struct tetramesh *mesh, struct vec3 pos);
/* adds boundary if necessary, otherwise completes looop */
void
tetramesh_line_close(struct tetramesh *mesh);

#define LIBMC_SELECT_RETURN(mesh, index, return)                               \
    struct mesh_tag_subset mesh = mesh_subset(executor, &fields[index], 0);    \
    do {                                                                       \
        if (mesh.total_count == SIZE_MAX) {                                    \
            executor->return_register = VECTOR_FIELD_NULL;                     \
            return;                                                            \
        }                                                                      \
    } while (0)

#define LIBMC_SELECT(mesh, index) LIBMC_SELECT_RETURN(mesh, index, return)

struct mesh_tag_subset
mesh_subset(
    struct timeline_execution_context *executor, struct vector_field *tags,
    mc_bool_t invert_selection
);

struct mesh_tag_subset
mesh_fullset(
    struct timeline_execution_context *executor, struct vector_field tree
);

void
mesh_subset_free(struct mesh_tag_subset subset);

inline struct tetra_tri
tetra_tri_flip(
    struct tetra_tri triangle, int32_t an, int32_t ab, int32_t bc, int32_t ca
)
{
    triangle = (struct tetra_tri){
        .a = triangle.b,
        .b = triangle.a,
        .c = triangle.c,
        .ab = ab,
        .bc = bc,
        .ca = ca,
        .antinorm = an,
        .is_dominant_sibling = !triangle.is_dominant_sibling,
    };

    triangle.a.norm = vec3_mul_scalar(-1, triangle.a.norm);
    triangle.b.norm = vec3_mul_scalar(-1, triangle.b.norm);
    triangle.c.norm = vec3_mul_scalar(-1, triangle.c.norm);

    return triangle;
}
#endif
