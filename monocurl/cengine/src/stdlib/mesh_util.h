//
//  mesh_util.h
//  Monocurl
//
//  Created by Manu Bhat on 2/22/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_meshes.h"
#include "tetramesh.h"

/*
 func mesh_lerp([mesh_tree] {[root] {mesh}, [subtags] {root, subtags},
 [predicate] {root, tag_predicate(tag)}}, target, t) = native
 not_implemented_yet(0) func mesh_bend([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}, target, t) = native
 not_implemented_yet(0) func mesh_sample([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}, t) = native
 not_implemented_yet(0) func mesh_normal([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}, t) = native
 not_implemented_yet(0) func mesh_tangent([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}, t) = native
 not_implemented_yet(0) func mesh_vertex_set([mesh_tree] {[root] {mesh},
 [subtags] {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_edge_set([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_select_tags([mesh_tree] {[root] {mesh},
 [subtags] {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_left([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_right([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_up([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_down([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_forward([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_backward([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_direc([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}, head, direction) =
 native not_implemented_yet(0) func mesh_rank([mesh_tree] {[root] {mesh},
 [subtags] {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_raycast([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}, src, direction) =
 native not_implemented_yet(0) func mesh_contains([mesh_tree] {[root] {mesh},
 [subtags] {root, subtags}, [predicate] {root, tag_predicate(tag)}}, point) =
 native not_implemented_yet(0) func mesh_center([mesh_tree] {[root] {mesh},
 [subtags] {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_dist([mesh_tree] {[root] {mesh}, [subtags]
 {root, subtags}, [predicate] {root, tag_predicate(tag)}}) = native
 not_implemented_yet(0) func mesh_contour_count([mesh_tree] {[root] {mesh},
 [subtags] {root, subtags}, [predicate] {root, tag_predicate(tag)}}, target, t)
 = native not_implemented_yet(0)
 */

#if MC_INTERNAL
LIBMC_DEC_FUNC(mesh_lerp);
LIBMC_DEC_FUNC(mesh_bend);
LIBMC_DEC_FUNC(mesh_sample);
LIBMC_DEC_FUNC(mesh_normal);
LIBMC_DEC_FUNC(mesh_tangent);
LIBMC_DEC_FUNC(mesh_edge_set);
LIBMC_DEC_FUNC(mesh_select_tags);
LIBMC_DEC_FUNC(mesh_direc);
LIBMC_DEC_FUNC(mesh_left);
LIBMC_DEC_FUNC(mesh_right);
LIBMC_DEC_FUNC(mesh_up);
LIBMC_DEC_FUNC(mesh_down);
LIBMC_DEC_FUNC(mesh_forward);
LIBMC_DEC_FUNC(mesh_backward);
LIBMC_DEC_FUNC(mesh_rank);
LIBMC_DEC_FUNC(mesh_raycast);
LIBMC_DEC_FUNC(mesh_contains);
struct vec3
lib_mc_mesh_vec3_center(
    struct timeline_execution_context *executor, struct mesh_tag_subset tags
);
struct vec3
lib_mc_mesh_vec3_center_fields(
    struct timeline_execution_context *executor, struct vector_field *fields
);
LIBMC_DEC_FUNC(mesh_center);
LIBMC_DEC_FUNC(mesh_dist);
LIBMC_DEC_FUNC(mesh_contour_count);
LIBMC_DEC_FUNC(mesh_contour_separated);
LIBMC_DEC_FUNC(mesh_matched);

LIBMC_DEC_FUNC(mesh_bounding_box);
LIBMC_DEC_FUNC(mesh_vertex_set);
LIBMC_DEC_FUNC(mesh_edge_set);
LIBMC_DEC_FUNC(mesh_triangle_set);
LIBMC_DEC_FUNC(mesh_wireframe);
LIBMC_DEC_FUNC(mesh_hash);

struct vec3
mesh_subset_full_cast(
    struct mesh_tag_subset tags, struct vec3 src, struct vec3 out
);

void
mesh_rotate(
    struct tetramesh *dump, struct tetramesh *mesh, struct vec3 com,
    struct vec3 rotation, float alpha
);

void
mesh_apply_matrix(
    struct mesh_tag_subset mesh, struct vec3 ihat, struct vec3 jhat,
    struct vec3 khat
);

LIBMC_DEC_FUNC(mesh_tag_apply);

float
mesh_direction(struct mesh_tag_subset subset, struct vec3 direction);

void
mesh_patharc_lerp(
    struct tetramesh const *a, struct tetramesh *dump,
    struct tetramesh *const b, float t, struct vec3 path_arc
);

void
mesh_bend_lerp(
    struct tetramesh const *a, struct tetramesh *dump,
    struct tetramesh *const b, float t
);

#endif
