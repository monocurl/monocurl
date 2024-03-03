//
//  tetramesh.h
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <assert.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>

#include "geo.h"
#include "mc_assert.h"
#include "vector_field.h"

#define TETRAMESH_FLAG_WANTS_PLANAR_TRANSFORM (1uLL << 0)

// so half edge structure able to represent all gaussian surfaces
// triangles have three neighbors, and a twin (which could be a tetrahedron)
// lins have exactly two neighbors, but also a twin
// hedrons have four neighbors, no twins

struct tetramesh {

    /* padding done to ensure consistent hashes...*/
#pragma message(                                                                         \
    "TODO do not make assumption that there's no padding (even if manually enforced...)" \
)
    /* the antinorm is the same, but a and b are flipped. c is necessarily the
     * same position */
    struct tetra_tri {
        // ccw clipped?
        struct tetra_tri_vertex {
            struct vec3 pos;
            struct vec3 norm;
            struct vec2 uv;
            struct vec4 col;
        } a, b, c;

        /* boundaries, negative one indexed if it's a line */
        int32_t ab;
        int32_t bc;
        int32_t ca;

        int32_t antinorm; /* twin, negative one_indexed if it's a tetrahedron */
        int32_t is_dominant_sibling; /* defines orientation */
    } *tris;

    // assumed ccw in conjunction with norm
    struct tetra_lin {
        struct tetra_lin_vertex {
            struct vec3 pos;
            struct vec4 col;
        } a, b;

        struct vec3 norm;

        int32_t prev, next;
        int32_t inverse; /* twin, negative one_indexed if it's a triangle, and
                            possibly nothing */
        int32_t antinorm;
        int32_t is_dominant_sibling; /* defines orientation. for lines, non
                                    dominant siblings are not drawn for now */
    } *lins;

    struct tetra_dot {
        struct vec3 pos;
        struct vec4 col;

        struct vec3 norm;

#pragma message(                                                                                                                                  \
    "TODO, seems like this isn't properly defined since multiple lins can have the same inverse???. Dots arent really well defined in general..." \
)
        int32_t inverse; /* twin cap */
        int32_t antinorm;
        int32_t is_dominant_sibling; /* defines orientation */
    } *dots;

    mc_count_t dot_count, lin_count, tri_count;

    struct tetramesh_uniforms {
        double z_class;

        float opacity;
        float stroke_miter_radius_scale;
        float stroke_radius;

        float dot_radius;
        unsigned short dot_vertex_count;

        mc_bool_t smooth; // vertex normals or face normals, probably ignore
                          // this for now?

        float gloss;
    } uniform;

    /* does not affect rendering, so not a uniform? */
    mc_tag_t *tags;
    mc_count_t tag_count;

    /* special methods? vtable??? separate classes??? two is fine for now... */
    //    struct vec3 (*local_to_global)(struct tetramesh *axis, struct vec3
    //    local); struct vec3 (*global_to_local)(struct tetramesh *axis, struct
    //    vec3 global);

#pragma message(                                                                                                                   \
    "OPTIMIZATION: vertex handles don't really need to be called explicitly, since they'll be requested in the copy? change this!" \
)
    mc_handle_t texture_handle, dot_handle, lin_handle, tri_handle,
        vert_uniform_handle, frag_uniform_handle;

    mc_bool_t modded, dirty_hash_cache;
    mc_hash_t hash_cache;
    /* for stuff like sorting, should not be used for comparisons */
    mc_hash_t payload;
    /* only use right now is when tag matching */
    uint64_t flags;

    mc_count_t ref_count;
};

#if MC_INTERNAL
struct vector_field
tetramesh_init(struct timeline_execution_context *executor);

struct vector_field
tetramesh_init_ptr(
    struct timeline_execution_context *executor, struct tetramesh *ptr
);

struct tetramesh *
tetramesh_raw_copy(struct tetramesh const *mesh);

struct vector_field
tetramesh_copy(
    struct timeline_execution_context *executor, struct vector_field tetramesh
);

struct vector_field
tetramesh_owned(
    struct timeline_execution_context *executor,
    struct vector_field tetramesh_wrapper
);

struct vector_field
tetramesh_comp(
    struct timeline_execution_context *executor, struct vector_field field,
    struct vector_field *rhs
);

mc_hash_t
tetramesh_hash(
    struct timeline_execution_context *executor, struct vector_field field
);

mc_count_t
tetramesh_bytes(
    struct timeline_execution_context *executor, struct vector_field field
);

/* to be called on timeline thread */
void
tetramesh_ref(struct tetramesh *mesh);

void
tetramesh_unref(struct tetramesh *mesh);

void
tetramesh_free(
    struct timeline_execution_context *executor, struct vector_field field
);

inline int32_t
tetramesh_tri_edge(struct tetra_tri const *tri, int edge)
{
    if (edge == 0) {
        return tri->ab;
    }
    else if (edge == 1) {
        return tri->bc;
    }
    else {
        assert(edge == 2);
        return tri->ca;
    }
}

inline void
tetramesh_tri_set_edge(struct tetra_tri *tri, int edge, int32_t val)
{
    if (edge == 0) {
        tri->ab = val;
    }
    else if (edge == 1) {
        tri->bc = val;
    }
    else {
        assert(edge == 2);
        tri->ca = val;
    }
}

inline int
tetramesh_tri_edge_for(struct tetra_tri const *tri, int32_t val)
{
    if (tri->ab == val) {
        return 0;
    }
    else if (tri->bc == val) {
        return 1;
    }
    else if (tri->ca == val) {
        return 2;
    }

    return -1;
}

// out is at least two elements long
// read in reverse direction!
inline void
tetramesh_tri_read_edge(
    struct tetra_tri const *tri, int edge, struct tetra_tri_vertex *out
)
{
    if (edge == 0) {
        out[0] = tri->b;
        out[1] = tri->a;
    }
    else if (edge == 1) {
        out[0] = tri->c;
        out[1] = tri->b;
    }
    else {
        assert(edge == 2);
        out[0] = tri->a;
        out[1] = tri->c;
    }
}

inline struct tetra_tri_vertex
tetramesh_tri_vertex_lerp(
    struct tetra_tri_vertex a, struct tetra_tri_vertex b, float t
)
{
    return (struct tetra_tri_vertex){
        vec3_lerp(a.pos, t, b.pos),
        vec3_lerp(a.norm, t, b.norm),
        vec2_lerp(a.uv, t, b.uv),
        vec4_lerp(a.col, t, b.col),
    };
}

mc_bool_t
tetramesh_assert_invariants(struct tetramesh *mesh);

#endif
