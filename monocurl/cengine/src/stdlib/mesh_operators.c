//
//  mesh_operators.c
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <string.h>

#include "anim_transform.h"
#include "lvalue.h"
#include "mc_util.h"
#include "mesh_operators.h"
#include "mesh_util.h"

#define MAX_SUBDIVISION 5
#define MAX_LINE_SUBDIVISION 1024
#define REVOLVE_STEP_RATE 0.2f
#define LIBMC_OP_MESH                                                          \
    LIBMC_FULL_CAST(_mesh_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);                   \
    struct mesh_tag_subset mesh = mesh_subset(executor, &fields[0], 0);        \
    if (mesh.total_count == SIZE_MAX) {                                        \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        return;                                                                \
    }                                                                          \
    struct vector_field out = vector_init(executor);                           \
    do {                                                                       \
        if (_mesh_ind.value.doub != 0) {                                       \
            struct mesh_tag_subset const _invert =                             \
                mesh_subset(executor, &fields[0], 1);                          \
            for (mc_ind_t i = 0; i < _invert.subset_count; ++i) {              \
                struct vector_field sub_out =                                  \
                    VECTOR_FIELD_COPY(executor, _invert.sources[i]);           \
                vector_plus(executor, out, &sub_out);                          \
            }                                                                  \
            mesh_subset_free(_invert);                                         \
        }                                                                      \
    } while (0)

static void
shift_mesh(
    struct timeline_execution_context *executor, struct vector_field out_vector,
    struct vec3 shift, struct mesh_tag_subset mesh
)
{
    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *const curr = mesh.meshes[i];
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        for (mc_ind_t j = 0; j < curr->dot_count; ++j) {
            dump->dots[j].pos = vec3_add(curr->dots[j].pos, shift);
        }

        for (mc_ind_t j = 0; j < curr->lin_count; ++j) {
            dump->lins[j].a.pos = vec3_add(curr->lins[j].a.pos, shift);
            dump->lins[j].b.pos = vec3_add(curr->lins[j].b.pos, shift);
        }

        for (mc_ind_t j = 0; j < curr->tri_count; ++j) {
            dump->tris[j].a.pos = vec3_add(curr->tris[j].a.pos, shift);
            dump->tris[j].b.pos = vec3_add(curr->tris[j].b.pos, shift);
            dump->tris[j].c.pos = vec3_add(curr->tris[j].c.pos, shift);
        }

        vector_plus(executor, out_vector, &sub_out);
        dump->modded = dump->dirty_hash_cache = 1;
    }
}

void
lib_mc_mesh_shift(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 shift;
    LIBMC_VEC3(shift, 3);
    LIBMC_OP_MESH;

    shift_mesh(executor, out, shift, mesh);

    executor->return_register = out;

    mesh_subset_free(mesh);
}

void
lib_mc_mesh_scale(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 scale;
    LIBMC_FULL_CAST(
        scale_f, 3, VECTOR_FIELD_TYPE_DOUBLE | VECTOR_FIELD_TYPE_VECTOR
    );
    if (scale_f.vtable->type & VECTOR_FIELD_TYPE_DOUBLE) {
        scale.x = scale.y = scale.z = (float) scale_f.value.doub;
    }
    else {
        LIBMC_VEC3(scale, 3);
    }

    LIBMC_OP_MESH;

    struct vec3 const com = lib_mc_mesh_vec3_center(executor, mesh);

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *const curr = mesh.meshes[i];
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        for (mc_ind_t j = 0; j < curr->dot_count; ++j) {
            dump->dots[j].pos = vec3_add(
                vec3_mul_vec3(scale, vec3_sub(curr->dots[j].pos, com)), com
            );
        }

        for (mc_ind_t j = 0; j < curr->lin_count; ++j) {
            dump->lins[j].a.pos = vec3_add(
                vec3_mul_vec3(scale, vec3_sub(curr->lins[j].a.pos, com)), com
            );
            dump->lins[j].b.pos = vec3_add(
                vec3_mul_vec3(scale, vec3_sub(curr->lins[j].b.pos, com)), com
            );
        }

        for (mc_ind_t j = 0; j < curr->tri_count; ++j) {
            dump->tris[j].a.pos = vec3_add(
                vec3_mul_vec3(scale, vec3_sub(curr->tris[j].a.pos, com)), com
            );
            dump->tris[j].b.pos = vec3_add(
                vec3_mul_vec3(scale, vec3_sub(curr->tris[j].b.pos, com)), com
            );
            dump->tris[j].c.pos = vec3_add(
                vec3_mul_vec3(scale, vec3_sub(curr->tris[j].c.pos, com)), com
            );
        }

        vector_plus(executor, out, &sub_out);
        dump->modded = dump->dirty_hash_cache = 1;
    }

    executor->return_register = out;

    mesh_subset_free(mesh);
}

void
lib_mc_mesh_embed_in_space(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 center;
    LIBMC_VEC3(center, 3);
    LIBMC_FULL_CAST(x_scale, 4, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(y_scale, 5, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(z_scale, 6, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_OP_MESH;
    struct vec3 const scale = { 1 / (float) x_scale.value.doub,
                                1 / (float) y_scale.value.doub,
                                1 / (float) z_scale.value.doub };

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *const curr = mesh.meshes[i];
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        for (mc_ind_t j = 0; j < curr->dot_count; ++j) {
            dump->dots[j].pos =
                vec3_add(vec3_mul_vec3(scale, curr->dots[j].pos), center);
        }

        for (mc_ind_t j = 0; j < curr->lin_count; ++j) {
            dump->lins[j].a.pos =
                vec3_add(vec3_mul_vec3(scale, curr->lins[j].a.pos), center);
            dump->lins[j].b.pos =
                vec3_add(vec3_mul_vec3(scale, curr->lins[j].b.pos), center);
        }

        for (mc_ind_t j = 0; j < curr->tri_count; ++j) {
            dump->tris[j].a.pos =
                vec3_add(vec3_mul_vec3(scale, curr->tris[j].a.pos), center);
            dump->tris[j].b.pos =
                vec3_add(vec3_mul_vec3(scale, curr->tris[j].b.pos), center);
            dump->tris[j].c.pos =
                vec3_add(vec3_mul_vec3(scale, curr->tris[j].c.pos), center);
        }

        vector_plus(executor, out, &sub_out);
        dump->modded = dump->dirty_hash_cache = 1;
    }

    executor->return_register = out;

    mesh_subset_free(mesh);
}

void
lib_mc_mesh_rotate(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 rotation;
    float alpha;
    LIBMC_FULL_CAST(
        rot_v, 3, VECTOR_FIELD_TYPE_DOUBLE | VECTOR_FIELD_TYPE_VECTOR
    );
    if (rot_v.vtable->type & VECTOR_FIELD_TYPE_DOUBLE) {
        rotation = (struct vec3){ 0, 0, 1 };
        alpha = (float) rot_v.value.doub;
    }
    else {
        LIBMC_VEC3(rotation, 3);
        alpha = vec3_norm(rotation);
        rotation = vec3_unit(rotation);
    }

    LIBMC_OP_MESH;

    struct vec3 const com = lib_mc_mesh_vec3_center(executor, mesh);

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *const curr = mesh.meshes[i];
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        mesh_rotate(dump, curr, com, rotation, alpha);
        dump->modded = dump->dirty_hash_cache = 1;

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;

    mesh_subset_free(mesh);
}

void
lib_mc_mesh_project(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 ray;
    LIBMC_VEC3(ray, 6);
    LIBMC_SELECT(screen, 3);

    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *const tag = mesh.meshes[i];
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        for (mc_ind_t j = 0; j < tag->dot_count; ++j) {
            dump->dots[j].pos =
                mesh_subset_full_cast(screen, tag->dots[j].pos, ray);
        }

        for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
            dump->lins[j].a.pos =
                mesh_subset_full_cast(screen, tag->lins[j].a.pos, ray);
            dump->lins[j].b.pos =
                mesh_subset_full_cast(screen, tag->lins[j].b.pos, ray);
        }

        for (mc_ind_t j = 0; j < tag->tri_count; ++j) {
            dump->tris[j].a.pos =
                mesh_subset_full_cast(screen, tag->tris[j].a.pos, ray);
            dump->tris[j].b.pos =
                mesh_subset_full_cast(screen, tag->tris[j].b.pos, ray);
            dump->tris[j].c.pos =
                mesh_subset_full_cast(screen, tag->tris[j].c.pos, ray);
        }

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;

    mesh_subset_free(screen);
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_faded(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(opacity, 3, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        dump->uniform.opacity = (float) opacity.value.doub;
        dump->modded = dump->dirty_hash_cache = 1;

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;

    mesh_subset_free(mesh);
}

void
lib_mc_mesh_zindex(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(index, 3, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        dump->uniform.z_class = index.value.doub;
        dump->modded = dump->dirty_hash_cache = 1;

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;

    mesh_subset_free(mesh);
}

struct _mapped_data {
    mc_count_t numel, offset;
    float *elems;
};

static inline struct vec2
read_vec2(struct _mapped_data *data)
{
    struct vec2 ret = { data->elems[data->offset],
                        data->elems[data->offset + 1] };
    data->offset += 2;
    return ret;
}

static inline struct vec3
read_vec3(struct _mapped_data *data)
{
    struct vec3 ret = { data->elems[data->offset],
                        data->elems[data->offset + 1],
                        data->elems[data->offset + 2] };
    data->offset += 3;
    return ret;
}

static inline struct vec4
read_vec4(struct _mapped_data *data)
{
    struct vec4 ret = { data->elems[data->offset],
                        data->elems[data->offset + 1],
                        data->elems[data->offset + 2],
                        data->elems[data->offset + 3] };
    data->offset += 4;
    return ret;
}

static mc_status_t
_map_single(
    struct timeline_execution_context *executor, struct vector_field func,
    struct vec3 in, struct _mapped_data *out, mc_count_t vec_size
)
{
    MC_MEM_RESERVEN(out->elems, out->numel, vec_size);

    struct vector_field vector_in = vector_init(executor);
    struct vector_field aux = double_init(executor, in.x);
    vector_plus(executor, vector_in, &aux);
    aux = double_init(executor, in.y);
    vector_plus(executor, vector_in, &aux);
    aux = double_init(executor, in.z);
    vector_plus(executor, vector_in, &aux);

    function_call(executor, func, 1, &vector_in);
    struct vector_field const ret = vector_field_extract_type(
        executor, &executor->return_register, VECTOR_FIELD_TYPE_VECTOR
    );
    executor->return_register = VECTOR_FIELD_NULL;

    if (!ret.vtable) {
        VECTOR_FIELD_FREE(executor, vector_in);
        goto free;
    }

    struct vector *const rv = ret.value.pointer;
    if (rv->field_count != vec_size) {
        VECTOR_FIELD_ERROR(
            executor, "Expected vector with %zu elements, received %zu",
            vec_size, rv->field_count
        );
        VECTOR_FIELD_FREE(executor, ret);
        VECTOR_FIELD_FREE(executor, vector_in);
        goto free;
    }

    for (mc_ind_t i = 0; i < vec_size; ++i) {
        struct vector_field const vector_field = vector_field_extract_type(
            executor, &rv->fields[i], VECTOR_FIELD_TYPE_DOUBLE
        );

        if (!vector_field.vtable) {
            VECTOR_FIELD_FREE(executor, vector_in);
            VECTOR_FIELD_FREE(executor, ret);
            goto free;
        }

        out->elems[out->numel++] = (float) vector_field.value.doub;
    }

    VECTOR_FIELD_FREE(executor, ret);
    VECTOR_FIELD_FREE(executor, vector_in);

    return MC_STATUS_SUCCESS;
free:
    mc_free(out->elems);
    out->numel = SIZE_MAX;
    return MC_STATUS_FAIL;
}

#pragma message("OPTIMIZATION, a lot of repeat points can be optimized")
static mc_status_t
_apply_map(
    struct timeline_execution_context *executor, struct _mapped_data *out,
    struct tetramesh *mesh, struct vector_field func, mc_count_t vec_size
)
{
    /* skip for uv */
    if (vec_size >= 3) {
        for (mc_ind_t i = 0; i < mesh->dot_count; ++i) {
            if (_map_single(executor, func, mesh->dots[i].pos, out, vec_size) !=
                MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }
        }

        for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
            if (_map_single(
                    executor, func, mesh->lins[i].a.pos, out, vec_size
                ) != MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }
            if (_map_single(
                    executor, func, mesh->lins[i].b.pos, out, vec_size
                ) != MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }
        }
    }

    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        if (_map_single(executor, func, mesh->tris[i].a.pos, out, vec_size) !=
            MC_STATUS_SUCCESS) {
            return MC_STATUS_FAIL;
        }
        if (_map_single(executor, func, mesh->tris[i].b.pos, out, vec_size) !=
            MC_STATUS_SUCCESS) {
            return MC_STATUS_FAIL;
        }
        if (_map_single(executor, func, mesh->tris[i].c.pos, out, vec_size) !=
            MC_STATUS_SUCCESS) {
            return MC_STATUS_FAIL;
        }
    }

    return MC_STATUS_SUCCESS;
}

void
lib_mc_mesh_uv_map(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_OP_MESH;
    LIBMC_FULL_CAST(func, 3, VECTOR_FIELD_TYPE_FUNCTION);

    struct _mapped_data data = { 0 };
    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        if (_apply_map(executor, &data, mesh.meshes[i], func, 2) !=
            MC_STATUS_SUCCESS) {
            VECTOR_FIELD_FREE(executor, out);
            mesh_subset_free(mesh);
            return;
        }
    }

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *const curr = mesh.meshes[i];
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        for (mc_ind_t j = 0; j < curr->tri_count; ++j) {
            dump->tris[j].a.uv = read_vec2(&data);
            dump->tris[j].b.uv = read_vec2(&data);
            dump->tris[j].c.uv = read_vec2(&data);
        }

        vector_plus(executor, out, &sub_out);
        dump->modded = dump->dirty_hash_cache = 1;
    }

    executor->return_register = out;
    mc_free(data.elems);
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_color_map(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_OP_MESH;
    LIBMC_FULL_CAST(func, 3, VECTOR_FIELD_TYPE_FUNCTION);

    struct _mapped_data data = { 0 };
    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        if (_apply_map(executor, &data, mesh.meshes[i], func, 4) !=
            MC_STATUS_SUCCESS) {
            VECTOR_FIELD_FREE(executor, out);
            mesh_subset_free(mesh);
            return;
        }
    }

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *const curr = mesh.meshes[i];
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        for (mc_ind_t j = 0; j < curr->dot_count; ++j) {
            dump->dots[j].col = read_vec4(&data);
        }

        for (mc_ind_t j = 0; j < curr->lin_count; ++j) {
            dump->lins[j].a.col = read_vec4(&data);
            dump->lins[j].b.col = read_vec4(&data);
        }

        for (mc_ind_t j = 0; j < curr->tri_count; ++j) {
            dump->tris[j].a.col = read_vec4(&data);
            dump->tris[j].b.col = read_vec4(&data);
            dump->tris[j].c.col = read_vec4(&data);
        }

        vector_plus(executor, out, &sub_out);
        dump->modded = dump->dirty_hash_cache = 1;
    }

    executor->return_register = out;
    mc_free(data.elems);
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_point_map(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_OP_MESH;
    LIBMC_FULL_CAST(func, 3, VECTOR_FIELD_TYPE_FUNCTION);

    struct _mapped_data data = { 0 };
    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        if (_apply_map(executor, &data, mesh.meshes[i], func, 3) !=
            MC_STATUS_SUCCESS) {
            VECTOR_FIELD_FREE(executor, out);
            mesh_subset_free(mesh);
            return;
        }
    }

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *const curr = mesh.meshes[i];
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        for (mc_ind_t j = 0; j < curr->dot_count; ++j) {
            dump->dots[j].pos = read_vec3(&data);
        }

        for (mc_ind_t j = 0; j < curr->lin_count; ++j) {
            dump->lins[j].a.pos = read_vec3(&data);
            dump->lins[j].b.pos = read_vec3(&data);
        }

        for (mc_ind_t j = 0; j < curr->tri_count; ++j) {
            dump->tris[j].a.pos = read_vec3(&data);
            dump->tris[j].b.pos = read_vec3(&data);
            dump->tris[j].c.pos = read_vec3(&data);
            struct vec3 norm = vec3_unit(vec3_cross(
                vec3_sub(dump->tris[j].b.pos, dump->tris[j].a.pos),
                vec3_sub(dump->tris[j].c.pos, dump->tris[j].a.pos)
            ));
            dump->tris[j].a.norm = norm;
            dump->tris[j].b.norm = norm;
            dump->tris[j].c.norm = norm;
        }

        vector_plus(executor, out, &sub_out);
        dump->modded = dump->dirty_hash_cache = 1;
    }

    mc_free(data.elems);
    mesh_subset_free(mesh);
    executor->return_register = out;
}

void
lib_mc_mesh_retagged(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_OP_MESH;
    LIBMC_FULL_CAST(func, 3, VECTOR_FIELD_TYPE_FUNCTION);

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        vector_plus(executor, out, &sub_out);

        struct vector_field arg = vector_init(executor);
        for (mc_ind_t j = 0; j < dump->tag_count; ++j) {
            struct vector_field sub = double_init(executor, dump->tags[j]);
            vector_plus(executor, arg, &sub);
        }
        function_call(executor, func, 1, &arg);

        struct vector_field const vec = vector_field_extract_type(
            executor, &executor->return_register, VECTOR_FIELD_TYPE_VECTOR
        );
        if (!vec.vtable) {
            VECTOR_FIELD_FREE(executor, out);
            VECTOR_FIELD_FREE(executor, arg);
            mesh_subset_free(mesh);
            return;
        }

        struct vector *vector = vec.value.pointer;
        dump->tag_count = vector->field_count;
        dump->tags =
            mc_reallocf(dump->tags, sizeof(mc_tag_t) * vector->field_count);

        for (mc_ind_t j = 0; j < vector->field_count; ++j) {
            struct vector_field const cast = vector_field_nocopy_extract_type(
                executor, vector->fields[j], VECTOR_FIELD_TYPE_DOUBLE
            );
            if (!cast.vtable) {
                VECTOR_FIELD_FREE(executor, executor->return_register);
                VECTOR_FIELD_FREE(executor, out);
                VECTOR_FIELD_FREE(executor, arg);
                mesh_subset_free(mesh);
                return;
            }

            dump->tags[j] = cast.value.doub;
        }
        dump->modded = dump->dirty_hash_cache = 1;

        VECTOR_FIELD_FREE(executor, arg);
        VECTOR_FIELD_FREE(executor, executor->return_register);
        executor->return_register = VECTOR_FIELD_NULL;
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_tag_map(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_OP_MESH;
    LIBMC_FULL_CAST(func, 3, VECTOR_FIELD_TYPE_FUNCTION);

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *const curr = mesh.meshes[i];

        struct vector_field arg[2];
        arg[0] = vector_init(executor);
        for (mc_ind_t q = 0; q < curr->tag_count; ++q) {
            struct vector_field aux = double_init(executor, curr->tags[q]);
            vector_plus(executor, arg[0], &aux);
        }
        arg[1] = mesh.sources[i];

        function_call(executor, func, 2, arg);

        if (!executor->return_register.vtable) {
            VECTOR_FIELD_FREE(executor, out);
            return;
        }

        vector_plus(executor, out, &executor->return_register);
        executor->return_register = VECTOR_FIELD_NULL;
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

static inline struct vec4
get_col(
    struct vec3 pos, mc_bool_t is_albedo, struct vec4 albedo, struct vec3 start,
    struct vec3 end, struct vec4 start_col, struct vec4 end_col
)
{
    if (is_albedo) {
        return albedo;
    }

    struct vec3 const raw_delta = vec3_sub(end, start);
    float norm = vec3_norm(raw_delta);
    if (norm < GEOMETRIC_EPSILON) {
        norm = 1;
    }
    struct vec3 const delta = vec3_unit(raw_delta);
    struct vec3 const comp = vec3_sub(pos, start);

    float t = vec3_dot(delta, comp) / norm;
    if (t < 0) {
        t = 0;
    }
    else if (t > 1) {
        t = 1;
    }

    return vec4_lerp(start_col, t, end_col);
}

void
lib_mc_mesh_recolored(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec4 albedo = { 0 }, start_col = { 0 }, end_col = { 0 };
    struct vec3 start_pos = { 0 }, end_pos = { 0 };

    LIBMC_FULL_CAST(fill_type, 9, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(color_ind, 3, VECTOR_FIELD_TYPE_DOUBLE);
    if (color_ind.value.doub == 0) {
        LIBMC_VEC4(albedo, 4);
    }
    else {
        LIBMC_VEC4(start_col, 5);
        LIBMC_VEC4(end_col, 6);
        LIBMC_VEC3(start_pos, 7);
        LIBMC_VEC3(end_pos, 8);
    }

    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct tetramesh *tag = mesh.meshes[i];
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        if (fill_type.value.doub == 0) {
            for (mc_ind_t j = 0; j < tag->dot_count; ++j) {
                dump->dots[j].col = get_col(
                    tag->dots[j].pos, color_ind.value.doub == 0, albedo,
                    start_pos, end_pos, start_col, end_col
                );
            }
        }
        else if (fill_type.value.doub == 1) {
            for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
                dump->lins[j].a.col = get_col(
                    tag->lins[j].a.pos, color_ind.value.doub == 0, albedo,
                    start_pos, end_pos, start_col, end_col
                );
                dump->lins[j].b.col = get_col(
                    tag->lins[j].b.pos, color_ind.value.doub == 0, albedo,
                    start_pos, end_pos, start_col, end_col
                );
            }
        }
        else if (fill_type.value.doub == 2) {
            for (mc_ind_t j = 0; j < tag->tri_count; ++j) {
                dump->tris[j].a.col = get_col(
                    tag->tris[j].a.pos, color_ind.value.doub == 0, albedo,
                    start_pos, end_pos, start_col, end_col
                );
                dump->tris[j].b.col = get_col(
                    tag->tris[j].b.pos, color_ind.value.doub == 0, albedo,
                    start_pos, end_pos, start_col, end_col
                );
                dump->tris[j].c.col = get_col(
                    tag->tris[j].c.pos, color_ind.value.doub == 0, albedo,
                    start_pos, end_pos, start_col, end_col
                );
            }
        }

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_retextured(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    char const *str = vector_field_str(executor, fields[3]);
    if (!str) {
        return;
    }

    char const *path = NULL;
    for (mc_ind_t i = 0; i < executor->media_count; ++i) {
        if (!strcmp(str, executor->media_cache[i].name) &&
            executor->media_cache[i].path) {
            path = executor->media_cache[i].path;
            break;
        }
    }

    if (!path) {
        VECTOR_FIELD_ERROR(executor, "Could not find an image named `%s`", str);
        executor->return_register = VECTOR_FIELD_NULL;
        mc_free((char *) str);
        return;
    }
    mc_free((char *) str);

    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        vector_plus(executor, out, &sub_out);

        dump->texture_handle = poll_texture(path);
        dump->modded = 1;
        dump->hash_cache = 1;
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_uprank(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        // guaranteed success
        struct vector_field copy = tetramesh_owned(executor, mesh.sources[i]);
        if (tetramesh_uprank(copy.value.pointer, 0) != MC_STATUS_SUCCESS) {
            VECTOR_FIELD_ERROR(executor, "Uprank failed");
            VECTOR_FIELD_FREE(executor, out);
            VECTOR_FIELD_FREE(executor, copy);
            executor->return_register = VECTOR_FIELD_NULL;
            mesh_subset_free(mesh);
            return;
        }

        vector_plus(executor, out, &copy);
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_downrank(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        // guaranteed success
        struct vector_field copy = tetramesh_owned(executor, mesh.sources[i]);
        tetramesh_downrank(executor, copy.value.pointer);
        vector_plus(executor, out, &copy);
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_subdivided(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(factor, 3, VECTOR_FIELD_TYPE_DOUBLE);
    if (factor.value.doub > MAX_SUBDIVISION) {
        VECTOR_FIELD_ERROR(executor, "Sub division factor exceeds maximum");
    }
    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        for (double j = 0; j < factor.value.doub; ++j) {
            tetramesh_tesselate(dump, dump->tri_count * 3);
        }

        dump->modded = 1;
        dump->hash_cache = 1;

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

static void
subdivide_lin(
    struct tetramesh *mesh, struct tetra_lin *dump, int32_t j, mc_count_t factor
)
{
    int32_t nbrs[4] = { j, mesh->lins[j].inverse, mesh->lins[j].antinorm,
                        mesh->lins[mesh->lins[j].inverse].antinorm };
    for (int n = 0; n < 4; ++n) {
        mc_count_t const k = (mc_count_t) nbrs[n];
        for (mc_ind_t q = 0; q < factor; ++q) {
            float const u = (float) q / factor;
            float const v = (float) (q + 1) / factor;

            copy_lin(&dump[k * factor + q], &mesh->lins[k], u, v);
            dump[k * factor + q].is_dominant_sibling =
                mesh->lins[k].is_dominant_sibling;
            dump[k * factor + q].inverse =
                (int32_t) (factor * (mc_count_t) mesh->lins[k].inverse +
                           factor - 1 - q);
            dump[k * factor + q].antinorm =
                (int32_t) (factor * (mc_count_t) mesh->lins[k].antinorm +
                           factor - 1 - q);

            if (q > 0) {
                dump[k * factor + q].prev = (int32_t) (k * factor + q - 1);
            }
            else if (mesh->lins[k].prev < 0) {
                dump[k * factor + q].prev = mesh->lins[k].prev;
                mesh->dots[-1 - mesh->lins[k].prev].inverse = -1 - (int32_t) k;
            }
            else {
                dump[k * factor + q].prev =
                    (int32_t) ((mc_count_t) mesh->lins[k].prev * factor +
                               factor - 1);
            }

            if (q < factor - 1) {
                dump[k * factor + q].next = (int32_t) (k * factor + q + 1);
            }
            else if (mesh->lins[k].next < 0) {
                dump[k * factor + q].next = mesh->lins[k].next;
                mesh->dots[-1 - mesh->lins[k].next].inverse = -1 - (int32_t) k;
            }
            else {
                dump[k * factor + q].next =
                    mesh->lins[k].next * (int32_t) factor;
            }
        }
    }
}

static void
line_tesselate(struct tetramesh *mesh, mc_count_t factor)
{
    if (factor < 1) {
        factor = 1;
    }

    mc_count_t const total_lines = (mc_count_t) (mesh->lin_count * factor);
    struct tetra_lin *lins =
        mc_malloc(sizeof(struct tetra_lin) * mesh->lin_count * factor);

    for (mc_ind_t q = 0; q < mesh->lin_count; ++q) {
        if (mesh->lins[q].is_dominant_sibling &&
            (int32_t) q < mesh->lins[q].antinorm) {
            subdivide_lin(mesh, lins, (int32_t) q, factor);
        }
    }

    mc_free(mesh->lins);
    mesh->lins = lins;
    mesh->lin_count = total_lines;
}

void
lib_mc_mesh_line_subdivided(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(factor, 3, VECTOR_FIELD_TYPE_DOUBLE);
    if (factor.value.doub > MAX_LINE_SUBDIVISION) {
        VECTOR_FIELD_ERROR(executor, "Sub division factor exceeds maximum");
    }
    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        if (dump->tri_count || !dump->lin_count) {
            VECTOR_FIELD_ERROR(
                executor, "Cannot line subdivide on mesh that has triangles"
            );
            executor->return_register = VECTOR_FIELD_NULL;
            VECTOR_FIELD_FREE(executor, out);
            mesh_subset_free(mesh);
            return;
        }

        line_tesselate(dump, (mc_count_t) factor.value.doub);

        dump->modded = 1;
        dump->hash_cache = 1;

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

#pragma message(                                                               \
    "TODO, norms are screwed up for the connecting region (also with revolve)" \
)
void
lib_mc_mesh_extruded(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 delta;
    LIBMC_VEC3(delta, 3);
    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const tag = sub_out.value.pointer;

        // create auxiliary copy
        // add connecting lines
        if (!tag->tri_count) {
            VECTOR_FIELD_ERROR(
                executor, "Can only extrude meshes that have triangles"
            );
            VECTOR_FIELD_FREE(executor, out);
            mesh_subset_free(mesh);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
            if (tag->lins[j].inverse >= 0) {
                VECTOR_FIELD_ERROR(
                    executor, "Can not extruded meshes that have loops "
                              "(without triangles). Try upranking!"
                );
                VECTOR_FIELD_FREE(executor, out);
                mesh_subset_free(mesh);
                executor->return_register = VECTOR_FIELD_NULL;
                return;
            }
        }

        // double tri count
        mc_count_t const old_count = tag->tri_count;
        tag->tri_count = (2 * tag->tri_count + 2 * tag->lin_count);
        tag->tris =
            mc_reallocf(tag->tris, tag->tri_count * sizeof(struct tetra_tri));
        for (mc_ind_t j = 0; j < old_count; ++j) {
            struct tetra_tri tri = tag->tris[j];

            if (tri.ab >= 0) {
                tri.ab += (int32_t) old_count;
            }
            else {
                tri.ab += (int32_t) tag->tri_count;
                tag->tris[j].ab +=
                    (int32_t) tag->tri_count - (int32_t) tag->lin_count;
            }

            if (tri.bc >= 0) {
                tri.bc += (int32_t) old_count;
            }
            else {
                tri.bc += (int32_t) tag->tri_count;
                tag->tris[j].bc +=
                    (int32_t) tag->tri_count - (int32_t) tag->lin_count;
            }

            if (tri.ca >= 0) {
                tri.ca += (int32_t) old_count;
            }
            else {
                tri.ca += (int32_t) tag->tri_count;
                tag->tris[j].ca +=
                    (int32_t) tag->tri_count - (int32_t) tag->lin_count;
            }

            tri.antinorm += (int32_t) old_count;
            tri.is_dominant_sibling = !tri.is_dominant_sibling;

            tri.a.pos = vec3_add(tri.a.pos, delta);
            tri.b.pos = vec3_add(tri.b.pos, delta);
            tri.c.pos = vec3_add(tri.c.pos, delta);

            tag->tris[j + old_count] = tri;
        }
        for (mc_ind_t j = 0; j < tag->lin_count; ++j) {
            struct tetra_lin const src = tag->lins[j];

            if (tag->tris[-1 - src.inverse].is_dominant_sibling) {
                struct tetra_lin const anti = tag->lins[src.antinorm];
                tag->tris[2 * old_count + tag->lin_count - j - 1] =
                    (struct tetra_tri){
                        .a = { src.b.pos, .col = src.b.col, .uv = { 0 },
                               .norm = src.norm },
                        .b = { src.a.pos, .col = src.a.col, .uv = { 0 },
                               .norm = src.norm },
                        .c = { vec3_add(src.a.pos, delta), .col = src.a.col,
                               .uv = { 0 }, .norm = src.norm },
                        .ab = -1 - src.inverse,
                        .bc = (int32_t) tag->tri_count - src.prev - 1,
                        .ca = (int32_t) (tag->tri_count - j) - 1,
                        .antinorm = (int32_t) (2 * old_count + tag->lin_count -
                                               (mc_count_t) src.antinorm - 1),
                        .is_dominant_sibling = 1,
                    };
                tag->tris[tag->tri_count - j - 1] = (struct tetra_tri){
                    .a = { src.b.pos, .col = src.b.col, .uv = { 0 },
                           .norm = src.norm },
                    .b = { vec3_add(src.a.pos, delta), .col = src.a.col,
                           .uv = { 0 }, .norm = src.norm },
                    .c = { vec3_add(src.b.pos, delta), .col = src.b.col,
                           .uv = { 0 }, .norm = src.norm },
                    .ab = (int32_t) (2 * old_count + tag->lin_count - j - 1),
                    .bc = (int32_t) (2 * old_count + tag->lin_count) -
                          src.next - 1,
                    .ca = -1 - src.inverse + (int32_t) (old_count),
                    .antinorm = (int32_t) tag->tri_count - src.antinorm - 1,
                    .is_dominant_sibling = 1,
                };
                tag->tris
                    [2 * old_count + tag->lin_count -
                     (mc_count_t) src.antinorm - 1] = (struct tetra_tri){
                    .a = { anti.b.pos, .col = anti.b.col, .uv = { 0 },
                           .norm = anti.norm },
                    .b = { anti.a.pos, .col = anti.a.col, .uv = { 0 },
                           .norm = anti.norm },
                    .c = { vec3_add(anti.b.pos, delta), .col = anti.b.col,
                           .uv = { 0 }, .norm = anti.norm },
                    .ab = -1 - anti.inverse,
                    .bc = (int32_t) tag->tri_count - src.antinorm - 1,
                    .ca = (int32_t) tag->tri_count - anti.next - 1,
                    .antinorm =
                        (int32_t) (2 * old_count + tag->lin_count - j) - 1,
                    .is_dominant_sibling = 0,
                };
                tag->tris[tag->tri_count - (mc_count_t) src.antinorm - 1] =
                    (struct tetra_tri){
                        .a = { vec3_add(anti.b.pos, delta), .col = anti.b.col,
                               .uv = { 0 }, .norm = anti.norm },
                        .b = { anti.a.pos, .col = anti.a.col, .uv = { 0 },
                               .norm = anti.norm },
                        .c = { vec3_add(anti.a.pos, delta), .col = anti.a.col,
                               .uv = { 0 }, .norm = anti.norm },
                        .ab = (int32_t) (2 * old_count + tag->lin_count) -
                              src.antinorm - 1,
                        .bc = (int32_t) (2 * old_count + tag->lin_count) -
                              anti.prev - 1,
                        .ca = -1 - anti.inverse + (int32_t) old_count,
                        .antinorm = (int32_t) tag->tri_count - src.antinorm - 1,
                        .is_dominant_sibling = 0,
                    };
            }
        }
        // there will be no lines at the end
        mc_free(tag->lins);
        tag->lin_count = 0;
        tag->lins = NULL;

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_revolved(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 rot_;
    LIBMC_VEC3(rot_, 3);

    float const alpha = vec3_norm(rot_);
    struct vec3 const rot = vec3_unit(rot_);
    LIBMC_OP_MESH;

    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const tag = sub_out.value.pointer;
        struct tetramesh *const src = mesh.meshes[i];

        if (!tag->lin_count || tag->tri_count) {
            VECTOR_FIELD_ERROR(
                executor, "Can only revolve meshes that are line meshes"
            );
            VECTOR_FIELD_FREE(executor, out);
            mesh_subset_free(mesh);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        for (mc_ind_t j = 0; j < tag->dot_count; ++j) {
            if (tag->dots[j].inverse >= 0) {
                VECTOR_FIELD_ERROR(
                    executor, "Can not revolve meshes that have single dots "
                              "(only available for lines)!"
                );
                VECTOR_FIELD_FREE(executor, out);
                mesh_subset_free(mesh);
                executor->return_register = VECTOR_FIELD_NULL;
                return;
            }
        }

        int32_t *line_map = malloc(sizeof(int32_t) * tag->lin_count);
        int32_t *dot_map = malloc(sizeof(int32_t) * tag->dot_count);
        int32_t *line_prev = malloc(sizeof(int32_t) * tag->lin_count);

        for (mc_ind_t j = 0, q = 0; j < tag->lin_count; ++j) {
            line_prev[j] = (int32_t) j;

            if (tag->lins[j].is_dominant_sibling) {
                line_map[j] = (int32_t) q++;
            }
        }

        for (mc_ind_t j = 0; j < tag->dot_count; ++j) {
            dot_map[j] = -1 - tag->dots[j].inverse;
        }

        // in x amount of steps, rotate slightly and connect
        // if alpha >= 2pi, then collapse back
        tag->lins = mc_reallocf(
            tag->lins,
            sizeof(struct tetra_lin) * mc_memory_upsize(tag->lin_count)
        );
        tag->tris = mc_reallocf(
            tag->tris,
            sizeof(struct tetra_lin) * mc_memory_upsize(tag->tri_count)
        );

        mc_count_t const base_lin = tag->lin_count;
        for (float t = REVOLVE_STEP_RATE, tp = 0;;) {
            if (t > alpha) {
                t = alpha;
            }

            mc_count_t const base = tag->tri_count;
            mc_count_t const sub_base_lin = tag->lin_count;

            for (mc_ind_t j = 0; j < tag->dot_count; ++j) {
                struct vec3 pos =
                    vec3_rotate_about_axis(tag->dots[j].pos, rot, tp);
                struct vec3 npos =
                    vec3_rotate_about_axis(tag->dots[j].pos, rot, t);

                int32_t next = -1, prev = dot_map[j];
                if (tag->lins[-1 - tag->dots[j].inverse].next >= 0) {
                    int32_t const aux = next;
                    next = prev;
                    prev = aux;
                    struct vec3 const vaux = pos;
                    pos = npos;
                    npos = vaux;
                    tag->lins[dot_map[j]].prev = (int32_t) tag->lin_count;
                }
                else {
                    tag->lins[dot_map[j]].next = (int32_t) tag->lin_count;
                }

                MC_MEM_RESERVE(tag->lins, tag->lin_count);
                tag->lins[tag->lin_count] = (struct tetra_lin){
                    .a = { pos, .col = tag->dots[j].col },
                    .b = { npos, .col = tag->dots[j].col },
                    .norm = tag->dots[j].norm,
                    .next = next,
                    .prev = prev,
                    .inverse = -1, /* filled in by triangles */
                    .antinorm = (int32_t) sub_base_lin + tag->dots[j].antinorm,
                    .is_dominant_sibling = 1,
                };

                dot_map[j] = (int32_t) tag->lin_count++;
            }

            for (mc_ind_t j = 0; j < base_lin; ++j) {
                if (!tag->lins[j].is_dominant_sibling) {
                    continue;
                }
                // create new position
                struct tetra_lin_vertex a = tag->lins[j].a;
                struct tetra_lin_vertex b = tag->lins[j].b;

                struct vec3 a_prime = vec3_rotate_about_axis(a.pos, rot, t);
                struct vec3 b_prime = vec3_rotate_about_axis(b.pos, rot, t);

                a.pos = vec3_rotate_about_axis(a.pos, rot, tp);
                b.pos = vec3_rotate_about_axis(b.pos, rot, tp);

                struct vec4 ap_col = a.col;
                struct vec4 bp_col = b.col;

                struct vec3 const norm = tag->lins[j].norm;
                struct vec3 const anti = vec3_mul_scalar(-1, norm);

                // join from previous
                int32_t prev = src->lins[j].prev;
                if (prev >= 0) {
                    prev = (int32_t) base + 2 * line_map[prev] + 1;
                }
                else {
                    prev = -(int32_t) sub_base_lin + prev;
                    tag->lins[-1 - prev].inverse =
                        -1 - (int32_t) tag->tri_count;
                }

                int32_t next = src->lins[j].next;
                if (next >= 0) {
                    next = (int32_t) base + 2 * line_map[next];
                }
                else {
                    next = -(int32_t) sub_base_lin + next;
                    tag->lins[-1 - next].inverse =
                        -2 - (int32_t) tag->tri_count;
                }

                if ((int32_t) j > tag->lins[j].inverse) {
                    int32_t const aux = next;
                    next = prev;
                    prev = aux;
                    struct vec3 const vaux = a_prime;
                    a_prime = b_prime;
                    b_prime = vaux;
                    struct vec4 const caux = ap_col;
                    ap_col = bp_col;
                    bp_col = caux;
                }

                MC_MEM_RESERVE(tag->tris, tag->tri_count);
                tag->tris[tag->tri_count] = (struct tetra_tri){
                    .a = { a.pos, .norm = norm, .uv = { 0 }, .col = a.col },
                    .b = { b.pos, .norm = norm, .uv = { 0 }, .col = b.col },
                    .c = { a_prime, .norm = norm, .uv = { 0 }, .col = ap_col },
                    .ab = line_prev[j],
                    .bc = (int32_t) tag->tri_count + 1,
                    .ca = prev,
                    .antinorm =
                        (int32_t) base + 2 * line_map[tag->lins[j].antinorm],
                    .is_dominant_sibling = 1,
                };
                tag->tri_count++;

                MC_MEM_RESERVE(tag->tris, tag->tri_count);
                tag->tris[tag->tri_count] = (struct tetra_tri){
                    .a = { a_prime, .norm = anti, .uv = { 0 }, .col = ap_col },
                    .b = { b.pos, .norm = anti, .uv = { 0 }, .col = b.col },
                    .c = { b_prime, .norm = anti, .uv = { 0 }, .col = bp_col },
                    .ab = (int32_t) tag->tri_count - 1,
                    .bc = next,
                    .ca = -1,
                    .antinorm = (int32_t) base +
                                2 * line_map[tag->lins[j].antinorm] + 1,
                    .is_dominant_sibling = 0,
                };
                line_prev[j] = (int32_t) tag->tri_count++;
            }

            if (t == alpha) {
                break;
            }
            tp = t;
            t += REVOLVE_STEP_RATE;
        }

        // join latest edge to the inverses
        for (mc_ind_t j = 0; j < base_lin; ++j) {
            if (!tag->lins[j].is_dominant_sibling) {
                struct tetra_lin_vertex const a = tag->lins[j].a;
                struct tetra_lin_vertex const b = tag->lins[j].b;

                tag->lins[j].a.pos = vec3_rotate_about_axis(a.pos, rot, alpha);
                tag->lins[j].b.pos = vec3_rotate_about_axis(b.pos, rot, alpha);
                tag->lins[j].inverse = line_prev[tag->lins[j].inverse];
                tag->lins[j].is_dominant_sibling = 1;

                if (tag->lins[j].prev < 0) {
                    tag->lins[j].prev = dot_map[-1 - tag->lins[j].prev];
                }
                if (tag->lins[j].next < 0) {
                    tag->lins[j].next = dot_map[-1 - tag->lins[j].next];
                }
            }
        }

        mc_free(line_map);
        mc_free(line_prev);
        mc_free(dot_map);
        mc_free(tag->dots);
        tag->dot_count = 0;
        tag->dots = NULL;
        tag->modded = tag->dirty_hash_cache = 1;

        for (mc_ind_t j = 0; j < tag->tri_count; ++j) {
            struct vec3 norm = vec3_unit(vec3_cross(
                vec3_sub(tag->tris[j].b.pos, tag->tris[j].a.pos),
                vec3_sub(tag->tris[j].c.pos, tag->tris[j].a.pos)
            ));
            tag->tris[j].a.norm = tag->tris[j].b.norm = tag->tris[j].c.norm =
                norm;
        }

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_glossy(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_OP_MESH;
    for (mc_ind_t i = 0; i < mesh.subset_count; ++i) {
        struct vector_field sub_out =
            tetramesh_owned(executor, mesh.sources[i]);
        struct tetramesh *const dump = sub_out.value.pointer;

        dump->uniform = GLOSSY_UNIFORM;

        dump->modded = 1;
        dump->dirty_hash_cache = 1;

        vector_plus(executor, out, &sub_out);
    }

    executor->return_register = out;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_centered(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 targ;
    LIBMC_VEC3(targ, 3);
    LIBMC_OP_MESH;

    struct vec3 const center = lib_mc_mesh_vec3_center(executor, mesh);
    struct vec3 const delta = vec3_sub(targ, center);

    shift_mesh(executor, out, delta, mesh);

    executor->return_register = out;
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_stack(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(buff, 4, VECTOR_FIELD_TYPE_DOUBLE);

    struct vec3 dir, adir, align_dir = { 0 };
    LIBMC_FULL_CAST(align_type, 2, VECTOR_FIELD_TYPE_DOUBLE);
    if (align_type.value.doub == 1) {
        LIBMC_VEC3(align_dir, 3);
        align_dir = vec3_unit(align_dir);
    }

    LIBMC_VEC3(dir, 1);
    dir = vec3_unit(dir);
    adir = vec3_mul_scalar(-1, dir);

    if (fabsf(vec3_dot(dir, align_dir)) > GEOMETRIC_EPSILON) {
        VECTOR_FIELD_ERROR(
            executor,
            "Expected align direction to be orthogonal to general direction"
        );
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    LIBMC_FULL_CAST(meshes, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const in = meshes.value.pointer;
    struct vector_field out = vector_init(executor);
    if (!in->field_count) {
        executor->return_register = out;
        return;
    }

    struct vec3 pivot = { 0 };
    float last_dir = 0;
    float last_align = 0;
    struct vector_field arg[3];
    arg[0] = double_init(executor, 0);
    fields = arg;
    for (mc_ind_t i = 0; i < in->field_count; ++i) {
        /* extraction assumptions assume an lvalue for mesh_shift */
        arg[1] = lvalue_init(executor, &in->fields[i]);

        LIBMC_SELECT_RETURN(sub_mesh, 0, VECTOR_FIELD_FREE(executor, out);
                            return);
        struct vec3 const sub_center =
            lib_mc_mesh_vec3_center(executor, sub_mesh);
        float const forward = mesh_direction(sub_mesh, dir);
        float const backward = -mesh_direction(sub_mesh, adir);

        float const align = mesh_direction(sub_mesh, align_dir);

        struct vec3 const diff = i == 0 ? VEC3_0 : vec3_sub(sub_center, pivot);
        struct vec3 const sub_tangent = vec3_proj_onto(diff, dir);
        struct vec3 const ortho = vec3_sub(sub_tangent, diff);
        struct vec3 const tangent =
            i == 0 ? VEC3_0 : vec3_mul_scalar(last_dir - backward, dir);
        struct vec3 const align_shift =
            i == 0 ? VEC3_0
                   : vec3_sub(
                         vec3_mul_scalar(last_align - align, align_dir),
                         vec3_proj_onto(ortho, align_dir)
                     );
        struct vec3 const shift =
            vec3_add(vec3_add(ortho, align_shift), tangent);

        struct vector_field sub = vector_init(executor);
        shift_mesh(executor, sub, shift, sub_mesh);

        vector_plus(executor, out, &sub);
        mesh_subset_free(sub_mesh);

        last_dir +=
            (float) buff.value.doub + (i == 0 ? forward : forward - backward);
        if (i == 0) {
            pivot = sub_center;
            last_align = align;
        }
    }

    executor->return_register = out;
}

void
lib_mc_mesh_matched_edge(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 dir;
    LIBMC_VEC3(dir, 4);
    LIBMC_OP_MESH;

    struct vector_field ref[3];
    ref[0] = double_init(executor, 0);
    ref[1] = fields[3];
    fields = ref;
    LIBMC_SELECT_RETURN(ref_set, 0, VECTOR_FIELD_FREE(executor, out);
                        mesh_subset_free(mesh); return;);

    float const ref_dir = mesh_direction(ref_set, dir);
    float const our_dir = mesh_direction(mesh, dir);

    struct vec3 const delta = vec3_mul_scalar(ref_dir - our_dir, dir);
    shift_mesh(executor, out, delta, mesh);

    executor->return_register = out;
    mesh_subset_free(ref_set);
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_next_to(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    float const buff = 0.1f;

    struct vec3 dir;
    LIBMC_VEC3(dir, 4);
    dir = vec3_unit(dir);
    LIBMC_OP_MESH;

    struct vec3 const center = lib_mc_mesh_vec3_center(executor, mesh);

    struct vector_field ref[3];
    ref[0] = double_init(executor, 0);
    ref[1] = fields[3];
    fields = ref;
    LIBMC_SELECT_RETURN(ref_set, 0, VECTOR_FIELD_FREE(executor, out);
                        mesh_subset_free(mesh); return;);

    struct vec3 const ref_center = lib_mc_mesh_vec3_center(executor, ref_set);

    float const ref_dir = mesh_direction(ref_set, dir);
    float const our_dir = -mesh_direction(mesh, vec3_mul_scalar(-1, dir));

    struct vec3 const tangent = vec3_mul_scalar(buff + ref_dir - our_dir, dir);
    struct vec3 const raw_diff = vec3_sub(ref_center, center);
    struct vec3 const ortho = vec3_sub(vec3_proj_onto(raw_diff, dir), raw_diff);
    shift_mesh(executor, out, vec3_sub(tangent, ortho), mesh);

    executor->return_register = out;
    mesh_subset_free(ref_set);
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_to_side(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 side;
    LIBMC_VEC3(side, 3);
    LIBMC_OP_MESH;

    /* probably dont hardcode this in future */
    float const s_right = 3.8f;
    float const s_left = -s_right;
    float const s_up = 2.05f;
    float const s_down = -s_up;

    float const right = mesh_direction(mesh, (struct vec3){ +1, 0, 0 });
    float const left = -mesh_direction(mesh, (struct vec3){ -1, 0, 0 });
    float const down = -mesh_direction(mesh, (struct vec3){ 0, -1, 0 });
    float const up = mesh_direction(mesh, (struct vec3){ 0, +1, 0 });

    // can be optimized
    struct vec3 const center = lib_mc_mesh_vec3_center(executor, mesh);

    struct vec3 delta = { 0 };
    if (side.x < 0) {
        delta.x = s_left - left;
    }
    else if (side.x > 0) {
        delta.x = s_right - right;
    }
    else {
        delta.x = -center.x;
    }

    if (side.y < 0) {
        delta.y = s_down - down;
    }
    else if (side.y > 0) {
        delta.y = s_up - up;
    }
    else {
        delta.y = -center.y;
    }

    delta.z = -center.z;

    shift_mesh(executor, out, delta, mesh);

    executor->return_register = out;
    mesh_subset_free(mesh);
}

/* width of each column, height of each row */
static void
grid_like(
    struct timeline_execution_context *executor, struct vector_field *fields,
    struct vector_field out, float **widths, float **heights,
    mc_count_t *out_rows, mc_count_t *out_cols
)
{
    float const buff = 0.2f;
    LIBMC_FULL_CAST(mesh_array, 0, VECTOR_FIELD_TYPE_VECTOR);
    struct vector *const grid = mesh_array.value.pointer;

    mc_count_t rows = grid->field_count, cols = 0;
    *heights = mc_malloc(sizeof(float) * rows);

    mc_count_t *row_elems = mc_calloc(rows, sizeof(mc_count_t));
    struct mesh_tag_subset **subsets =
        mc_calloc(rows, sizeof(struct mesh_tag_subset *));

    for (mc_ind_t i = 0; i < grid->field_count; ++i) {
        fields = grid->fields;
        LIBMC_FULL_CAST_RETURN(
            mesh_row, i, VECTOR_FIELD_TYPE_VECTOR, goto free
        );

        struct vector *const row = mesh_row.value.pointer;
        row_elems[i] = row->field_count;

        struct vector_field arg[3];
        arg[0] = double_init(executor, 0);

        for (mc_ind_t j = cols; j < row->field_count; ++j) {
            MC_MEM_RESERVE(*widths, cols);
            (*widths)[j] = 0;
            cols++;
        }

        subsets[i] =
            mc_malloc(sizeof(struct mesh_tag_subset) * row->field_count);
        (*heights)[i] = buff;

        for (mc_ind_t j = 0; j < row->field_count; ++j) {
            arg[1] = lvalue_init(executor, &row->fields[j]);
            fields = arg;
            LIBMC_SELECT_RETURN(mesh, 0, goto free);

            subsets[i][j] = mesh;

            float const left = mesh_direction(mesh, (struct vec3){ -1, 0, 0 });
            float const right = mesh_direction(mesh, (struct vec3){ 1, 0, 0 });
            float const up = mesh_direction(mesh, (struct vec3){ 0, 1, 0 });
            float const down = mesh_direction(mesh, (struct vec3){ 0, 1, 0 });

            float const w = (right + left);
            float const h = (up + down);
            if ((*widths)[j] < w + buff) {
                (*widths)[j] = w + buff;
            }
            if ((*heights)[i] < h + buff) {
                (*heights)[i] = h + buff;
            }
        }
    }

    /* adjust positions of all meshes */
    float y = 0;
    for (mc_ind_t r = 0; r < rows; ++r) {
        float x = 0;
        for (mc_ind_t c = 0; c < row_elems[r]; ++c) {
            struct vec3 const center = { x + (*widths)[c] / 2,
                                         y - (*heights)[r] / 2, 0 };
            struct vec3 const ref =
                lib_mc_mesh_vec3_center(executor, subsets[r][c]);
            shift_mesh(executor, out, vec3_sub(center, ref), subsets[r][c]);
            x += (*widths)[c];
        }
        y -= (*heights)[r];
    }

    executor->return_register = out;

    for (mc_ind_t i = 0; i < rows; ++i) {
        for (mc_ind_t j = 0; j < row_elems[i]; ++j) {
            mesh_subset_free(subsets[i][j]);
        }
        mc_free(subsets[i]);
    }
    mc_free(subsets);
    mc_free(row_elems);

    *out_rows = rows;
    *out_cols = cols;

    return;
free:
    VECTOR_FIELD_FREE(executor, out);

    for (mc_ind_t i = 0; i < rows; ++i) {
        for (mc_ind_t j = 0; j < row_elems[i]; ++j) {
            mesh_subset_free(subsets[i][j]);
        }
        mc_free(subsets[i]);
    }
    mc_free(subsets);
    mc_free(*widths);
    mc_free(*heights);
    mc_free(row_elems);
    *widths = NULL;
    *heights = NULL;

    executor->return_register = VECTOR_FIELD_NULL;
}

void
lib_mc_mesh_grid(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vector_field out = vector_init(executor);
    float *widths = NULL, *heights = NULL;
    mc_count_t out_rows, out_cols;
    grid_like(executor, fields, out, &widths, &heights, &out_rows, &out_cols);
    mc_free(widths);
    mc_free(heights);
}

void
lib_mc_mesh_table(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vector_field out = vector_init(executor);
    float *widths = NULL, *heights = NULL;
    mc_count_t out_rows, out_cols;
    grid_like(executor, fields, out, &widths, &heights, &out_rows, &out_cols);
    if (!executor->return_register.vtable) {
        return;
    }

    struct vector_field mesh_field = tetramesh_init(executor);

    struct tetramesh *const mesh = mesh_field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    float total_width = 0, total_height = 0;
    for (mc_ind_t i = 0; i < out_cols; ++i) {
        total_width += widths[i];
    }
    for (mc_ind_t i = 0; i < out_rows; ++i) {
        total_height += heights[i];
    }

    float y = 0;
    for (mc_ind_t r = 0; r <= out_rows; ++r) {
        tetramesh_line(
            mesh, (struct vec3){ 0, y, 0 }, (struct vec3){ total_width, y, 0 },
            (struct vec3){ 0, 0, 1 }
        );
        tetramesh_line_close(mesh);

        if (r < out_rows) {
            y -= heights[r];
        }
    }

    float x = 0;
    for (mc_ind_t c = 0; c <= out_cols; ++c) {
        tetramesh_line(
            mesh, (struct vec3){ x, 0, 0 },
            (struct vec3){ x, -total_height, 0 }, (struct vec3){ 0, 0, 1 }
        );
        tetramesh_line_close(mesh);

        if (c < out_rows) {
            x += widths[c];
        }
    }

    mc_free(widths);
    mc_free(heights);
    vector_plus(executor, out, &mesh_field);

    if (libmc_tag_and_color1(executor, mesh, &fields[1]) != MC_STATUS_SUCCESS) {
        VECTOR_FIELD_FREE(executor, out);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
}
