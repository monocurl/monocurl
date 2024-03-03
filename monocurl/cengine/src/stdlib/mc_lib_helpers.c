//
//  mc_lib_helpers.c
//  Monocurl
//
//  Created by Manu Bhat on 2/19/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include "mc_lib_helpers.h"
#include "mc_meshes.h"

mc_bool_t
libmc_verify_mesh_tree(
    struct timeline_execution_context *executor, struct vector_field curr
)
{
    struct vector_field const gen = vector_field_nocopy_extract_type_message(
        executor, curr, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_MESH,
        "Invalid mesh tree node. Received %s expected %s"
    );
    if (!gen.vtable) {
        return 0;
    }

    if (gen.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vec = gen.value.pointer;
        for (mc_ind_t i = 0; i < vec->field_count; ++i) {
            if (!libmc_verify_mesh_tree(executor, vec->fields[i])) {
                return 0;
            }
        }
        return 1;
    }
    else {
        return 1; /* a mesh */
    }

    return 0;
}

mc_status_t
libmc_tag(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field tag
)
{
    struct vector_field const element =
        vector_field_nocopy_extract_type_message(
            executor, tag, VECTOR_FIELD_TYPE_VECTOR,
            "Could not cast tag to expected type. Received %s expected %s"
        );

    if (element.vtable) {
        struct vector *const vector = element.value.pointer;
        mesh->tag_count = vector->field_count;
        mesh->tags = mc_malloc(sizeof(mc_tag_t) * mesh->tag_count);

        for (mc_ind_t i = 0; i < mesh->tag_count; ++i) {
            struct vector_field const curr = vector_field_nocopy_extract_type(
                executor, vector->fields[i], VECTOR_FIELD_TYPE_DOUBLE
            );
            if (!curr.vtable) {
                tetramesh_unref(mesh);
                return MC_STATUS_FAIL;
            }

            mesh->tags[i] = curr.value.doub;
        }
    }
    else {
        VECTOR_FIELD_ERROR(executor, "Illegal tag!");
        executor->return_register = VECTOR_FIELD_NULL;
        tetramesh_unref(mesh);
        return MC_STATUS_FAIL;
    }

    return MC_STATUS_SUCCESS;
}

/* frees mesh if error */
/* dimension is highest order dimension */

// tag, [color] {[default] {}, [colored] {dot}}
mc_status_t
libmc_tag_and_color0(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
)
{
    if (libmc_tag(executor, mesh, fields[0]) != MC_STATUS_SUCCESS) {
        return MC_STATUS_FAIL;
    }
    LIBMC_FULL_CAST_RETURN(ind, 1, VECTOR_FIELD_TYPE_DOUBLE,
                           tetramesh_unref(mesh);
                           return MC_STATUS_FAIL);

    if (ind.value.doub >= 1) {
        struct vec4 dot;
        LIBMC_VEC4_RETURN(dot, 2, tetramesh_unref(mesh); return MC_STATUS_FAIL);
        for (mc_ind_t i = 0; i < mesh->dot_count; ++i) {
            mesh->dots[i].col = dot;
        }
    }

    return MC_STATUS_SUCCESS;
}

// tag, [color] {[default] {}, [stroke] {stroke}, [dotted] {stroke, dot}}
mc_status_t
libmc_tag_and_color1(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
)
{
    if (libmc_tag(executor, mesh, fields[0]) != MC_STATUS_SUCCESS) {
        return MC_STATUS_FAIL;
    }
    LIBMC_FULL_CAST_RETURN(ind, 1, VECTOR_FIELD_TYPE_DOUBLE,
                           tetramesh_unref(mesh);
                           return MC_STATUS_FAIL);

    if (ind.value.doub >= 1) {
        struct vec4 stroke;
        LIBMC_VEC4_RETURN(stroke, 2, tetramesh_unref(mesh);
                          return MC_STATUS_FAIL);
        for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
            mesh->lins[i].a.col = mesh->lins[i].b.col = stroke;
        }
    }

    if (ind.value.doub >= 2) {
        struct vec4 dot;
        LIBMC_VEC4_RETURN(dot, 3, tetramesh_unref(mesh); return MC_STATUS_FAIL);
        for (mc_ind_t i = 0; i < mesh->dot_count; ++i) {
            mesh->dots[i].col = dot;
        }
    }
    else {
        mesh->uniform.dot_radius = 0;
    }

    /* hide interior stroke to avoid doubling*/
    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        if (!mesh->lins[i].is_dominant_sibling) {
            mesh->lins[i].a.col.w = mesh->lins[i].b.col.w = 0;
        }
    }

    return MC_STATUS_SUCCESS;
}

/* tag, [color] {[default] {}, [stroke] {stroke}, [solid] {stroke, fill}} */
mc_status_t
libmc_tag_and_color2(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
)
{
    if (libmc_tag(executor, mesh, fields[0]) != MC_STATUS_SUCCESS) {
        return MC_STATUS_FAIL;
    }
    LIBMC_FULL_CAST_RETURN(ind, 1, VECTOR_FIELD_TYPE_DOUBLE,
                           tetramesh_unref(mesh);
                           return MC_STATUS_FAIL);

    if (ind.value.doub >= 1) {
        struct vec4 stroke;
        LIBMC_VEC4_RETURN(stroke, 2, tetramesh_unref(mesh);
                          return MC_STATUS_FAIL);
        for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
            mesh->lins[i].a.col = mesh->lins[i].b.col = stroke;
        }
    }

    if (ind.value.doub >= 2) {
        if (tetramesh_uprank(mesh, 0)) {
            VECTOR_FIELD_ERROR(executor, "Error upranking");
            tetramesh_unref(mesh);
            return MC_STATUS_FAIL;
        }

        struct vec4 fill;
        LIBMC_VEC4_RETURN(fill, 3, tetramesh_unref(mesh);
                          return MC_STATUS_FAIL);
        for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
            mesh->tris[i].a.col = mesh->tris[i].b.col = mesh->tris[i].c.col =
                fill;
        }
    }

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        if (!mesh->lins[i].is_dominant_sibling) {
            mesh->lins[i].a.col.w = mesh->lins[i].b.col.w = 0;
        }
    }

    return MC_STATUS_SUCCESS;
}

mc_status_t
libmc_tag_and_color2_forceuprank(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
)
{
    if (libmc_tag(executor, mesh, fields[0]) != MC_STATUS_SUCCESS) {
        return MC_STATUS_FAIL;
    }
    LIBMC_FULL_CAST_RETURN(ind, 1, VECTOR_FIELD_TYPE_DOUBLE,
                           tetramesh_unref(mesh);
                           return MC_STATUS_FAIL);

    if (tetramesh_uprank(mesh, 0) != MC_STATUS_SUCCESS) {
        VECTOR_FIELD_ERROR(executor, "Error upranking");
        tetramesh_unref(mesh);
        return MC_STATUS_FAIL;
    }

    if (ind.value.doub >= 1) {
        struct vec4 stroke;
        LIBMC_VEC4_RETURN(stroke, 2, tetramesh_unref(mesh);
                          return MC_STATUS_FAIL);
        for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
            if (!mesh->lins[i].is_dominant_sibling) {
                mesh->lins[i].a.col.w = mesh->lins[i].b.col.w = 0;
            }
            else {
                mesh->lins[i].a.col = mesh->lins[i].b.col = stroke;
            }
        }
    }
    else {
        /* Forceup rank typically only wants fill, no stroke */
        mesh->uniform.stroke_radius = 0;
    }

    if (ind.value.doub >= 2) {
        struct vec4 fill;
        LIBMC_VEC4_RETURN(fill, 3, tetramesh_unref(mesh);
                          return MC_STATUS_FAIL);
        for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
            mesh->tris[i].a.col = mesh->tris[i].b.col = mesh->tris[i].c.col =
                fill;
        }
    }
    else {
        for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
            mesh->tris[i].a.col = mesh->tris[i].b.col = mesh->tris[i].c.col =
                VEC4_1;
        }
    }

    return MC_STATUS_SUCCESS;
}

// [color] {[main] {tag}, [color] {tag, surface}}
mc_status_t
libmc_tag_and_color3(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field const *fields
)
{
    if (libmc_tag(executor, mesh, fields[0]) != MC_STATUS_SUCCESS) {
        return MC_STATUS_FAIL;
    }
    LIBMC_FULL_CAST_RETURN(ind, 1, VECTOR_FIELD_TYPE_DOUBLE,
                           tetramesh_unref(mesh);
                           return MC_STATUS_FAIL);

    if (ind.value.doub >= 1) {
        struct vec4 fill;
        LIBMC_VEC4_RETURN(fill, 2, tetramesh_unref(mesh);
                          return MC_STATUS_FAIL);
        for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
            mesh->tris[i].a.col = mesh->tris[i].b.col = mesh->tris[i].c.col =
                fill;
        }
    }

    if (ind.value.doub >= 2) {
        if (tetramesh_uprank(mesh, 0) != MC_STATUS_SUCCESS) {
            tetramesh_unref(mesh);
            return MC_STATUS_FAIL;
        }
    }

    return MC_STATUS_SUCCESS;
}
