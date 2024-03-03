//
//  mesh_geometry.c
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <float.h>
#define _USE_MATH_DEFINES
#include <math.h>
#include <stdlib.h>

#include "geo.h"
#include "mesh_geometry.h"
#include "mesh_util.h"
#include "vector.h"

#define MAX_POLYGON_VERTEX_COUNT 1024
#define MAX_SPHERE_VERTEX_DEPTH 7
#define DEFAULT_CIRCLE_VERTEX_COUNT 64
#define DEFAULT_SPHERE_VERTEX_DEPTH 4
#define DEFAULT_SPHERE_VERTEX_SEED 8
#define DEFAULT_LINE_BUFFER 0.0
#define BEZIER_SAMPLE 40

#define MAX_TIP_R 0.0425f
#define MIN_TIP_R_TO_LENGTH 0.3f
#define LINE_R_OVER_TIP_R 0.275f

// func Dot([config] {[main] {point}, [parameterized] {point, normal}},
// [coloring] {[main] {tag}, [colored] {tag, color}}) = native
// not_implemented_yet(0)
void
lib_mc_dot_mesh(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 point;
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_VEC3(point, 1);

    struct vec3 normal;
    if (config_ind.value.doub == 1) {
        LIBMC_VEC3(normal, 2);
        normal = vec3_unit(normal);
        LIBMC_NONNULLVEC3(normal);
    }
    else {
        normal = (struct vec3){ 0, 0, 1 };
    }

    struct vec3 const antinorm = vec3_mul_scalar(-1, normal);

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;
    mesh->dot_count = 4;
    mesh->dots = mc_malloc(sizeof(struct tetra_dot) * 4);

    mesh->dots[0] = (struct tetra_dot){
        .pos = point,
        .col = VEC4_1,
        .norm = normal,
        .inverse = 1,
        .antinorm = 2,
        .is_dominant_sibling = 1,
    };
    mesh->dots[1] = (struct tetra_dot){
        .pos = point,
        .col = VEC4_1,
        .norm = normal,
        .inverse = 0,
        .antinorm = 3,
        .is_dominant_sibling = 0,
    };
    mesh->dots[2] = (struct tetra_dot){
        .pos = point,
        .col = VEC4_1,
        .norm = antinorm,
        .inverse = 3,
        .antinorm = 0,
        .is_dominant_sibling = 1,
    };
    mesh->dots[3] = (struct tetra_dot){
        .pos = point,
        .col = VEC4_1,
        .norm = antinorm,
        .inverse = 2,
        .antinorm = 1,
        .is_dominant_sibling = 0,
    };

    if (libmc_tag_and_color0(executor, mesh, &fields[3]) != 0) {
        return;
    }

    executor->return_register = field;
}

// func Circle([config] {[main] {center, radius}, [parameterized] {center,
// radius, samples, normal}}, [color] {[main] {tag}, [stroke] {tag, stroke},
// [solid] {tag, stroke, fill}}) = native circle(config, center, radius,
// samples, normal, color, tag, stroke, fill) func RegularPolygon([config]
// {[main] {center, n, circumradius}, [parameterized] {center, n, circumradius,
// normal}}, [color] {[main] {tag}, [stroke] {tag, stroke}, [solid] {tag,
// stroke, fill}}) = native not_implemented_yet(0)
void
lib_mc_circle(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);

    struct vector_field samples = fields[3];
    if (config_ind.value.doub == 0) {
        samples = double_init(executor, DEFAULT_CIRCLE_VERTEX_COUNT);
    }

    fields[3] = fields[2];
    fields[2] = samples;

    lib_mc_regular_polygon(executor, caller, fc, fields);
}

void
lib_mc_annulus(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 center;
    LIBMC_VEC3(center, 1);
    LIBMC_FULL_CAST(inner, 2, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(outer, 3, VECTOR_FIELD_TYPE_DOUBLE);
    if (inner.value.doub <= 0 || outer.value.doub <= 0 ||
        inner.value.doub >= outer.value.doub) {
        VECTOR_FIELD_ERROR(executor, "Invalid radii");
        return;
    }

    mc_count_t samples = DEFAULT_CIRCLE_VERTEX_COUNT;
    struct vec3 normal = { 0, 0, 1 };
    //    if (config_ind.value.doub == 1) {
    //        LIBMC_VEC3(normal, 4);
    //        normal = vec3_unit(normal);
    //        LIBMC_NONNULLVEC3(normal);
    //    }
    //    else {
    //        normal = (struct vec3){ 0, 0, 1 };
    //    }

    struct plane3 const plane = vec3_plane_basis(normal);

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    for (mc_ind_t i = 0; i < samples; ++i) {
        float cos = (float) outer.value.doub *
                    cosf((i + 1) * 2 * (float) MC_PI / samples);
        float sin = (float) outer.value.doub *
                    sinf((i + 1) * 2 * (float) MC_PI / samples);
        if (i == samples - 1) {
            cos = (float) outer.value.doub;
            sin = 0;
        }

        struct vec3 const curr = (struct vec3){
            cos * plane.a.x + sin * plane.b.x + center.x,
            cos * plane.a.y + sin * plane.b.y + center.y,
            cos * plane.a.z + sin * plane.b.z + center.z,
        };

        if (!i) {
            struct vec3 const org = {
                (float) outer.value.doub * plane.a.x + center.x,
                (float) outer.value.doub * plane.a.y + center.y,
                (float) outer.value.doub * plane.a.z + center.z,
            };

            tetramesh_line(mesh, org, curr, normal);
        }
        else {
            tetramesh_line_to(mesh, curr);
        }
    }
    tetramesh_line_close(mesh);

    for (mc_ind_t i = 0; i < samples; ++i) {
        float cos = (float) inner.value.doub *
                    cosf((i + 1) * 2 * (float) MC_PI / samples);
        float sin = -(float) inner.value.doub *
                    sinf((i + 1) * 2 * (float) MC_PI / samples);
        if (i == samples - 1) {
            cos = (float) inner.value.doub;
            sin = 0;
        }

        struct vec3 const curr = (struct vec3){
            cos * plane.a.x + sin * plane.b.x + center.x,
            cos * plane.a.y + sin * plane.b.y + center.y,
            cos * plane.a.z + sin * plane.b.z + center.z,
        };

        if (!i) {
            struct vec3 const org = {
                (float) inner.value.doub * plane.a.x + center.x,
                (float) inner.value.doub * plane.a.y + center.y,
                (float) inner.value.doub * plane.a.z + center.z,
            };

            tetramesh_line(mesh, org, curr, normal);
        }
        else {
            tetramesh_line_to(mesh, curr);
        }
    }

    tetramesh_line_close(mesh);

    if (libmc_tag_and_color2(executor, mesh, &fields[4]) != 0) {
        return;
    }

    executor->return_register = field;
}

void
lib_mc_general_rect(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields, mc_bool_t force_up_rank
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 center;
    LIBMC_VEC3(center, 1);
    LIBMC_FULL_CAST(width, 2, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(height, 3, VECTOR_FIELD_TYPE_DOUBLE);

    struct vec3 normal;
    if (config_ind.value.doub == 1) {
        LIBMC_VEC3(normal, 4);
        normal = vec3_unit(normal);
        LIBMC_NONNULLVEC3(normal);
    }
    else {
        normal = (struct vec3){ 0, 0, 1 };
    }

    struct plane3 const plane = vec3_plane_basis(normal);

    float const xr = (float) width.value.doub / 2;
    float const yr = (float) height.value.doub / 2;

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    struct vec3 const a = { center.x + xr * plane.a.x + yr * plane.b.x,
                            center.y + xr * plane.a.y + yr * plane.b.y,
                            center.z + xr * plane.a.z + yr * plane.b.z };

    struct vec3 const b = {
        center.x - xr * plane.a.x + yr * plane.b.x,
        center.y - xr * plane.a.y + yr * plane.b.y,
        center.z - xr * plane.a.z + yr * plane.b.z,
    };

    struct vec3 const c = {
        center.x - xr * plane.a.x - yr * plane.b.x,
        center.y - xr * plane.a.y - yr * plane.b.y,
        center.z - xr * plane.a.z - yr * plane.b.z,
    };

    struct vec3 const d = {
        center.x + xr * plane.a.x - yr * plane.b.x,
        center.y + xr * plane.a.y - yr * plane.b.y,
        center.z + xr * plane.a.z - yr * plane.b.z,
    };

    tetramesh_line(mesh, a, b, normal);
    tetramesh_line_to(mesh, c);
    tetramesh_line_to(mesh, d);
    tetramesh_line_to(mesh, a);
    tetramesh_line_close(mesh);

    if (force_up_rank) {
        for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
            mesh->lins[i].a.col = mesh->lins[i].b.col = VEC4_0;
        }

        if (libmc_tag_and_color2_forceuprank(executor, mesh, &fields[5]) != 0) {
            return;
        }
    }
    else if (libmc_tag_and_color2(executor, mesh, &fields[5]) != 0) {
        return;
    }

    /* proper uv coordinates */
    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
#define ITERATE(triangle)                                                      \
    do {                                                                       \
        if (vec3_equals(mesh->tris[i].triangle.pos, a))                        \
            mesh->tris[i].triangle.uv = (struct vec2){ 1, 0 };                 \
        else if (vec3_equals(mesh->tris[i].triangle.pos, b))                   \
            mesh->tris[i].triangle.uv = (struct vec2){ 0, 0 };                 \
        else if (vec3_equals(mesh->tris[i].triangle.pos, c))                   \
            mesh->tris[i].triangle.uv = (struct vec2){ 0, 1 };                 \
        else                                                                   \
            mesh->tris[i].triangle.uv = (struct vec2){ 1, 1 };                 \
    } while (0)

        ITERATE(a);
        ITERATE(b);
        ITERATE(c);

#undef ITERATE
    }

    executor->return_register = field;
}

// func Rect([config] {[main] {center, width, height}, [parameterized] {center,
// width, height, normal}}, [color] {[main] {tag}, [stroke] {tag, stroke},
// [solid] {tag, stroke, fill}}) = native not_implemented_yet(0)
void
lib_mc_rect(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    lib_mc_general_rect(executor, caller, fc, fields, 0);
}

// func Arrow([config] {[main] {tail, head}, [parameterized] {tail, head,
// normal, path_arc}}, [color] {[main] {tag}, [stroke] {tag, stroke}, [solid]
// {tag, stroke, fill}}) = native not_implemented_yet(0) func
// RegularPolygon([config] {[main] {center, n, circumradius}, [parameterized]
// {center, n, circumradius, normal}}, [color] {[main] {tag}, [stroke] {tag,
// stroke}, [solid] {tag, stroke, fill}}) = native not_implemented_yet(0)
void
lib_mc_regular_polygon(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 center;
    LIBMC_VEC3(center, 1);
    LIBMC_FULL_CAST(samples, 2, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(radius, 3, VECTOR_FIELD_TYPE_DOUBLE);

    mc_count_t sample_count = (mc_count_t) samples.value.doub;
    if (sample_count <= 2 || sample_count > MAX_POLYGON_VERTEX_COUNT) {
        VECTOR_FIELD_ERROR(
            executor,
            "Invalid sample count. Received `%zu`, expected an amount between "
            "3 and %d",
            sample_count, MAX_POLYGON_VERTEX_COUNT
        );
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vec3 normal;
    if (config_ind.value.doub == 1) {
        LIBMC_VEC3(normal, 4);
        normal = vec3_unit(normal);
        LIBMC_NONNULLVEC3(normal);
    }
    else {
        normal = (struct vec3){ 0, 0, 1 };
    }

    struct plane3 const plane = vec3_plane_basis(normal);

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    for (mc_ind_t i = 0; i < sample_count; ++i) {
        float const cos = (float) radius.value.doub *
                          cosf((i + 1) * 2 * (float) MC_PI / sample_count);
        float const sin = (float) radius.value.doub *
                          sinf((i + 1) * 2 * (float) MC_PI / sample_count);

        struct vec3 const curr = (struct vec3){
            cos * plane.a.x + sin * plane.b.x + center.x,
            cos * plane.a.y + sin * plane.b.y + center.y,
            cos * plane.a.z + sin * plane.b.z + center.z,
        };

        if (!i) {
            struct vec3 const org = {
                (float) radius.value.doub * plane.a.x + center.x,
                (float) radius.value.doub * plane.a.y + center.y,
                (float) radius.value.doub * plane.a.z + center.z,
            };

            tetramesh_line(mesh, org, curr, normal);
        }
        else {
            tetramesh_line_to(mesh, curr);
        }
    }

    tetramesh_line_close(mesh);

    if (libmc_tag_and_color2(executor, mesh, &fields[5]) != 0) {
        return;
    }

    executor->return_register = field;
}

// func Polygon(vertices, [color] {[main] {tag}, [stroke] {tag, stroke}, [solid]
// {tag, stroke, fill}}) = native not_implemented_yet(0)
void
lib_mc_polygon(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(vertices, 1, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const vertices_v = vertices.value.pointer;

    if (vertices_v->field_count <= 2) {
        VECTOR_FIELD_ERROR(
            executor,
            "Invalid sample count. Received `%zu`, expected an amount greater "
            "than or equal to 3",
            vertices_v->field_count
        );
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vector_field const vfield = tetramesh_init(executor);
    struct tetramesh *const mesh = vfield.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    struct vector_field *const org_fields = fields;
    /* hack to make macros read from vector instead of from actual arguments */
    fields = vertices_v->fields;

    struct vec3 root = { 0 };
    for (mc_ind_t i = 0; i < vertices_v->field_count; ++i) {
        struct vec3 current_vertex;
        LIBMC_VEC3_RETURN(current_vertex, i, goto deconstructor);

        if (i == 0) {
            root = current_vertex;
        }
        else if (i == 1) {
            tetramesh_line(
                mesh, root, current_vertex, (struct vec3){ 0, 0, 1 }
            );
        }
        else {
            tetramesh_line_to(mesh, current_vertex);
        }
    }

    tetramesh_line_to(mesh, mesh->lins[0].a.pos);
    tetramesh_line_close(mesh);

    struct vec3 norm = { 0, 0, 1 };
    if (config.value.doub != 0) {
        LIBMC_VEC3_RETURN(norm, 2, goto deconstructor);
    }

    for (mc_ind_t i = 0; i < vertices_v->field_count; ++i) {
        mesh->lins[i].norm =
            mesh->lins[i].norm.z > 0 ? norm : vec3_mul_scalar(-1, norm);
    }

    if (libmc_tag_and_color2(executor, mesh, &org_fields[3]) != 0) {
        return;
    }

    executor->return_register = vfield;
    return;

deconstructor:
    tetramesh_unref(mesh);
}

// native polyline(config, vertices, normal_hint, tag, color, stroke, dot)
void
lib_mc_polyline(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(vertices, 1, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const vertices_v = vertices.value.pointer;

    if (vertices_v->field_count < 2) {
        VECTOR_FIELD_ERROR(executor, "Required at least 2 vertices!");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vector_field const vfield = tetramesh_init(executor);
    struct tetramesh *const mesh = vfield.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    struct vector_field *const org_fields = fields;
    /* hack to make macros read from vector instead of from actual arguments */
    fields = vertices_v->fields;

    struct vec3 root = { 0 };
    for (mc_ind_t i = 0; i < vertices_v->field_count; ++i) {
        struct vec3 current_vertex;
        LIBMC_VEC3_RETURN(current_vertex, i, goto deconstructor);

        if (i == 0) {
            root = current_vertex;
        }
        else if (i == 1) {
            tetramesh_line(
                mesh, root, current_vertex, (struct vec3){ 0, 0, 1 }
            );
        }
        else {
            tetramesh_line_to(mesh, current_vertex);
        }
    }

    tetramesh_line_close(mesh);

    struct vec3 norm = { 0, 0, 1 };
    if (config.value.doub != 0) {
        LIBMC_VEC3_RETURN(norm, 2, goto deconstructor);
    }

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        mesh->lins[i].norm =
            mesh->lins[i].norm.z > 0 ? norm : vec3_mul_scalar(-1, norm);
    }

    if (libmc_tag_and_color1(executor, mesh, &org_fields[3]) != 0) {
        return;
    }

    executor->return_register = vfield;
    return;

deconstructor:
    tetramesh_unref(mesh);
}

/* this is terrible... */
static mc_status_t
_start_and_end(
    struct timeline_execution_context *executor, struct vec3 *start_v,
    struct vec3 *end_v, struct vector_field *fields
)
{
    LIBMC_FULL_CAST_RETURN(
        start, 0, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_MESH,
        return MC_STATUS_FAIL
    );
    LIBMC_FULL_CAST_RETURN(
        end, 1, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_MESH,
        return MC_STATUS_FAIL
    );

    struct vector_field buffer[3];
    buffer[0] = double_init(executor, 0);

    struct vec3 aux_a, aux_b;
    mc_bool_t start_is_mesh = 0;
    if (!(start.vtable->type & VECTOR_FIELD_TYPE_MESH)) {
        LIBMC_FULL_CAST_RETURN(
            vec, 0, VECTOR_FIELD_TYPE_VECTOR, goto arg_0_mesh
        );
        struct vector *v = vec.value.pointer;
        if (v->field_count != 3) {
            goto arg_0_mesh;
        }
        float x[3];
        for (mc_ind_t i = 0; i < 3; ++i) {
            struct vector_field const curr = vector_field_safe_extract_type(
                executor, v->fields[i], VECTOR_FIELD_TYPE_DOUBLE
            );
            if (curr.vtable && curr.vtable->type & VECTOR_FIELD_TYPE_DOUBLE) {
                x[i] = (float) curr.value.doub;
            }
            else {
                goto arg_0_mesh;
            }
        }

        aux_a = (struct vec3){ x[0], x[1], x[2] };
    }
    else {
    arg_0_mesh:
        start_is_mesh = 1;
        buffer[1] = fields[0];
        aux_a = lib_mc_mesh_vec3_center_fields(executor, buffer);
        if (aux_a.x != aux_a.x) {
            return MC_STATUS_FAIL;
        }
    }

    mc_bool_t end_is_mesh = 0;
    if (!(end.vtable->type & VECTOR_FIELD_TYPE_MESH)) {
        LIBMC_FULL_CAST_RETURN(
            vec, 1, VECTOR_FIELD_TYPE_VECTOR, goto arg_1_mesh
        );
        struct vector *v = vec.value.pointer;
        if (v->field_count != 3) {
            goto arg_1_mesh;
        }
        float x[3];
        for (mc_ind_t i = 0; i < 3; ++i) {
            struct vector_field const curr = vector_field_safe_extract_type(
                executor, v->fields[i], VECTOR_FIELD_TYPE_DOUBLE
            );
            if (curr.vtable && curr.vtable->type & VECTOR_FIELD_TYPE_DOUBLE) {
                x[i] = (float) curr.value.doub;
            }
            else {
                goto arg_1_mesh;
            }
        }

        aux_b = (struct vec3){ x[0], x[1], x[2] };
    }
    else {
    arg_1_mesh:
        end_is_mesh = 1;
        buffer[1] = fields[1];
        aux_b = lib_mc_mesh_vec3_center_fields(executor, buffer);
        if (aux_b.x != aux_b.x) {
            return MC_STATUS_FAIL;
        }
    }

    if (start_is_mesh) {
        struct vec3 const delta = vec3_unit(vec3_sub(aux_b, aux_a));
        buffer[1] = fields[0];
        struct vector_field *const aux = fields;
        fields = buffer;
        LIBMC_SELECT_RETURN(mesh_a, 0, return MC_STATUS_FAIL);
        fields = aux;

        float const comp = vec3_dot(delta, aux_a);
        float const dist =
            mesh_direction(mesh_a, delta) - comp + (float) DEFAULT_LINE_BUFFER;
        *start_v = vec3_add(aux_a, vec3_mul_scalar(dist, delta));
        mesh_subset_free(mesh_a);
    }
    else {
        *start_v = aux_a;
    }

    if (end_is_mesh) {
        struct vec3 const delta = vec3_unit(vec3_sub(aux_a, aux_b));
        buffer[1] = fields[1];
        fields = buffer;
        LIBMC_SELECT_RETURN(mesh_b, 0, return MC_STATUS_FAIL);

        float const comp = vec3_dot(delta, aux_b);
        float const dist =
            mesh_direction(mesh_b, delta) - comp + (float) DEFAULT_LINE_BUFFER;
        *end_v = vec3_add(aux_b, vec3_mul_scalar(dist, delta));
        mesh_subset_free(mesh_b);
    }
    else {
        *end_v = aux_b;
    }

    return MC_STATUS_SUCCESS;
}

// func Line([config] {[main] {start, end}, [parameterized] {start, end,
// normal}}, [color] {[main] {tag}, [stroke] {tag, stroke}, [dotted] {tag,
// stroke, dot}})
void
lib_mc_line(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 start, end;
    if (_start_and_end(executor, &start, &end, &fields[1]) !=
        MC_STATUS_SUCCESS) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vec3 normal;
    if (config_ind.value.doub == 1) {
        LIBMC_VEC3(normal, 3);
        normal = vec3_unit(normal);
        LIBMC_NONNULLVEC3(normal);
    }
    else {
        normal = (struct vec3){ 0, 0, 1 };
    }

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    tetramesh_line(mesh, start, end, normal);
    tetramesh_line_close(mesh);

    if (libmc_tag_and_color1(executor, mesh, &fields[4]) != 0) {
        return;
    }

    executor->return_register = field;
}
// func Triangle([config] {[main] {p, q, r}}, [color] {[main] {tag}, [stroke]
// {tag, stroke}, [solid] {tag, stroke, fill}}) = native not_implemented_yet(0)
void
lib_mc_triangle(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);

    struct vec3 norm = { 0, 0, 0 };
    if (config_ind.value.doub != 0) {
        LIBMC_VEC3(norm, 4);
    }

    struct vec3 p, q, r;
    LIBMC_VEC3(p, 1);
    LIBMC_VEC3(q, 2);
    LIBMC_VEC3(r, 3);

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    struct vec3 general_norm = vec3_cross(vec3_sub(q, p), vec3_sub(r, p));
    if (vec3_norm(general_norm) < GEOMETRIC_EPSILON) {
        general_norm = norm;
    }

    tetramesh_line(mesh, p, q, general_norm);
    tetramesh_line_to(mesh, r);
    tetramesh_line_to(mesh, p);
    tetramesh_line_close(mesh);

    if (libmc_tag_and_color2(executor, mesh, &fields[5]) != 0) {
        return;
    }

    executor->return_register = field;
}

static void
arc(struct tetramesh *mesh, struct vec3 pivot, struct vec3 ihat,
    struct vec3 jhat)
{
    mc_count_t const max = DEFAULT_CIRCLE_VERTEX_COUNT / 4;
    for (mc_ind_t i = 0; i < max; ++i) {
        float const theta = (float) M_PI / 2 * (float) (i + 1) / max;
        float const c = cosf(theta);
        float const s = sinf(theta);

        tetramesh_line_to(
            mesh,
            vec3_add(
                pivot,
                vec3_add(vec3_mul_scalar(c, ihat), vec3_mul_scalar(s, jhat))
            )
        );
    }
}

void
lib_mc_capsule(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 start, end, norm;
    LIBMC_FULL_CAST(config, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_VEC3(start, 1);
    LIBMC_VEC3(end, 2);
    if (config.value.doub == 1) {
        LIBMC_VEC3(norm, 3);
        norm = vec3_unit(norm);
    }
    else {
        norm = (struct vec3){ 0, 0, 1 };
    }
    LIBMC_FULL_CAST(in_rad, 4, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(out_rad, 5, VECTOR_FIELD_TYPE_DOUBLE);

    if (in_rad.value.doub > out_rad.value.doub || in_rad.value.doub < 0) {
        VECTOR_FIELD_ERROR(executor, "Invalid inner radius");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    float const i_rad = (float) in_rad.value.doub;
    float const o_rad = (float) out_rad.value.doub;

    struct vec3 delta = vec3_sub(end, start);
    struct vec3 fake_delta;
    if (vec3_equals(delta, VEC3_0)) {
        fake_delta = (struct vec3){ 0, 0, 1 };
    }
    else {
        fake_delta = vec3_unit(delta);
    }

    struct vec3 const right = vec3_unit(vec3_cross(fake_delta, norm));
    struct vec3 const scaled_right = vec3_mul_scalar(o_rad, right);
    struct vec3 const in_right = vec3_mul_scalar(o_rad - i_rad, right);
    struct vec3 const i_right = vec3_mul_scalar(i_rad, right);
    struct vec3 const scaled_forw = vec3_mul_scalar(o_rad, fake_delta);
    struct vec3 const in_forw = vec3_mul_scalar(o_rad - i_rad, fake_delta);
    struct vec3 const i_forw = vec3_mul_scalar(i_rad, fake_delta);

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    struct vec3 const p = vec3_sub(vec3_add(start, scaled_right), in_forw);
    struct vec3 const q = vec3_add(vec3_add(end, scaled_right), in_forw);
    struct vec3 const s = vec3_sub(vec3_sub(start, scaled_right), in_forw);
    struct vec3 const p0 = vec3_add(vec3_sub(start, scaled_forw), in_right);
    struct vec3 const r0 = vec3_sub(vec3_add(end, scaled_forw), in_right);

    tetramesh_line(mesh, p, q, norm);
    arc(mesh, vec3_add(end, vec3_add(in_forw, in_right)), i_right, i_forw);
    tetramesh_line_to(mesh, r0);
    arc(mesh, vec3_add(end, vec3_sub(in_forw, in_right)), i_forw,
        vec3_mul_scalar(-1, i_right));
    tetramesh_line_to(mesh, s);
    arc(mesh, vec3_sub(start, vec3_add(in_forw, in_right)),
        vec3_mul_scalar(-1, i_right), vec3_mul_scalar(-1, i_forw));
    tetramesh_line_to(mesh, p0);
    arc(mesh, vec3_add(start, vec3_sub(in_right, in_forw)),
        vec3_mul_scalar(-1, i_forw), i_right);
    tetramesh_line_close(mesh);

    if (libmc_tag_and_color2(executor, mesh, &fields[6]) != 0) {
        return;
    }

    executor->return_register = field;
}

static struct vec3
sphere_point(double phi, double theta)
{
    double const z = sin(phi);
    double const r = cos(phi);
    double const x = cos(theta) * r;
    double const y = sin(theta) * r;
    return (struct vec3){ (float) x, (float) y, (float) z };
}
// func Sphere([config] {[main] {center, radius}, [parameterized] {center,
// radius, sample_depth}}, [color] {[main] {tag}, [color] {tag, surface},
// [solid] {tag, surface, fill_rate}}) = native not_implemented_yet(0)
#pragma message(                                                               \
    "OPTIMIZATION, i don't really like this (or the triangulation in general)" \
)
/* might want to try https://arxiv.org/pdf/0912.4540.pdf with delaunay
 * triangulations? */
void
lib_mc_sphere(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 center;
    LIBMC_VEC3(center, 1);
    LIBMC_FULL_CAST(radius, 2, VECTOR_FIELD_TYPE_DOUBLE);

    mc_count_t sample_depth = DEFAULT_SPHERE_VERTEX_DEPTH;
    if (config_ind.value.doub == 1) {
        LIBMC_FULL_CAST(samples, 3, VECTOR_FIELD_TYPE_DOUBLE);
        sample_depth = (mc_count_t) samples.value.doub;

        if (sample_depth < 1 || sample_depth > MAX_SPHERE_VERTEX_DEPTH) {
            VECTOR_FIELD_ERROR(
                executor,
                "Invalid sample depth. Received `%zu`, expected an amount "
                "between 1 and %d",
                (int) sample_depth, MAX_SPHERE_VERTEX_DEPTH
            );
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }

    mc_count_t const triangle_row_count =
        DEFAULT_SPHERE_VERTEX_SEED * sample_depth;

    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->tri_count = (triangle_row_count - 1) * (triangle_row_count * 4);
    mesh->tris = mc_malloc(sizeof(struct tetra_tri) * mesh->tri_count);
    mesh->uniform = GLOSSY_UNIFORM;

    for (mc_ind_t i = 0; i < triangle_row_count; ++i) {
        double const phi_0 = MC_PI * (float) i / triangle_row_count - MC_PI / 2;
        double const phi_1 =
            MC_PI * (float) (i + 1) / triangle_row_count - MC_PI / 2;
        for (mc_ind_t j = 0; j < triangle_row_count; ++j) {
            double const theta_0 = 2 * MC_PI * (float) j / triangle_row_count;
            double const theta_1 =
                2 * MC_PI * (float) (j + 1) / triangle_row_count;

            struct vec3 const v0 = sphere_point(phi_0, theta_0);
            struct vec3 const v1 = sphere_point(phi_0, theta_1);
            struct vec3 const v2 = sphere_point(phi_1, theta_0);
            struct vec3 const v3 = sphere_point(phi_1, theta_1);

            if (i == 0) {
                mesh->tris[j] = (struct tetra_tri){
                    .a = { .pos = v0,
                           .norm = v0,
                           .uv = { 0, 0 },
                           .col = VEC4_1 },
                    .b = { .pos = v3,
                           .norm = v3,
                           .uv = { 0, 0 },
                           .col = VEC4_1 },
                    .c = { .pos = v2,
                           .norm = v2,
                           .uv = { 0, 0 },
                           .col = VEC4_1 },
                    .ab = (int32_t) (j == 0 ? triangle_row_count - 1 : j - 1),
                    .bc = (int32_t) (j + triangle_row_count),
                    .ca = (int32_t) (j == triangle_row_count - 1 ? 0 : j + 1),
                    .antinorm = (int32_t) (j + mesh->tri_count / 2),
                };
            }
            else if (i == triangle_row_count - 1) {
                // first set of rows doesn't have as much
                mc_ind_t const base_index =
                    2 * i * triangle_row_count - triangle_row_count;
                mesh->tris[base_index + j] = (struct tetra_tri){
                    .a = { .pos = v0,
                           .norm = v0,
                           .uv = { 0, 0 },
                           .col = VEC4_1 },
                    .b = { .pos = v1,
                           .norm = v1,
                           .uv = { 0, 0 },
                           .col = VEC4_1 },
                    .c = { .pos = v3,
                           .norm = v3,
                           .uv = { 0, 0 },
                           .col = VEC4_1 },
                    .ab = (int32_t) (base_index + j - triangle_row_count),
                    .bc = (int32_t) (base_index +
                                     (j == triangle_row_count - 1 ? 0 : j + 1)),
                    .ca = (int32_t) (base_index +
                                     (j == 0 ? triangle_row_count - 1 : j - 1)),
                    .antinorm =
                        (int32_t) (base_index + j + mesh->tri_count / 2),
                };
            }
            else {
                mc_ind_t const base_index =
                    2 * i * triangle_row_count - triangle_row_count;
                mesh->tris[base_index + j] = (struct tetra_tri){
                    .a = { .pos = v0,
                           .norm = v0,
                           .uv = { 0, 0 },
                           .col = VEC4_1 },
                    .b = { .pos = v1,
                           .norm = v1,
                           .uv = { 0, 0 },
                           .col = VEC4_1 },
                    .c = { .pos = v3,
                           .norm = v3,
                           .uv = { 0, 0 },
                           .col = VEC4_1 },
                    .ab = (int32_t) (base_index + j - triangle_row_count),
                    .bc = (int32_t) (base_index + triangle_row_count +
                                     (j == triangle_row_count - 1 ? 0 : j + 1)),
                    .ca = (int32_t) (base_index + triangle_row_count + j),
                    .antinorm =
                        (int32_t) (base_index + j + mesh->tri_count / 2),
                };
                mesh->tris[base_index + j + triangle_row_count] =
                    (struct tetra_tri){
                        .a = { .pos = v0,
                               .norm = v0,
                               .uv = { 0, 0 },
                               .col = VEC4_1 },
                        .b = { .pos = v3,
                               .norm = v3,
                               .uv = { 0, 0 },
                               .col = VEC4_1 },
                        .c = { .pos = v2,
                               .norm = v2,
                               .uv = { 0, 0 },
                               .col = VEC4_1 },
                        .ab = (int32_t) (base_index + j),
                        .bc =
                            (int32_t) (base_index + j + 2 * triangle_row_count),
                        .ca = (int32_t) (base_index +
                                         (j == 0 ? triangle_row_count - 1
                                                 : j - 1)),
                        .antinorm =
                            (int32_t) (base_index + j + triangle_row_count +
                                       mesh->tri_count / 2),
                    };
            }
        }
    }

    for (mc_ind_t i = 0; i < mesh->tri_count / 2; ++i) {
        float const r = (float) radius.value.doub;
        mesh->tris[i].a.pos =
            vec3_add(vec3_mul_scalar(r, mesh->tris[i].a.pos), center);
        mesh->tris[i].b.pos =
            vec3_add(vec3_mul_scalar(r, mesh->tris[i].b.pos), center);
        mesh->tris[i].c.pos =
            vec3_add(vec3_mul_scalar(r, mesh->tris[i].c.pos), center);
    }

    /* write antinorms */
    for (mc_ind_t i = 0; i < mesh->tri_count / 2; ++i) {
        struct tetra_tri const copy = mesh->tris[i];
        mesh->tris[i + mesh->tri_count / 2] = tetra_tri_flip(
            copy, (int32_t) i, mesh->tris[copy.ab].antinorm,
            mesh->tris[copy.ca].antinorm, mesh->tris[copy.bc].antinorm
        );
    }

    if (libmc_tag_and_color3(executor, mesh, &fields[4]) != 0) {
        return;
    }

    for (mc_ind_t i = 0; i < mesh->tri_count / 2; ++i) {
        mesh->tris[i + mesh->tri_count / 2].a.col = (struct vec4){ 0 };
        mesh->tris[i + mesh->tri_count / 2].b.col = (struct vec4){ 0 };
        mesh->tris[i + mesh->tri_count / 2].c.col = (struct vec4){ 0 };
    }

    executor->return_register = field;
}

// func RectangularPrism([config] {[main] {center, dimensions}}, [color] {[main]
// {tag}, [color] {tag, surface}, [solid] {tag, surface, fill_rate}}) = native
// not_implemented_yet(0)
void
lib_mc_rectangular_prism(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    static int tri_indices[12][3] = {
        { 0, 2, 3 }, { 0, 3, 1 }, { 4, 5, 7 }, { 4, 7, 6 },
        { 0, 4, 6 }, { 0, 6, 2 }, { 1, 3, 7 }, { 1, 7, 5 },
        { 0, 1, 5 }, { 0, 5, 4 }, { 2, 6, 7 }, { 2, 7, 3 },
    };
    static int tri_nbrs[12][3] = {
        { 5, 11, 1 }, { 0, 6, 8 },  { 9, 7, 3 },  { 2, 10, 4 },
        { 9, 3, 5 },  { 4, 10, 0 }, { 1, 11, 7 }, { 6, 2, 8 },
        { 1, 7, 9 },  { 8, 2, 4 },  { 5, 3, 11 }, { 10, 6, 1 },
    };
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 center, dimension;
    LIBMC_VEC3(center, 1);
    LIBMC_VEC3(dimension, 2);

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = GLOSSY_UNIFORM;
    mesh->tri_count = 24;
    mesh->tris = mc_malloc(sizeof(struct tetra_tri) * 24);

    float const rx = dimension.x / 2;
    float const ry = dimension.y / 2;
    float const rz = dimension.z / 2;

    for (mc_ind_t i = 0; i < 12; ++i) {
        struct tetra_tri curr;

        struct tetra_tri_vertex verts[3];

        for (mc_ind_t j = 0; j < 3; ++j) {
            int const ind = tri_indices[i][j];

            verts[j].col = VEC4_1;
            verts[j].uv = (struct vec2){ 0, 0 };
            verts[j].pos = (struct vec3){
                center.x + (((ind & 1) << 1) - 1) * rx,
                center.y + (((ind & 2) << 0) - 1) * ry,
                center.z + (((ind & 4) >> 1) - 1) * rz,
            };
        }

        struct vec3 const norm = vec3_cross(
            vec3_sub(verts[1].pos, verts[0].pos),
            vec3_sub(verts[2].pos, verts[0].pos)
        );
        for (mc_ind_t j = 0; j < 3; ++j) {
            verts[j].norm = norm;
        }

        curr.a = verts[0];
        curr.b = verts[1];
        curr.c = verts[2];
        curr.ab = 2 * tri_nbrs[i][0];
        curr.bc = 2 * tri_nbrs[i][1];
        curr.ca = 2 * tri_nbrs[i][2];
        curr.antinorm = 2 * (int32_t) i + 1;
        curr.is_dominant_sibling = 1;

        mesh->tris[2 * i] = curr;
    }

    /* flip */
    for (mc_ind_t i = 0; i < 12; ++i) {
        struct tetra_tri const copy = mesh->tris[2 * i];
        mesh->tris[2 * i + 1] = tetra_tri_flip(
            copy, 2 * (int32_t) i, mesh->tris[copy.ab].antinorm,
            mesh->tris[copy.ca].antinorm, mesh->tris[copy.bc].antinorm
        );
    }

    if (libmc_tag_and_color3(executor, mesh, &fields[3]) != 0) {
        return;
    }

    for (mc_ind_t i = 0; i < 12; ++i) {
        mesh->tris[2 * i + 1].a.col = (struct vec4){ 0 };
        mesh->tris[2 * i + 1].b.col = (struct vec4){ 0 };
        mesh->tris[2 * i + 1].c.col = (struct vec4){ 0 };
    }

    executor->return_register = field;
}

// func Cylinder([config] {[main] {center, radius, height}, [parameterized]
// {center, radius, height, direction, sample_count}}, [color] {[main] {tag},
// [color] {tag, surface}, [solid] {tag, surface, fill_rate}}) = native
// not_implemented_yet(0)
void
lib_mc_cylinder(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 center;
    LIBMC_VEC3(center, 1);
    LIBMC_FULL_CAST(radius, 2, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(height, 3, VECTOR_FIELD_TYPE_DOUBLE);

    mc_count_t sample_count = DEFAULT_CIRCLE_VERTEX_COUNT;
    struct vec3 direction;
    if (config_ind.value.doub == 1) {
        LIBMC_VEC3(direction, 4);
        LIBMC_FULL_CAST(samples, 5, VECTOR_FIELD_TYPE_DOUBLE);

        if ((int) samples.value.doub <= 2 ||
            (int) samples.value.doub > MAX_POLYGON_VERTEX_COUNT) {
            VECTOR_FIELD_ERROR(
                executor,
                "Invalid sample count. Received `%d`, expected an amount "
                "between 3 and %d",
                (int) samples.value.doub, MAX_POLYGON_VERTEX_COUNT
            );
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        sample_count = (mc_count_t) samples.value.doub;
        LIBMC_NONNULLVEC3(direction);
    }
    else {
        direction = (struct vec3){ 0, 0, 1 };
    }

    float const r = (float) radius.value.doub;
    float const h = (float) height.value.doub / 2;
    struct plane3 const plane = vec3_plane_basis(direction);

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = GLOSSY_UNIFORM;
    mesh->tri_count = sample_count * 4 + (sample_count - 2) * 4;
    mesh->tris = mc_malloc(sizeof(struct tetra_tri) * mesh->tri_count);

    struct tetra_tri_vertex prev_u, prev_d;
    prev_u.pos = (struct vec3){
        r * plane.a.x + center.x + h * direction.x,
        r * plane.a.y + center.y + h * direction.y,
        r * plane.a.z + center.z + h * direction.z,
    };
    prev_d.pos = (struct vec3){
        r * plane.a.x + center.x - h * direction.x,
        r * plane.a.y + center.y - h * direction.y,
        r * plane.a.z + center.z - h * direction.z,
    };
    prev_u.col = prev_d.col = VEC4_1;
    prev_u.uv = prev_d.uv = (struct vec2){ 0, 0 };
    prev_u.norm = prev_d.norm = plane.a;

    struct tetra_tri_vertex org_u = prev_u;
    struct tetra_tri_vertex org_d = prev_d;
    org_u.norm = direction;
    org_d.norm = vec3_mul_scalar(-1, direction);

    for (mc_ind_t i = 0; i < sample_count; ++i) {
        struct tetra_tri_vertex curr_u;
        struct tetra_tri_vertex curr_d;

        float const cos = cosf((i + 1) * 2 * (float) MC_PI / sample_count);
        float const sin = sinf((i + 1) * 2 * (float) MC_PI / sample_count);

        curr_u.col = curr_d.col = VEC4_1;
        curr_u.uv = curr_d.uv = (struct vec2){ 0, 0 };
        curr_u.pos = (struct vec3){
            r * cos * plane.a.x + r * sin * plane.b.x + center.x +
                h * direction.x,
            r * cos * plane.a.y + r * sin * plane.b.y + center.y +
                h * direction.y,
            r * cos * plane.a.z + r * sin * plane.b.z + center.z +
                h * direction.z,
        };
        curr_d.pos = (struct vec3){
            r * cos * plane.a.x + r * sin * plane.b.x + center.x -
                h * direction.x,
            r * cos * plane.a.y + r * sin * plane.b.y + center.y -
                h * direction.y,
            r * cos * plane.a.z + r * sin * plane.b.z + center.z -
                h * direction.z,
        };
        curr_u.norm = curr_d.norm = (struct vec3){
            cos * plane.a.x + sin * plane.b.x,
            cos * plane.a.y + sin * plane.b.y,
            cos * plane.a.z + sin * plane.b.z,
        };

        int32_t corresponding_circle_edge;
        if (!i) {
            corresponding_circle_edge = 4 * (int32_t) sample_count;
        }
        else if (i == sample_count - 1) {
            corresponding_circle_edge =
                4 * (int32_t) (sample_count + sample_count - 3);
        }
        else {
            corresponding_circle_edge = 4 * (int32_t) (sample_count + i - 1);
        }

        mesh->tris[4 * i] = (struct tetra_tri){
            .a = curr_d,
            .b = curr_u,
            .c = prev_d,
            .ab = i == sample_count - 1 ? 1 : 4 * (int32_t) i + 5,
            .bc = 4 * (int32_t) i + 1,
            .ca = corresponding_circle_edge,
            .antinorm = 4 * (int32_t) i + 2,
            .is_dominant_sibling = 1,
        };
        mesh->tris[4 * i + 1] = (struct tetra_tri){
            .a = prev_d,
            .b = curr_u,
            .c = prev_u,
            .ab = 4 * (int32_t) i,
            .bc = corresponding_circle_edge + 1,
            .ca = (int32_t) (!i ? 4 * sample_count - 4 : 4 * i - 4),
            .antinorm = 4 * (int32_t) i + 3,
            .is_dominant_sibling = 1,
        };

        struct tetra_tri_vertex mod_prev_d = prev_d;
        struct tetra_tri_vertex mod_prev_u = prev_u;

        prev_d = curr_d;
        prev_u = curr_u;

        if (1 <= i && i < sample_count - 1) {
            curr_d.norm = mod_prev_d.norm = vec3_mul_scalar(-1, direction);
            curr_u.norm = mod_prev_u.norm = direction;

            int32_t const p_index =
                i == 1 ? 0 : 4 * (int32_t) (sample_count + i - 2);
            int32_t const n_index = i == sample_count - 2
                                        ? 4 * (int32_t) (sample_count) -4
                                        : 4 * (int32_t) (sample_count + i);

            mesh->tris[4 * (sample_count + i - 1)] = (struct tetra_tri){
                .a = curr_d,
                .b = mod_prev_d,
                .c = org_d,
                .ab = 4 * (int32_t) i,
                .bc = p_index,
                .ca = n_index,
                .antinorm = 4 * (int32_t) (sample_count + i - 1) + 2,
                .is_dominant_sibling = 1,
            };
            mesh->tris[4 * (sample_count + i - 1) + 1] = (struct tetra_tri){
                .a = org_u,
                .b = mod_prev_u,
                .c = curr_u,
                .ab = p_index + 1,
                .bc = 4 * (int32_t) i + 1,
                .ca = n_index + 1,
                .antinorm = 4 * (int32_t) (sample_count + i - 1) + 3,
                .is_dominant_sibling = 1,
            };
        }
    }

    /* write antinorms */
    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        struct tetra_tri const copy = mesh->tris[i];
        mesh->tris[i + 2] = tetra_tri_flip(
            copy, (int32_t) i, mesh->tris[copy.ab].antinorm,
            mesh->tris[copy.ca].antinorm, mesh->tris[copy.bc].antinorm
        );

        /* skip antinorms */
        if (i & 1) {
            i += 2;
        }
    }

    if (libmc_tag_and_color3(executor, mesh, &fields[6]) != 0) {
        return;
    }

    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        mesh->tris[i + 2].a.col = (struct vec4){ 0 };
        mesh->tris[i + 2].b.col = (struct vec4){ 0 };
        mesh->tris[i + 2].c.col = (struct vec4){ 0 };

        /* skip antinorms */
        if (i & 1) {
            i += 2;
        }
    }

    tetramesh_assert_invariants(mesh);

    executor->return_register = field;
}

static mc_status_t
bezier_dump(
    struct timeline_execution_context *executor, struct tetramesh *mesh,
    struct vector_field *fields
)
{
    struct vec3 v0, v1, v2, v3;
    LIBMC_VEC3_RETURN(v0, 0, return MC_STATUS_FAIL);
    LIBMC_VEC3_RETURN(v1, 1, return MC_STATUS_FAIL);
    LIBMC_VEC3_RETURN(v2, 2, return MC_STATUS_FAIL);
    LIBMC_VEC3_RETURN(v3, 3, return MC_STATUS_FAIL);

    for (mc_ind_t i = 0; i < BEZIER_SAMPLE; ++i) {
        float t = 1 - (float) (i + 1) / BEZIER_SAMPLE;
        float t_prime = 1 - t;

        float a = t * t * t;
        float b = 3 * t * t * t_prime;
        float c = 3 * t * t_prime * t_prime;
        float d = t_prime * t_prime * t_prime;

        struct vec3 const curr = vec3_add(
            vec3_add(vec3_mul_scalar(a, v0), vec3_mul_scalar(b, v1)),
            vec3_add(vec3_mul_scalar(c, v2), vec3_mul_scalar(d, v3))
        );

        if (!i && !mesh->lin_count) {
            tetramesh_line(mesh, v0, curr, (struct vec3){ 0, 0, 1 });
        }
        else {
            tetramesh_line_to(mesh, curr);
        }
    }

    return MC_STATUS_SUCCESS;
}

// func Bezier(control_points, [color] {[main] {tag}, [stroke] {tag, stroke},
// [solid] {tag, stroke, fill}}) = native not_implemented_yet(0)
void
lib_mc_bezier(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(control_points, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector_field *const tags = &fields[1];

    struct vector *const control_v = control_points.value.pointer;
    fields = control_v->fields;

    if (control_v->field_count < 4 || control_v->field_count % 3 != 1) {
        VECTOR_FIELD_ERROR(
            executor,
            "Expected at least 4 control points, and for a count to be 1 "
            "more than a multiple of 3"
        );
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;
    mesh->lin_count = BEZIER_SAMPLE * (control_v->field_count - 1) / 3;
    mesh->lins = mc_malloc(sizeof(struct tetra_lin) * mesh->lin_count);

    for (mc_ind_t i = 0; i < (control_v->field_count - 1) / 3; ++i) {
        if (bezier_dump(executor, mesh, &fields[3 * i]) != 0) {
            tetramesh_unref(mesh);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }
    tetramesh_line_close(mesh);

    struct vec3 norm = { 0 };
    for (mc_ind_t i = 0, j = (mc_ind_t) mesh->lins[0].prev; j;
         j = i, i = (mc_ind_t) mesh->lins[i].next) {
        struct vec3 const curr = vec3_cross(
            vec3_sub(mesh->lins[j].b.pos, mesh->lins[j].a.pos),
            vec3_sub(mesh->lins[i].b.pos, mesh->lins[i].a.pos)
        );
        norm = vec3_add(curr, norm);
    }
    norm = vec3_unit(norm);

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        mesh->lins[i].norm =
            mesh->lins[i].norm.z > 0 ? norm : vec3_mul_scalar(-1, norm);
    }

    if (libmc_tag_and_color2(executor, mesh, tags) != 0) {
        return;
    }

    executor->return_register = field;
}

static struct vector_field *
remap(
    struct timeline_execution_context *executor, struct vector_field func,
    struct vec3_plane_covering contour
)
{
    if (contour.count == SIZE_MAX) {
        executor->return_register = VECTOR_FIELD_NULL;
        return NULL;
    }
    else if (!contour.count) {
        VECTOR_FIELD_ERROR(
            executor, "Expected subset to have at least 1 element"
        );
        executor->return_register = VECTOR_FIELD_NULL;
        return NULL;
    }

    /* guaranteed to succeed */
    func = vector_field_nocopy_extract_type(
        executor, func, VECTOR_FIELD_TYPE_FUNCTION
    );

    struct vector_field in_vector = vector_init(executor);
    struct vector *const in_vector_pointer = in_vector.value.pointer;
    for (int i = 0; i < 3; ++i) {
        struct vector_field zero_element = double_init(executor, 0);
        vector_plus(executor, in_vector, &zero_element);
    }

    struct vector_field *const remap =
        mc_calloc(contour.count, sizeof(struct vector_field));
    for (mc_ind_t i = 0; i < contour.count; ++i) {
        if (!contour.enabled_points[i]) {
            continue;
        }

        struct vec3 const point = contour.points[i];

        in_vector_pointer->fields[0] = double_init(executor, point.x);
        in_vector_pointer->fields[1] = double_init(executor, point.y);
        in_vector_pointer->fields[2] = double_init(executor, point.z);

        function_call(executor, func, 1, &in_vector);
        if (!executor->return_register.vtable) {
            goto deconstructor;
        }
        remap[i] =
            vector_field_lvalue_unwrap(executor, &executor->return_register);
        executor->return_register = VECTOR_FIELD_NULL;
    }

    VECTOR_FIELD_FREE(executor, in_vector);
    return remap;

deconstructor:
    VECTOR_FIELD_FREE(executor, in_vector);
    for (mc_ind_t i = 0; i < contour.count; ++i) {
        VECTOR_FIELD_FREE(executor, remap[i]);
    }
    mc_free(remap);
    executor->return_register = VECTOR_FIELD_NULL;

    return NULL;
}

// func Field([points] {[main] {x_min, x_max, y_min, y_max}, [step] {x_min,
// x_max, y_min, y_max, x_step, y_step}, [mask] {x_min, x_max, y_min, y_max,
// x_step, y_step, predicate(x,y)}, [domain] {domain, resample_rate}},
// mesh_at(pos)) = native field(points, x_min, x_max, y_min, y_max, x_step,
// y_step, predicate!, mesh_at!)
void
lib_mc_field(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3_plane_covering const contour =
        tetramesh_planar2d_sample(executor, fields);

    struct vector_field *const meshes = remap(executor, fields[8], contour);

    if (!meshes) {
        vec3_plane_covering_free(contour);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    mc_ind_t j = 0;
    for (mc_ind_t i = 0; i < contour.count; ++i) {
        if (contour.enabled_points[i]) {
            meshes[j++] = meshes[i];
        }
    }

    struct vector_field const vector_field = vector_init(executor);
    struct vector *const vector = vector_field.value.pointer;
    vector->field_count = j;
    vector->fields = meshes;
    executor->byte_alloc += vector->field_count;

    executor->return_register = vector_field;

    vec3_plane_covering_free(contour);
}

#pragma message("CLEANUP, document the inner mechanics of this slightly better")
// ([points] {[main] {x_min, x_max, y_min, y_max}, [step] {x_min, x_max, y_min,
// y_max, x_step, y_step}, [mask] {x_min, x_max, y_min, y_max, x_step, y_step,
// mask(pos)}, [domain] {domain, resample_rate}}, tag, [color] {[auto_stroke]
// {color_at(pos)}, [custom_stroke] {color_at(pos), stroke}}) = native
// color_grid(points, x_min, x_max, y_min, y_max, x_step, y_step, mask!, tag,
// color, color_at!, stroke)
void
lib_mc_color_grid(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3_plane_covering const contour =
        tetramesh_planar2d_sample(executor, fields);

    struct vector_field *const meshes = remap(executor, fields[10], contour);

    if (!meshes) {
        vec3_plane_covering_free(contour);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    struct vec4 stroke = VEC4_1;
    // guaranteed to succeed
    LIBMC_FULL_CAST(color_index, 9, VECTOR_FIELD_TYPE_DOUBLE);
    if (color_index.value.doub == 1) {
        LIBMC_VEC4_RETURN(stroke, 11, vec3_plane_covering_free(contour);
                          return);
    }

    struct vec4 *const colors = mc_malloc(sizeof(struct vec4) * contour.count);

    int *const block =
        mc_malloc((contour.rows - 1) * (contour.cols - 1) * sizeof(int));
    int *const up = mc_calloc(contour.rows * contour.cols, sizeof(int));
    int *const down = mc_calloc(contour.rows * contour.cols, sizeof(int));
    int *const left = mc_calloc(contour.rows * contour.cols, sizeof(int));
    int *const right = mc_calloc(contour.rows * contour.cols, sizeof(int));

    struct vec3 const norm = { 0, 0, 1 };
    struct vec3 const antinorm = { 0, 0, -1 };

    struct vector_field const tag = fields[8];

    fields = meshes;
    for (mc_ind_t i = 0; i < contour.count; ++i) {
        if (!contour.enabled_points[i]) {
            continue;
        }

        struct vec4 color_at;
        LIBMC_VEC4_RETURN(color_at, i, goto free);

        colors[i] = color_at;
    }

    struct vector_field const ret = tetramesh_init(executor);
    struct tetramesh *const mesh = ret.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    if (libmc_tag(executor, mesh, tag) != MC_STATUS_SUCCESS) {
        goto free;
    }

    for (mc_ind_t r = 0; r < contour.rows - 1; ++r) {
        for (mc_ind_t c = 0; c < contour.cols - 1; ++c) {
            block[r * (contour.cols - 1) + c] =
                contour.enabled_points[r * contour.cols + c] &&
                contour.enabled_points[r * contour.cols + c + 1] &&
                contour.enabled_points[(r + 1) * contour.cols + c] &&
                contour.enabled_points[(r + 1) * contour.cols + c + 1];
        }
    }

    /* fill in lines */
    for (mc_ind_t r = 0; r < contour.rows; ++r) {
        for (mc_ind_t c = 0; c < contour.cols; ++c) {
            /* check right and down and apply lines  */
            if (c < contour.cols - 1) {
                /* horizontal line */
                mc_bool_t const south =
                    r > 0 && block[(r - 1) * (contour.cols - 1) + c];
                mc_bool_t const north =
                    r < contour.rows - 1 && block[r * (contour.cols - 1) + c];

                if (south != north) {
                    //                    struct vec4 const color = south ?
                    //                    colors[(r - 1) * contour.cols + c] :
                    //                    colors[r * contour.cols + c];

                    MC_MEM_RESERVEN(mesh->lins, mesh->lin_count, 2);
                    mesh->lins[mesh->lin_count] = (struct tetra_lin){
                        .a = { .pos = contour.points[r * contour.cols + c],
                               .col = stroke },
                        .b = { .pos = contour.points[r * contour.cols + c + 1],
                               .col = stroke },
                        .norm = north ? norm : antinorm,
                        .prev = -1,
                        .next = -1,
                        .inverse = 0,
                        .antinorm = (int32_t) mesh->lin_count + 1,
                        .is_dominant_sibling = 1,
                    };
                    mesh->lins[mesh->lin_count + 1] = (struct tetra_lin){
                        .a = { .pos = contour.points[r * contour.cols + c + 1],
                               .col = stroke },
                        .b = { .pos = contour.points[r * contour.cols + c],
                               .col = stroke },
                        .norm = north ? antinorm : norm,
                        .prev = -1,
                        .next = -1,
                        .inverse = 0,
                        .antinorm = (int32_t) mesh->lin_count,
                        .is_dominant_sibling = 1,
                    };

                    int const elem =
                        -1 - (int32_t) (north ? mesh->lin_count
                                              : mesh->lin_count + 1);
                    if (r > 0) {
                        up[(r - 1) * contour.cols + c] = elem;
                    }
                    down[r * contour.cols + c] = elem;

                    mesh->lin_count += 2;
                }
            }

            if (r < contour.rows - 1) {
                /* east and west based on viewpoint point of the line */
                mc_bool_t const west =
                    c > 0 && block[r * (contour.cols - 1) + c - 1];
                mc_bool_t const east =
                    c < contour.cols - 1 && block[r * (contour.cols - 1) + c];

                if (east != west) {
                    //                    struct vec4 const color = east ?
                    //                    colors[r * contour.cols + c] :
                    //                    colors[r * contour.cols + c - 1];

                    MC_MEM_RESERVEN(mesh->lins, mesh->lin_count, 2);
                    mesh->lins[mesh->lin_count] = (struct tetra_lin){
                        .a = { .pos = contour.points[r * contour.cols + c],
                               .col = stroke },
                        .b = { .pos =
                                   contour.points[(r + 1) * contour.cols + c],
                               .col = stroke },
                        .norm = west ? norm : antinorm,
                        .prev = -1,
                        .next = -1,
                        .inverse = 0,
                        .antinorm = (int32_t) mesh->lin_count + 1,
                        .is_dominant_sibling = 1,
                    };
                    mesh->lins[mesh->lin_count + 1] = (struct tetra_lin){
                        .a = { .pos =
                                   contour.points[(r + 1) * contour.cols + c],
                               .col = stroke },
                        .b = { .pos = contour.points[r * contour.cols + c],
                               .col = stroke },
                        .norm = west ? antinorm : norm,
                        .prev = -1,
                        .next = -1,
                        .inverse = 0,
                        .antinorm = (int32_t) mesh->lin_count,
                        .is_dominant_sibling = 1,
                    };

                    int const elem =
                        -1 - (int32_t) (west ? mesh->lin_count
                                             : mesh->lin_count + 1);
                    if (c > 0) {
                        right[r * contour.cols + c - 1] = elem;
                    }
                    left[r * contour.cols + c] = elem;

                    mesh->lin_count += 2;
                }
            }
        }
    }

    /* join nexts and prevs */
    for (mc_ind_t r = 0; r < contour.rows; ++r) {
        for (mc_ind_t c = 0; c < contour.cols; ++c) {
            if (c < contour.cols - 1) {
                /* horizontal line */
                mc_bool_t const south =
                    r > 0 && block[(r - 1) * (contour.cols - 1) + c];
                mc_bool_t const north =
                    r < contour.rows - 1 && block[r * (contour.cols - 1) + c];

                int32_t const lin = r > 0 ? -1 - up[(r - 1) * contour.cols + c]
                                          : -1 - down[r * contour.cols + c];
                int32_t pair = -1;

                if (south && !north) {
                    if (r < contour.rows - 1 &&
                        left[r * contour.cols + c] < 0) {
                        // up
                        pair = -1 - left[r * contour.cols + c];
                    }
                    else if (c > 0 && down[r * contour.cols + c - 1] < 0) {
                        // left
                        pair = -1 - down[r * contour.cols + c - 1];
                    }
                    else {
                        // down
                        pair = -1 - left[(r - 1) * contour.cols + c];
                    }
                }
                else if (!south && north) {
                    if (r > 0 && right[(r - 1) * contour.cols + c] < 0) {
                        // down
                        pair = -1 - right[(r - 1) * contour.cols + c];
                    }
                    else if (c < contour.cols - 2 && down[r * contour.cols + c + 1] < 0) {
                        // right
                        pair = -1 - down[r * contour.cols + c + 1];
                    }
                    else {
                        // up
                        pair = -1 - right[r * contour.cols + c];
                    }
                }

                if (south != north) {
                    mesh->lins[lin].next = pair;
                    mesh->lins[pair].prev = lin;
                    mesh->lins[mesh->lins[lin].antinorm].prev =
                        mesh->lins[pair].antinorm;
                    mesh->lins[mesh->lins[pair].antinorm].next =
                        mesh->lins[lin].antinorm;
                }
            }

            if (r < contour.rows - 1) {
                mc_bool_t const east =
                    c < contour.cols - 1 && block[r * (contour.cols - 1) + c];
                mc_bool_t const west =
                    c > 0 && block[r * (contour.cols - 1) + c - 1];

                /* dereferenced only when it's valid */
                int32_t const lin = c > 0 ? -1 - right[r * contour.cols + c - 1]
                                          : -1 - left[r * contour.cols + c];
                int32_t pair = -1;

                if (east && !west) {
                    if (c > 0 && down[r * contour.cols + c - 1] < 0) {
                        // left
                        pair = -1 - down[r * contour.cols + c - 1];
                    }
                    else if (r > 0 && left[(r - 1) * contour.cols + c] < 0) {
                        // down
                        pair = -1 - left[(r - 1) * contour.cols + c];
                    }
                    else {
                        // right
                        pair = -1 - down[r * contour.cols + c];
                    }
                }
                else if (!east && west) {
                    if (c < contour.cols - 1 && up[r * contour.cols + c] < 0) {
                        // right
                        pair = -1 - up[r * contour.cols + c];
                    }
                    else if (r < contour.rows - 2 && left[(r + 1) * contour.cols + c] < 0) {
                        // up
                        pair = -1 - left[(r + 1) * contour.cols + c];
                    }
                    else {
                        // right
                        pair = -1 - up[r * contour.cols + c - 1];
                    }
                }

                if (east != west) {
                    mesh->lins[lin].next = pair;
                    mesh->lins[pair].prev = lin;
                    mesh->lins[mesh->lins[lin].antinorm].prev =
                        mesh->lins[pair].antinorm;
                    mesh->lins[mesh->lins[pair].antinorm].next =
                        mesh->lins[lin].antinorm;
                }
            }
        }
    }

    /* build triangles */
    int32_t k = 0;
    for (mc_ind_t r = 0; r < contour.rows - 1; ++r) {
        for (mc_ind_t c = 0; c < contour.cols - 1; ++c) {
            if (block[r * (contour.cols - 1) + c]) {
                if (r > 0) {
                    up[(r - 1) * contour.cols + c] = k;
                }
                down[(r + 1) * contour.cols + c] = k + 1;

                if (c > 0) {
                    right[r * contour.cols + c - 1] = k + 1;
                }
                left[r * contour.cols + c + 1] = k;

                k += 4;
            }
        }
    }

    for (mc_ind_t r = 0; r < contour.rows - 1; ++r) {
        for (mc_ind_t c = 0; c < contour.cols - 1; ++c) {
            if (block[r * (contour.cols - 1) + c]) {
                struct vec3 const p0 = contour.points[r * contour.cols + c];
                struct vec3 const p1 =
                    contour.points[(r + 1) * contour.cols + c];
                struct vec3 const p2 = contour.points[r * contour.cols + c + 1];
                struct vec3 const p3 =
                    contour.points[(r + 1) * contour.cols + c + 1];

                struct tetra_tri_vertex const v0 = {
                    .pos = p0,
                    .norm = norm,
                    .uv = { 0, 0 },
                    .col = colors[r * contour.cols + c]
                };
                struct tetra_tri_vertex const v1 = {
                    .pos = p1,
                    .norm = norm,
                    .uv = { 0, 0 },
                    .col = colors[r * contour.cols + c]
                };
                struct tetra_tri_vertex const v2 = {
                    .pos = p2,
                    .norm = norm,
                    .uv = { 0, 0 },
                    .col = colors[r * contour.cols + c]
                };
                struct tetra_tri_vertex const v3 = {
                    .pos = p3,
                    .norm = norm,
                    .uv = { 0, 0 },
                    .col = colors[r * contour.cols + c]
                };

                int32_t const north = up[r * contour.cols + c];
                int32_t const east = right[r * contour.cols + c];
                int32_t const west = left[r * contour.cols + c];
                int32_t const south = down[r * contour.cols + c];

                MC_MEM_RESERVEN(mesh->tris, mesh->tri_count, 4);
                mesh->tris[mesh->tri_count] = (struct tetra_tri){
                    .a = v0,
                    .b = v2,
                    .c = v3,
                    .ab = south,
                    .bc = east,
                    .ca = (int32_t) mesh->tri_count + 1,
                    .antinorm = (int32_t) mesh->tri_count + 2,
                    .is_dominant_sibling = 1,
                };
                mesh->tris[mesh->tri_count + 1] = (struct tetra_tri){
                    .a = v0,
                    .b = v3,
                    .c = v1,
                    .ab = (int32_t) mesh->tri_count,
                    .bc = north,
                    .ca = west,
                    .antinorm = (int32_t) mesh->tri_count + 3,
                    .is_dominant_sibling = 1,
                };

                int32_t const a_north =
                    north < 0 ? -1 - mesh->lins[-1 - north].antinorm
                              : north + 2;
                int32_t const a_east =
                    east < 0 ? -1 - mesh->lins[-1 - east].antinorm : east + 2;
                int32_t const a_west =
                    west < 0 ? -1 - mesh->lins[-1 - west].antinorm : west + 2;
                int32_t const a_south =
                    south < 0 ? -1 - mesh->lins[-1 - south].antinorm
                              : south + 2;

                mesh->tris[mesh->tri_count + 2] = tetra_tri_flip(
                    mesh->tris[mesh->tri_count], (int32_t) mesh->tri_count,
                    a_south, (int32_t) mesh->tri_count + 3, a_east
                );
                mesh->tris[mesh->tri_count + 3] = tetra_tri_flip(
                    mesh->tris[mesh->tri_count + 1],
                    (int32_t) mesh->tri_count + 1,
                    (int32_t) mesh->tri_count + 2, a_west, a_north
                );

                /* build line inverses */
                if (north < 0) {
                    mesh->lins[-1 - north].inverse =
                        -1 - ((int32_t) mesh->tri_count + 1);
                    mesh->lins[-1 - a_north].inverse =
                        -1 - ((int32_t) mesh->tri_count + 3);
                }
                if (south < 0) {
                    mesh->lins[-1 - south].inverse =
                        -1 - ((int32_t) mesh->tri_count);
                    mesh->lins[-1 - a_south].inverse =
                        -1 - ((int32_t) mesh->tri_count + 2);
                }
                if (east < 0) {
                    mesh->lins[-1 - east].inverse =
                        -1 - ((int32_t) mesh->tri_count);
                    mesh->lins[-1 - a_east].inverse =
                        -1 - ((int32_t) mesh->tri_count + 2);
                }
                if (west < 0) {
                    mesh->lins[-1 - west].inverse =
                        -1 - ((int32_t) mesh->tri_count + 1);
                    mesh->lins[-1 - a_west].inverse =
                        -1 - ((int32_t) mesh->tri_count + 3);
                }

                /* build other inverses */

                mesh->tri_count += 4;
            }
        }
    }

    assert(tetramesh_assert_invariants(mesh));
    executor->return_register = ret;

free:
    for (mc_ind_t i = 0; i < contour.count; ++i) {
        VECTOR_FIELD_FREE(executor, fields[i]);
    }
    vec3_plane_covering_free(contour);

    mc_free(fields);
    mc_free(colors);
    mc_free(block);

    mc_free(up);
    mc_free(down);
    mc_free(right);
    mc_free(left);
}

/* if half flag is true, then path_arc is assumed to be zero */
void
vector_like(
    struct timeline_execution_context *executor, struct vec3 tail,
    struct vec3 delta, struct vec3 normal, float path_arc, mc_bool_t half,
    struct vector_field *tags
)
{
    /* create a tetramesh */
    assert(!half || fabsf(path_arc) < GEOMETRIC_EPSILON);

    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    float const alpha = fabsf(path_arc);
    mc_count_t const samples =
        alpha < GEOMETRIC_EPSILON ? 2 : DEFAULT_CIRCLE_VERTEX_COUNT;
    struct vec3 positions[DEFAULT_CIRCLE_VERTEX_COUNT];
    struct vec3 norm[DEFAULT_CIRCLE_VERTEX_COUNT];
    float const length = vec3_norm(delta);
    float head_r = length * MIN_TIP_R_TO_LENGTH > MAX_TIP_R
                       ? MAX_TIP_R
                       : length * MIN_TIP_R_TO_LENGTH;
    float const line_r = head_r * LINE_R_OVER_TIP_R;
    float head_h = head_r * sqrtf(3);

    struct vec3 const v_path_arc = vec3_mul_scalar(path_arc, normal);
    float const sinc =
        alpha < GEOMETRIC_EPSILON ? 1 : sinf(alpha / 2) / (alpha / 2);
    float const modded_length = length / sinc;
    float const true_end = modded_length < GEOMETRIC_EPSILON
                               ? 0
                               : (modded_length - head_h) / modded_length;

    for (mc_ind_t i = 0; i < samples; ++i) {
        float const t = (float) i / (samples - 1) * true_end;
        positions[i] =
            vec3_patharc_lerp(tail, t, vec3_add(tail, delta), v_path_arc);
    }
    for (mc_ind_t i = 0; i < samples; i++) {
        mc_ind_t const j = i == 0 ? i : i - 1;
        mc_ind_t const k = i == 0 ? i + 1 : i;
        norm[i] = vec3_mul_scalar(
            line_r,
            vec3_unit(vec3_cross(vec3_sub(positions[k], positions[j]), normal))
        );
    }

    tetramesh_line(mesh, tail, vec3_add(positions[0], norm[0]), normal);
    for (mc_ind_t i = 1; i < samples; ++i) {
        tetramesh_line_to(mesh, vec3_add(positions[i], norm[i]));
    }

    tetramesh_line_to(
        mesh, vec3_add(
                  positions[samples - 1],
                  vec3_mul_scalar(1 / LINE_R_OVER_TIP_R, norm[samples - 1])
              )
    );
    tetramesh_line_to(mesh, vec3_add(tail, delta));

    if (!half) {
        tetramesh_line_to(
            mesh, vec3_sub(
                      positions[samples - 1],
                      vec3_mul_scalar(1 / LINE_R_OVER_TIP_R, norm[samples - 1])
                  )
        );
        for (mc_rind_t i = (mc_rind_t) samples - 1; i >= 0; --i) {
            tetramesh_line_to(mesh, vec3_sub(positions[i], norm[i]));
        }
    }

    tetramesh_line_to(mesh, tail);
    tetramesh_line_close(mesh);

    if (libmc_tag_and_color2_forceuprank(executor, mesh, tags) != 0) {
        return;
    }

    executor->return_register = field;
}

// func HalfVector([config] {[main] {tail, delta}, [normal] {tail, delta,
// normal}}, [color] {[main] {tag}, [stroke] {tag, stroke}, [solid] {tag,
// stroke, fill}}) = native not_implemented_yet(0)
void
lib_mc_half_vector(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 tail, delta;
    LIBMC_VEC3(tail, 1);
    LIBMC_VEC3(delta, 2);

    struct vec3 normal;
    if (config_ind.value.doub == 1) {
        LIBMC_VEC3(normal, 3);
        normal = vec3_unit(normal);
        LIBMC_NONNULLVEC3(normal);
    }
    else {
        normal = (struct vec3){ 0, 0, 1 };
    }

    vector_like(executor, tail, delta, normal, 0, 1, &fields[4]);
}

// func Vector([config] {[main] {tail, delta}, [normal] {tail, delta, normal}},
// [color] {[main] {tag}, [stroke] {tag, stroke}, [solid] {tag, stroke, fill}})
// = native not_implemented_yet(0)
void
lib_mc_vector(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 tail, delta;
    LIBMC_VEC3(tail, 1);
    LIBMC_VEC3(delta, 2);

    struct vec3 normal;
    if (config_ind.value.doub == 1) {
        LIBMC_VEC3(normal, 3);
        normal = vec3_unit(normal);
        LIBMC_NONNULLVEC3(normal);
    }
    else {
        normal = (struct vec3){ 0, 0, 1 };
    }

    vector_like(executor, tail, delta, normal, 0, 0, &fields[4]);
}

// func Arrow([config] {[main] {tail, head}, [parameterized] {tail, head,
// normal, path_arc}}, [color] {[main] {tag}, [stroke] {tag, stroke}, [solid]
// {tag, stroke, fill}}) = native not_implemented_yet(0)
void
lib_mc_arrow(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);

    struct vec3 normal;
    float path_arc = 0;

    if (config_ind.value.doub == 1) {
        LIBMC_VEC3(normal, 3);
        normal = vec3_unit(normal);
        LIBMC_NONNULLVEC3(normal);
        LIBMC_FULL_CAST(patharc, 4, VECTOR_FIELD_TYPE_DOUBLE);
        path_arc = (float) patharc.value.doub;
        if (path_arc < -6.2 || path_arc > 6.2) {
            VECTOR_FIELD_ERROR(executor, "Invalid patharc! %f", path_arc);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }
    else {
        normal = (struct vec3){ 0, 0, 1 };
    }

    struct vec3 start, end;
    if (_start_and_end(executor, &start, &end, &fields[1]) !=
        MC_STATUS_SUCCESS) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    vector_like(
        executor, start, vec3_sub(end, start), normal, path_arc, 0, &fields[5]
    );
}

void
lib_mc_arc(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    /* probably should delete amount of duplicate code at some point */
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 center;
    LIBMC_VEC3(center, 1);
    LIBMC_FULL_CAST(radius, 2, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(theta_start, 3, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(theta_end, 4, VECTOR_FIELD_TYPE_DOUBLE);

    mc_count_t const sample_count = 40;

    struct vec3 normal;
    if (config_ind.value.doub == 1) {
        LIBMC_VEC3(normal, 5);
        normal = vec3_unit(normal);
        LIBMC_NONNULLVEC3(normal);
    }
    else {
        normal = (struct vec3){ 0, 0, 1 };
    }

    struct plane3 const plane = vec3_plane_basis(normal);

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    for (mc_ind_t i = 0; i < sample_count; ++i) {
        float const theta =
            (float) (theta_start.value.doub +
                     (theta_end.value.doub - theta_start.value.doub) * (i + 1) /
                         sample_count);
        float const cos = (float) radius.value.doub * cosf(theta);
        float const sin = (float) radius.value.doub * sinf(theta);

        struct vec3 const curr = (struct vec3){
            cos * plane.a.x + sin * plane.b.x + center.x,
            cos * plane.a.y + sin * plane.b.y + center.y,
            cos * plane.a.z + sin * plane.b.z + center.z,
        };

        if (!i) {
            float const cs = (float) radius.value.doub *
                             cosf((float) theta_start.value.doub);
            float const sn = (float) radius.value.doub *
                             sinf((float) theta_start.value.doub);

            struct vec3 const org = {
                cs * plane.a.x + sn * plane.b.x + center.x,
                cs * plane.a.y + sn * plane.b.y + center.y,
                cs * plane.a.z + sn * plane.b.z + center.z,
            };

            tetramesh_line(mesh, org, curr, normal);
        }
        else {
            tetramesh_line_to(mesh, curr);
        }
    }

    tetramesh_line_close(mesh);

    if (libmc_tag_and_color2(executor, mesh, &fields[6]) != 0) {
        return;
    }

    executor->return_register = field;
}

// func Plane([config] {[main] {normal, dist, width, height}}, [color] {[main]
// {tag}, [solid] {tag, fill}}) = native not_implemented_yet(0)
/* while really similar to rectangle right now, theoretically in future we allow
   definitions such as having two spanning vectors, which would make it
   different from a rectangle
 */
void
lib_mc_plane(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config_ind, 0, VECTOR_FIELD_TYPE_DOUBLE);
    struct vec3 normal;
    LIBMC_VEC3(normal, 1);
    LIBMC_FULL_CAST(dist, 2, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(width, 3, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(height, 4, VECTOR_FIELD_TYPE_DOUBLE);

    LIBMC_NONNULLVEC3(normal);

    struct vec3 const center = vec3_mul_scalar((float) dist.value.doub, normal);

    struct plane3 const plane = vec3_plane_basis(normal);

    float const xr = (float) width.value.doub / 2;
    float const yr = (float) height.value.doub / 2;

    /* create a tetramesh */
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    struct vec3 const a = { center.x + xr * plane.a.x + yr * plane.b.x,
                            center.y + xr * plane.a.y + yr * plane.b.y,
                            center.z + xr * plane.a.z + yr * plane.b.z };

    struct vec3 const b = {
        center.x - xr * plane.a.x + yr * plane.b.x,
        center.y - xr * plane.a.y + yr * plane.b.y,
        center.z - xr * plane.a.z + yr * plane.b.z,
    };

    struct vec3 const c = {
        center.x - xr * plane.a.x - yr * plane.b.x,
        center.y - xr * plane.a.y - yr * plane.b.y,
        center.z - xr * plane.a.z - yr * plane.b.z,
    };

    struct vec3 const d = {
        center.x + xr * plane.a.x - yr * plane.b.x,
        center.y + xr * plane.a.y - yr * plane.b.y,
        center.z + xr * plane.a.z - yr * plane.b.z,
    };

    tetramesh_line(mesh, a, b, normal);
    tetramesh_line_to(mesh, c);
    tetramesh_line_to(mesh, d);
    tetramesh_line_to(mesh, a);
    tetramesh_line_close(mesh);

    if (libmc_tag_and_color2_forceuprank(executor, mesh, &fields[5]) != 0) {
        return;
    }

    executor->return_register = field;
}
