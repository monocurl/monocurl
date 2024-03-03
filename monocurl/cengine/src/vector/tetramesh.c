//
//  tetramesh.c
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <math.h>
#include <stdlib.h>
#include <string.h>

#include "callback.h"
#include "primitives.h"
#include "tetramesh.h"

static struct vector_field_vtable const vtable = {
    .type = VECTOR_FIELD_TYPE_MESH,
    .type_name = "mesh",

    .copy = tetramesh_copy,
    .assign = NULL,
    .plus_assign = NULL,

    .op_call = NULL,

    .op_add = NULL,
    .op_multiply = NULL,
    .op_subtract = NULL,
    .op_negative = NULL,
    .op_divide = NULL,
    .op_power = NULL,

    .op_bool = NULL,
    .op_contains = NULL,
    .op_comp = tetramesh_comp,

    .op_index = NULL,
    .op_attribute = NULL,

    .hash = tetramesh_hash,

    .bytes = tetramesh_bytes,
    .free = tetramesh_free,

    .out_of_frame_like = 0,
};

struct vector_field
tetramesh_init(struct timeline_execution_context *executor)
{
    struct tetramesh *ret = mc_calloc(1, sizeof(struct tetramesh));

    ret->modded = 1;
    ret->dirty_hash_cache = 1;
    ret->ref_count = 1;

    return (struct vector_field){
        .value = { .pointer = ret },
        .vtable = &vtable,
    };
}

struct vector_field
tetramesh_init_ptr(
    struct timeline_execution_context *executor, struct tetramesh *ptr
)
{
    ptr->modded = 1;
    ptr->dirty_hash_cache = 1;

    return (struct vector_field){
        .value = { .pointer = ptr },
        .vtable = &vtable,
    };
}

struct tetramesh *
tetramesh_raw_copy(struct tetramesh const *mesh)
{
    struct tetramesh *const ret = mc_calloc(1, sizeof(struct tetramesh));

    ret->tag_count = mesh->tag_count;
    if (ret->tag_count) {
        ret->tags = mc_malloc(sizeof(mc_tag_t) * ret->tag_count);
        memcpy(ret->tags, mesh->tags, sizeof(mc_tag_t) * ret->tag_count);
    }

    ret->dot_count = mesh->dot_count;
    if (ret->dot_count) {
        ret->dots = mc_malloc(sizeof(struct tetra_dot) * ret->dot_count);
        memcpy(
            ret->dots, mesh->dots, sizeof(struct tetra_dot) * ret->dot_count
        );
    }

    ret->lin_count = mesh->lin_count;
    if (ret->lin_count) {
        ret->lins = mc_malloc(sizeof(struct tetra_lin) * ret->lin_count);
        memcpy(
            ret->lins, mesh->lins, sizeof(struct tetra_lin) * ret->lin_count
        );
    }

    ret->tri_count = mesh->tri_count;
    if (ret->tri_count) {
        ret->tris = mc_malloc(sizeof(struct tetra_tri) * ret->tri_count);
        memcpy(
            ret->tris, mesh->tris, sizeof(struct tetra_tri) * ret->tri_count
        );
    }

    ret->dirty_hash_cache = mesh->dirty_hash_cache;
    ret->hash_cache = mesh->hash_cache;
    ret->payload = mesh->payload;
    ret->flags = mesh->flags;

    ret->texture_handle = mesh->texture_handle; /* texture handles are shared*/

    ret->uniform = mesh->uniform;

    ret->modded = 1;
    ret->ref_count = 1;

    return ret;
}

struct vector_field
tetramesh_copy(
    struct timeline_execution_context *executor, struct vector_field src
)
{
    struct tetramesh *const ret = src.value.pointer;
    tetramesh_ref(ret);

    return (struct vector_field){
        .value = { .pointer = ret },
        .vtable = &vtable,
    };
}

struct vector_field
tetramesh_owned(
    struct timeline_execution_context *executor,
    struct vector_field tetramesh_wrapper
)
{

    struct vector_field src = vector_field_nocopy_extract_type(
        executor, tetramesh_wrapper, VECTOR_FIELD_TYPE_MESH
    );
    struct tetramesh *const ret = tetramesh_raw_copy(src.value.pointer);
    /* likely about to be modded */
    ret->dirty_hash_cache = ret->modded = 1;

    return (struct vector_field){
        .value = { .pointer = ret },
        .vtable = &vtable,
    };
}

mc_count_t
tetramesh_bytes(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    struct tetramesh *mesh = field.value.pointer;
    mc_count_t ret = sizeof(*mesh) + sizeof(field);
    ret += mesh->tri_count * sizeof(*mesh->tris);
    ret += mesh->lin_count * sizeof(*mesh->lins);
    ret += mesh->dot_count * sizeof(*mesh->dots);
    ret += mesh->tag_count * sizeof(*mesh->tags);

    return ret;
}
mc_hash_t
tetramesh_hash(
    struct timeline_execution_context *executor, struct vector_field m
)
{
    struct tetramesh *const mesh = m.value.pointer;

    if (mesh->dirty_hash_cache) {
        mc_hash_t hash = 5381;

        hash ^= 0x9e3779b9 + mesh->texture_handle + (hash << 16) + (hash >> 12);

        hash ^= 0x9e3779b9 + mesh->tag_count + (hash << 16) + (hash >> 12);

        hash ^= 0x9e3779b9 + mesh->dot_count + (hash << 16) + (hash >> 12);
        hash ^= 0x9e3779b9 + mesh->lin_count + (hash << 16) + (hash >> 12);
        hash ^= 0x9e3779b9 + mesh->tri_count + (hash << 16) + (hash >> 12);

#pragma message(                                                               \
    "OPTIMIZATION: make this better! and avoid packing assumptions as well..." \
)
        hash ^= 0x9e3779b9 + (mc_hash_t) (mesh->uniform.z_class * 0xFFFF) +
                (hash << 16) + (hash >> 12);
        hash ^= 0x9e3779b9 + (mc_hash_t) mesh->uniform.smooth ^
                (mc_hash_t) (mesh->uniform.opacity * 0xFF) + (hash << 16) +
                    (hash >> 12);
        hash ^= 0x9e3779b9 + (mc_hash_t) mesh->uniform.dot_vertex_count ^
                (mc_hash_t) (mesh->uniform.gloss * 0xFF) + (hash << 16) +
                    (hash >> 12);
        hash ^= 0x9e3779b9 + (mc_hash_t) (mesh->uniform.dot_radius * 0xFFFF) +
                (hash << 16) + (hash >> 12);
        hash ^= 0x9e3779b9 +
                (mc_hash_t) (mesh->uniform.stroke_radius * 0xFFFF) +
                (hash << 16) + (hash >> 12);
        hash ^= 0x9e3779b9 +
                (mc_hash_t) (mesh->uniform.stroke_miter_radius_scale * 0xFFFF) +
                (hash << 16) + (hash >> 12);
        hash ^= 0x9e3779b9 +
                str_hash(
                    (unsigned char const *) mesh->tris,
                    sizeof(struct tetra_tri) * mesh->tri_count
                ) +
                (hash << 16) + (hash >> 12);
        hash ^= 0x9e3779b9 +
                str_hash(
                    (unsigned char const *) mesh->lins,
                    sizeof(struct tetra_lin) * mesh->lin_count
                ) +
                (hash << 16) + (hash >> 12);
        hash ^= 0x9e3779b9 +
                str_hash(
                    (unsigned char const *) mesh->dots,
                    sizeof(struct tetra_dot) * mesh->dot_count
                ) +
                (hash << 16) + (hash >> 12);
        hash ^= 0x9e3779b9 +
                str_hash(
                    (unsigned char const *) mesh->tags,
                    sizeof(mc_tag_t) * mesh->tag_count
                ) +
                (hash << 16) + (hash >> 12);

        mesh->hash_cache = hash;
        mesh->dirty_hash_cache = 0;
    }

    return mesh->hash_cache;
}

struct vector_field
tetramesh_comp(
    struct timeline_execution_context *executor, struct vector_field mesh,
    struct vector_field *rhs
)
{
    struct vector_field rhs_val =
        vector_field_safe_extract_type(executor, *rhs, VECTOR_FIELD_TYPE_MESH);

    int ret;
    if (!rhs_val.vtable) {
        return double_init(executor, 1);
    }
    else if ((ret = (int) rhs_val.vtable->type - (int) VECTOR_FIELD_TYPE_MESH) != 0) {
        return double_init(executor, ret);
    }

    struct tetramesh *const m = mesh.value.pointer;
    struct tetramesh *const r = rhs_val.value.pointer;

    if (m == r) {
        return double_init(executor, 0);
    }

    if (m->texture_handle != r->texture_handle) {
        return double_init(
            executor, (int) m->texture_handle - (int) r->texture_handle
        );
    }
    else {
        if ((ret = (int) m->tri_count - (int) r->tri_count)) {
            return double_init(executor, ret);
        }
        if ((ret = (int) m->lin_count - (int) r->lin_count)) {
            return double_init(executor, ret);
        }
        if ((ret = (int) m->dot_count - (int) r->dot_count)) {
            return double_init(executor, ret);
        }

        if ((ret = (int) m->tag_count - (int) r->tag_count)) {
            return double_init(executor, ret);
        }

        if ((ret = memcmp(
                 m->tris, r->tris, sizeof(struct tetra_tri) * m->tri_count
             ))) {
            return double_init(executor, ret);
        }
        if ((ret = memcmp(
                 m->lins, r->lins, sizeof(struct tetra_lin) * m->lin_count
             ))) {
            return double_init(executor, ret);
        }
        if ((ret = memcmp(
                 m->dots, r->dots, sizeof(struct tetra_dot) * m->dot_count
             ))) {
            return double_init(executor, ret);
        }

        if ((ret = memcmp(m->tags, r->tags, sizeof(mc_tag_t) * m->tag_count))) {
            return double_init(executor, ret);
        }

#pragma message("TODO work on better comparisons that dont read padding!")
        if ((ret = memcmp(
                 &m->uniform, &r->uniform, sizeof(struct tetramesh_uniforms)
             ))) {
            return double_init(executor, ret);
        }
    }

    return double_init(executor, 0);
}

void
tetramesh_ref(struct tetramesh *mesh)
{
    ++mesh->ref_count;
}

/* must be called with timeline lock */
void
tetramesh_unref(struct tetramesh *mesh)
{
    if (--mesh->ref_count) {
        return;
    }

    if (mesh->dot_handle) {
        free_buffer(mesh->dot_handle);
    }
    if (mesh->lin_handle) {
        free_buffer(mesh->lin_handle);
    }
    if (mesh->tri_handle) {
        free_buffer(mesh->tri_handle);
    }
    if (mesh->vert_uniform_handle) {
        free_buffer(mesh->vert_uniform_handle);
    }
    if (mesh->frag_uniform_handle) {
        free_buffer(mesh->frag_uniform_handle);
    }

    mc_free(mesh->tris);
    mc_free(mesh->lins);
    mc_free(mesh->dots);

    mc_free(mesh->tags);

    mc_free(mesh);
}

void
tetramesh_free(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    struct tetramesh *const mesh = field.value.pointer;
    tetramesh_unref(mesh);
}

extern inline int32_t
tetramesh_tri_edge(struct tetra_tri const *tri, int edge);

extern inline void
tetramesh_tri_set_edge(struct tetra_tri *tri, int edge, int32_t val);

extern inline int
tetramesh_tri_edge_for(struct tetra_tri const *tri, int32_t val);

extern inline void
tetramesh_tri_read_edge(
    struct tetra_tri const *tri, int edge, struct tetra_tri_vertex *out
);

extern inline struct tetra_tri_vertex
tetramesh_tri_vertex_lerp(
    struct tetra_tri_vertex a, struct tetra_tri_vertex b, float t
);

mc_bool_t
tetramesh_assert_invariants(struct tetramesh *mesh)
{
    if (!MC_DEBUG) {
        return 1;
    }

    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        if (mesh->tris[mesh->tris[i].antinorm].antinorm != (int32_t) i) {
            assert(0);
            return 0;
        }

        if (mesh->tris[i].is_dominant_sibling ==
            mesh->tris[mesh->tris[i].antinorm].is_dominant_sibling) {
            assert(0);
            return 0;
        }

        for (int e = 0; e < 3; ++e) {
            int32_t const val = tetramesh_tri_edge(&mesh->tris[i], e);
            struct tetra_tri_vertex buff[2];
            tetramesh_tri_read_edge(&mesh->tris[i], e, buff);
            if (val < 0) {
                if (mesh->lins[-1 - val].inverse != -1 - (int32_t) i) {
                    assert(0);
                    return 0;
                }
                if (!vec3_equals(mesh->lins[-1 - val].a.pos, buff[1].pos) ||
                    !vec3_equals(mesh->lins[-1 - val].b.pos, buff[0].pos)) {
                    assert(0);
                    return 0;
                }
            }
            else {
                if (tetramesh_tri_edge_for(&mesh->tris[val], (int32_t) i) ==
                    -1) {
                    assert(0);
                    return 0;
                }
            }
        }
    }

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        if (mesh->lins[i].inverse >= 0) {
            if (mesh->lins[mesh->lins[i].inverse].inverse != (int32_t) i) {
                assert(0);
                return 0;
            }

            if (mesh->lins[i].is_dominant_sibling ==
                mesh->lins[mesh->lins[i].inverse].is_dominant_sibling) {
                assert(0);
                return 0;
            }
        }

        if (mesh->lins[i].next >= 0) {
            if (mesh->lins[mesh->lins[i].next].prev != (int32_t) i) {
                assert(0);
                return 0;
            }
            if (!vec3_equals(
                    mesh->lins[i].b.pos, mesh->lins[mesh->lins[i].next].a.pos
                )) {
                assert(0);
                return 0;
            }
        }

        if (mesh->lins[i].next < 0 &&
            (-1 - mesh->lins[i].next) >= (int32_t) mesh->dot_count) {
            assert(0);
            return 0;
        }

        if (mesh->lins[i].prev >= 0) {
            if (mesh->lins[mesh->lins[i].prev].next != (int32_t) i) {
                assert(0);
                return 0;
            }
        }

        if (mesh->lins[i].prev < 0 &&
            (-1 - mesh->lins[i].prev) >= (int32_t) mesh->dot_count) {
            assert(0);
            return 0;
        }
    }

    return 1;
}
