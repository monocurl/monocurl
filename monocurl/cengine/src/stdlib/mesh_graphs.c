//
//  mesh_graphs.c
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <string.h>

#include "mesh_geometry.h"
#include "mesh_graphs.h"
#include "mesh_operators.h"
#include "mesh_tex.h"
#include "mesh_util.h"

#define DEFAULT_FUNC_SAMPLE 256
#define MAX_FUNC_SAMPLE 4096
#define DEFAULT_IMPLICIT_SAMPLE 64
#define MAX_IMPLICIT_SAMPLE 96
#define AXIS_BUFFER 0.2f
#define ZERO_ELEMENT_OFFSET 0.15f
#define DIGIT_BUFFER_SIZE 32
#define LARGE_TICK_EXTEND 0.075f
#define LARGE_TICK_WIDTH 1.5f
#define LARGE_TICK_GRID_WIDTH 2.0f
#define LARGE_TICK_OPACITY 0.7f
#define LARGE_TICK_GRID_OPACITY 0.6f
#define SMALL_TICK_EXTEND 0.05f
#define SMALL_TICK_WIDTH 1.0f
#define SMALL_TICK_OPACITY 0.6f
#define SMALL_TICK_GRID_OPACITY 0.4f

/* guaranteed success */
static struct vector_field
align(
    struct timeline_execution_context *executor, struct vector_field aux_mesh,
    struct vec3 aux_point, struct vector_field target_mesh,
    struct vec3 write_direction, struct vec3 relative, struct vec3 norm
)
{

    struct vector_field dir = vector_init(executor);
    struct vector_field aux = double_init(executor, relative.x);
    vector_plus(executor, dir, &aux);
    aux = double_init(executor, relative.y);
    vector_plus(executor, dir, &aux);
    aux = double_init(executor, relative.z);
    vector_plus(executor, dir, &aux);

    ((struct tetramesh *) aux_mesh.value.pointer)->dots[0].pos = aux_point;

    struct vector_field fields[5] = {
        double_init(executor, 0), target_mesh, VECTOR_FIELD_NULL, aux_mesh, dir,
    };
    LIBMC_SELECT_RETURN(raw, 0, return VECTOR_FIELD_NULL);
    // {0,0,1} goes to norm
    // {1,0,0} goes to write_direction
    // {0,1,0} goes to cross product of the two
    struct vec3 const jhat = vec3_cross(norm, write_direction);
    mesh_apply_matrix(raw, write_direction, jhat, norm);
    mesh_subset_free(raw);

    lib_mc_mesh_next_to(executor, VECTOR_FIELD_NULL, 5, fields);
    VECTOR_FIELD_FREE(executor, dir);
    VECTOR_FIELD_FREE(executor, target_mesh);
    return executor->return_register;
}

static void
get_grid(
    struct timeline_execution_context *executor, struct vector_field out,
    struct vec4 color, struct vec3 center, struct vec3 direction,
    struct vec3 up, struct vec3 norm, mc_bool_t actual_grid, double xmin,
    double xmax, double xstep, double xscale, float small_down, float small_up,
    float large_down, float large_up
)
{
    direction = vec3_unit(direction);
    up = vec3_unit(up);
    norm = vec3_unit(norm);

    struct vector_field large_ticks = tetramesh_init(executor);
    struct vector_field small_ticks = tetramesh_init(executor);

    struct tetramesh *large = large_ticks.value.pointer;
    struct tetramesh *smaller = small_ticks.value.pointer;
    smaller->uniform = large->uniform = STANDARD_UNIFORM;
    smaller->uniform.stroke_radius = SMALL_TICK_WIDTH;
    large->uniform.stroke_radius =
        actual_grid ? LARGE_TICK_GRID_WIDTH : LARGE_TICK_WIDTH;

    double i;
    mc_ind_t j = 0;
    for (i = 0; i >= xmin; i -= xstep) {
        --j;
    }
    for (i += xstep, ++j; i <= xmax; ++j, i += xstep) {
        struct vec3 pivot =
            vec3_add(center, vec3_mul_scalar((float) (i / xscale), direction));
        float const up_h = j % 4 == 0 ? large_up : small_up;
        struct vec3 const up_p = vec3_add(pivot, vec3_mul_scalar(up_h, up));
        float const down_h = j % 4 == 0 ? large_down : small_down;
        struct vec3 const down_p = vec3_sub(pivot, vec3_mul_scalar(down_h, up));
        struct tetramesh *mesh = j % 4 == 0 ? large : smaller;
        tetramesh_line(mesh, down_p, up_p, norm);
        tetramesh_line_close(mesh);
    }

    for (mc_ind_t k = 0; k < large->lin_count; ++k) {
        large->lins[k].a.col = large->lins[k].b.col = color;
        large->lins[k].a.col.w = large->lins[k].b.col.w =
            actual_grid ? LARGE_TICK_GRID_OPACITY : LARGE_TICK_OPACITY;
    }

    for (mc_ind_t k = 0; k < smaller->lin_count; ++k) {
        smaller->lins[k].a.col = smaller->lins[k].b.col = color;
        smaller->lins[k].a.col.w = smaller->lins[k].b.col.w =
            actual_grid ? SMALL_TICK_GRID_OPACITY : SMALL_TICK_OPACITY;
    }

    vector_plus(executor, out, &small_ticks);
    vector_plus(executor, out, &large_ticks);
}

static void
reverse_vector(struct vector_field vector)
{
    if (!vector.vtable) {
        return;
    }

    struct vector *v = vector.value.pointer;
    for (mc_ind_t i = 0; i < v->field_count / 2; ++i) {
        struct vector_field const aux = v->fields[i];
        v->fields[i] = v->fields[v->field_count - 1 - i];
        v->fields[v->field_count - 1 - i] = aux;
    }
}

/* very ugly, clean up at some point */
static struct vector_field
get_axis(
    struct timeline_execution_context *executor, struct vec3 center,
    struct vec3 direction, struct vec3 up, struct vec3 norm,
    struct vec3 write_direction, int axis, struct vector_field tag,
    struct vec4 color, struct vector_field color_field, mc_bool_t disable_ticks,
    struct vector_field *fields, float *out_min, float *out_max, float *out_step
)
{
    up = vec3_unit(up);
    write_direction = vec3_unit(write_direction);
    direction = vec3_unit(direction);
    norm = vec3_unit(norm);

    LIBMC_FULL_CAST_RETURN(
        x_spacing, 0, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
    );
    LIBMC_FULL_CAST_RETURN(
        x_scale, 1, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
    );
    LIBMC_FULL_CAST_RETURN(
        x_labels, 5, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
    );
    float const xscale = (float) x_scale.value.doub;
    float xmin, xmax, xstep;
    mc_count_t label_rate;
    if (x_scale.value.doub < 0.01) {
        VECTOR_FIELD_ERROR(executor, "x_unit too small");
        executor->return_register = VECTOR_FIELD_NULL;
        return VECTOR_FIELD_NULL;
    }
    if (x_spacing.value.doub == 0) {
        LIBMC_FULL_CAST_RETURN(
            x_rad, 2, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
        );
        xmin = -(float) x_rad.value.doub;
        xmax = (float) x_rad.value.doub;
        xstep = xscale / 4;
    }
    else {
        LIBMC_FULL_CAST_RETURN(
            x_min, 2, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
        );
        LIBMC_FULL_CAST_RETURN(
            x_max, 3, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
        );

        xmin = (float) x_min.value.doub;
        xmax = (float) x_max.value.doub;
        if (x_spacing.value.doub == 2) {
            LIBMC_FULL_CAST_RETURN(
                x_step, 4, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
            );
            xstep = (float) x_step.value.doub;
            if (xstep < 0.01 || (xmax - xmin) / xstep > 64) {
                VECTOR_FIELD_ERROR(executor, "Step size too small");
                executor->return_register = VECTOR_FIELD_NULL;
                return VECTOR_FIELD_NULL;
            }
        }
        else {
            xstep = xscale / 4;
        }
    }

    if (x_labels.value.doub == 0) {
        label_rate = 4;
    }
    else {
        LIBMC_FULL_CAST_RETURN(
            x_label_rate, 7, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
        );
        label_rate = (mc_count_t) x_label_rate.value.doub;
    }

    struct vector_field out = vector_init(executor);
    struct vector_field clear = vector_init(executor);
    struct vector_field _aux_mesh = tetramesh_init(executor);
    /* a bit hacky... */
    struct tetramesh *aux_mesh = _aux_mesh.value.pointer;
    aux_mesh->dots = mc_malloc(sizeof(*aux_mesh->dots));
    aux_mesh->dot_count = 1;
    aux_mesh->dots[0] = (struct tetra_dot){
        .pos = { 0, 0, 0 },
    };

    for (mc_ind_t i = 0; i < 4; ++i) {
        struct vector_field aux = double_init(executor, 0);
        vector_plus(executor, clear, &aux);
    }

    struct vector_field tag_arg[5] = { VECTOR_FIELD_NULL, tag,
                                       double_init(executor, 2), clear,
                                       color_field };

    struct vec3 const right_delta =
        vec3_mul_scalar(xmax / xscale + AXIS_BUFFER, direction);

    struct vec3 const left_delta =
        vec3_mul_scalar(xmin / xscale - AXIS_BUFFER, direction);

    /* labels */
    if (label_rate >= 1) {
        struct vec3 position = vec3_mul_scalar(-1, up);

        double i;
        mc_ind_t j = 0;
        for (i = 0; i >= xmin; i -= xstep) {
            --j;
        }
        for (i += xstep, ++j; i <= xmax; ++j, i += xstep) {
            if (j % label_rate == 0 && (j || axis <= 0)) {
                char digits[DIGIT_BUFFER_SIZE];
                snprintf(digits, sizeof(digits), "%g", i);
                tag_arg[0] = double_init(executor, 0.5);
                get_tex(executor, digits, tag_arg);
                if (!executor->return_register.vtable) {
                    goto free;
                }
                float const u = !j && axis == 0
                                    ? (float) (i / xscale) - ZERO_ELEMENT_OFFSET
                                    : (float) i / xscale;
                executor->return_register = align(
                    executor, _aux_mesh,
                    vec3_add(center, vec3_mul_scalar(u, direction)),
                    executor->return_register, write_direction, position, norm
                );
                vector_plus(executor, out, &executor->return_register);
            }
        }
    }

    /* ticks */
    if (!disable_ticks) {
        get_grid(
            executor, out, color, center, direction, up, norm, 0, xmin, xmax,
            xstep, xscale, SMALL_TICK_EXTEND, SMALL_TICK_EXTEND,
            LARGE_TICK_EXTEND, LARGE_TICK_EXTEND
        );
    }

    /* general label */
    tag_arg[3] = color_field;
    char const *str = vector_field_str(executor, fields[6]);

    if (!str) {
        goto free;
    }

    tag_arg[0] = double_init(executor, 0.6);
    get_tex(executor, str, tag_arg);
    mc_free((char *) str);
    if (!executor->return_register.vtable) {
        goto free;
    }
    executor->return_register = align(
        executor, _aux_mesh, vec3_add(center, right_delta),
        executor->return_register, write_direction, direction, norm
    );
    vector_plus(executor, out, &executor->return_register);

    /* left axes */
    vector_like(executor, center, right_delta, norm, 0, 0, &tag_arg[1]);
    if (!executor->return_register.vtable) {
        goto free;
    }
    vector_plus(executor, out, &executor->return_register);

    /* right axes */
    vector_like(executor, center, left_delta, norm, 0, 0, &tag_arg[1]);
    if (!executor->return_register.vtable) {
        goto free;
    }
    vector_plus(executor, out, &executor->return_register);

    *out_min = xmin / xscale;
    *out_max = xmax / xscale;
    *out_step = xstep / xscale;

    VECTOR_FIELD_FREE(executor, clear);
    VECTOR_FIELD_FREE(executor, _aux_mesh);
    return out;

free:
    VECTOR_FIELD_FREE(executor, _aux_mesh);
    VECTOR_FIELD_FREE(executor, out);
    VECTOR_FIELD_FREE(executor, clear);
    return VECTOR_FIELD_NULL;
}

void
lib_mc_axis_1d(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 center, direction, norm;
    struct vec4 color;
    LIBMC_FULL_CAST(plane, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_VEC3(center, 1);
    if (plane.value.doub == 1) {
        LIBMC_VEC3(direction, 2);
        LIBMC_VEC3(norm, 3);
    }
    else {
        direction = (struct vec3){ 1, 0, 0 };
        norm = (struct vec3){ 0, 0, 1 };
    }
    LIBMC_VEC4(color, 5);

    float aux;
    executor->return_register = get_axis(
        executor, center, direction, vec3_cross(norm, direction), norm,
        direction, -1, fields[4], color, fields[5], 0, &fields[6], &aux, &aux,
        &aux
    );
    reverse_vector(executor->return_register);
}

void
lib_mc_axis_2d(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 center, x_dir, y_dir;
    struct vec4 color;
    float x_min, x_max, x_step;
    float y_min, y_max, y_step;

    LIBMC_FULL_CAST(plane, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_VEC3(center, 1);
    if (plane.value.doub == 1) {
        LIBMC_VEC3(x_dir, 2);
        LIBMC_VEC3(y_dir, 3);
    }
    else {
        x_dir = (struct vec3){ 1, 0, 0 };
        y_dir = (struct vec3){ 0, 1, 0 };
    }
    LIBMC_VEC4(color, 5);
    LIBMC_FULL_CAST(grid, 6, VECTOR_FIELD_TYPE_DOUBLE);

    struct vector_field out = vector_init(executor);

    struct vector_field x = get_axis(
        executor, center, x_dir, y_dir, vec3_cross(x_dir, y_dir), x_dir, 0,
        fields[4], color, fields[5], grid.value.doub == 1, &fields[7], &x_min,
        &x_max, &x_step
    );

    if (!x.vtable) {
        VECTOR_FIELD_FREE(executor, out);
        return;
    }
    vector_plus(executor, out, &x);

    struct vector_field y = get_axis(
        executor, center, y_dir, x_dir, vec3_cross(x_dir, y_dir), x_dir, 1,
        fields[4], color, fields[5], grid.value.doub == 1, &fields[15], &y_min,
        &y_max, &y_step
    );
    if (!y.vtable) {
        VECTOR_FIELD_FREE(executor, out);
        return;
    }
    vector_plus(executor, out, &y);

    if (grid.value.doub == 1) {
        // add grid
        get_grid(
            executor, out, color, center, x_dir, y_dir,
            vec3_cross(x_dir, y_dir), 1, x_min, x_max, x_step, 1, -y_min, y_max,
            -y_min, y_max
        );
        get_grid(
            executor, out, color, center, y_dir, x_dir,
            vec3_cross(x_dir, y_dir), 1, y_min, y_max, y_step, 1, -x_min, x_max,
            -x_min, x_max
        );
    }

    reverse_vector(out);
    executor->return_register = out;
}

void
lib_mc_axis_3d(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 center, x_dir, y_dir, z_dir;
    struct vec4 color;
    float x_min, x_max, x_step;
    float y_min, y_max, y_step;
    float z_min, z_max, z_step;

    LIBMC_FULL_CAST(plane, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_VEC3(center, 1);
    if (plane.value.doub == 1) {
        LIBMC_VEC3(x_dir, 2);
        LIBMC_VEC3(y_dir, 3);
        LIBMC_VEC3(z_dir, 4);
    }
    else {
        x_dir = (struct vec3){ 1, 0, 0 };
        y_dir = (struct vec3){ 0, 1, 0 };
        z_dir = (struct vec3){ 0, 0, 1 };
    }
    LIBMC_VEC4(color, 6);
    LIBMC_FULL_CAST(grid, 7, VECTOR_FIELD_TYPE_DOUBLE);

    struct vector_field out = vector_init(executor);

    struct vector_field x = get_axis(
        executor, center, x_dir, y_dir, z_dir, x_dir, 0, fields[5], color,
        fields[6], grid.value.doub == 1, &fields[8], &x_min, &x_max, &x_step
    );
    if (!x.vtable) {
        VECTOR_FIELD_FREE(executor, out);
        return;
    }
    vector_plus(executor, out, &x);

    struct vector_field y = get_axis(
        executor, center, y_dir, x_dir, z_dir, x_dir, 1, fields[5], color,
        fields[6], grid.value.doub == 1, &fields[16], &y_min, &y_max, &y_step
    );
    if (!y.vtable) {
        VECTOR_FIELD_FREE(executor, out);
        return;
    }
    vector_plus(executor, out, &y);

    struct vector_field z = get_axis(
        executor, center, z_dir, x_dir, vec3_cross(x_dir, z_dir), x_dir, 2,
        fields[5], color, fields[6], grid.value.doub == 1, &fields[24], &z_min,
        &z_max, &z_step
    );
    if (!z.vtable) {
        VECTOR_FIELD_FREE(executor, out);
        return;
    }
    vector_plus(executor, out, &z);

    if (grid.value.doub == 1) {
        /* xy */
        get_grid(
            executor, out, color, center, x_dir, y_dir, z_dir, 1, x_min, x_max,
            x_step, 1, -y_min, y_max, -y_min, y_max
        );
        get_grid(
            executor, out, color, center, y_dir, x_dir, z_dir, 1, y_min, y_max,
            y_step, 1, -x_min, x_max, -x_min, x_max
        );

        /* xz */
        get_grid(
            executor, out, color, center, x_dir, z_dir, y_dir, 1, x_min, x_max,
            x_step, 1, -z_min, z_max, -z_min, z_max
        );
        get_grid(
            executor, out, color, center, z_dir, x_dir, y_dir, 1, z_min, z_max,
            z_step, 1, -x_min, x_max, -x_min, x_max
        );

        /* yz */
        get_grid(
            executor, out, color, center, y_dir, z_dir, x_dir, 1, y_min, y_max,
            y_step, 1, -z_min, z_max, -z_min, z_max
        );
        get_grid(
            executor, out, color, center, z_dir, y_dir, x_dir, 1, z_min, z_max,
            z_step, 1, -y_min, y_max, -y_min, y_max
        );
    }

    reverse_vector(out);
    executor->return_register = out;
}

void
lib_mc_parametric_func(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(domain, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(start, 1, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(stop, 2, VECTOR_FIELD_TYPE_DOUBLE);
    mc_count_t sample_count = DEFAULT_FUNC_SAMPLE;
    if (domain.value.doub == 1) {
        LIBMC_FULL_CAST(samples, 3, VECTOR_FIELD_TYPE_DOUBLE);
        sample_count = (mc_count_t) samples.value.doub;
        if (sample_count > MAX_FUNC_SAMPLE) {
            VECTOR_FIELD_ERROR(executor, "Max sample count exceeded");
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        else if (sample_count <= 1) {
            VECTOR_FIELD_ERROR(executor, "Expected at least two samples");
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }

    LIBMC_FULL_CAST(func, 4, VECTOR_FIELD_TYPE_FUNCTION);

    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;
    mesh->uniform.stroke_radius = 2;

    struct vector_field *const tags = &fields[5];

    struct vec3 first = { 0 };
    for (mc_ind_t i = 0; i < sample_count; ++i) {
        double const t = (double) i / (sample_count - 1);
        double const u = start.value.doub * (1 - t) + stop.value.doub * t;

        struct vector_field in = double_init(executor, u);
        function_call(executor, func, 1, &in);
        struct vector_field out[1] = { vector_field_extract_type(
            executor, &executor->return_register, VECTOR_FIELD_TYPE_VECTOR
        ) };
        fields = out;
        struct vec3 curr;
        LIBMC_VEC3(curr, 0);

        if (!executor->return_register.vtable) {
            VECTOR_FIELD_FREE(executor, out[0]);
            VECTOR_FIELD_FREE(executor, field);
            return;
        }

        if (i == 0) {
            first = curr;
        }
        else if (i == 1) {
            tetramesh_line(mesh, first, curr, (struct vec3){ 0, 0, 1 });
        }
        else {
            tetramesh_line_to(mesh, curr);
        }
        VECTOR_FIELD_FREE(executor, out[0]);
        executor->return_register = VECTOR_FIELD_NULL;
    }
    tetramesh_line_close(mesh);

    if (libmc_tag_and_color1(executor, mesh, tags)) {
        return;
    }

    executor->return_register = field;
}

void
lib_mc_explicit_func_diff(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec4 pos_fill, neg_fill;
    LIBMC_FULL_CAST(domain, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(start, 1, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(stop, 2, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_VEC4(pos_fill, 7);
    LIBMC_VEC4(neg_fill, 8);
    mc_count_t sample_count = DEFAULT_FUNC_SAMPLE;
    if (domain.value.doub == 1) {
        LIBMC_FULL_CAST(samples, 3, VECTOR_FIELD_TYPE_DOUBLE);
        sample_count = (mc_count_t) samples.value.doub;
        if (sample_count > MAX_FUNC_SAMPLE) {
            VECTOR_FIELD_ERROR(executor, "Max sample count exceeded");
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        else if (sample_count <= 1) {
            VECTOR_FIELD_ERROR(executor, "Expected at least two samples");
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }

    LIBMC_FULL_CAST(f, 4, VECTOR_FIELD_TYPE_FUNCTION);
    LIBMC_FULL_CAST(g, 5, VECTOR_FIELD_TYPE_FUNCTION);

    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;
    mesh->uniform.stroke_radius = 0;

    struct vector_field *const tags = &fields[6];

    float *f_vals = mc_malloc(sizeof(float) * sample_count);
    float *g_vals = mc_malloc(sizeof(float) * sample_count);

    struct vec3 first = { 0 };
    for (mc_ind_t i = 0; i < sample_count; ++i) {
        double const t = (double) i / (sample_count - 1);
        double const u = start.value.doub * (1 - t) + stop.value.doub * t;

        struct vector_field in = double_init(executor, u);
        function_call(executor, f, 1, &in);
        struct vector_field out = vector_field_extract_type(
            executor, &executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!executor->return_register.vtable) {
            VECTOR_FIELD_FREE(executor, field);
            mc_free(f_vals);
            mc_free(g_vals);
            return;
        }

        struct vec3 curr = { (float) u, (float) out.value.doub, 0 };
        f_vals[i] = (float) out.value.doub;

        if (i == 0) {
            first = curr;
        }
        else if (i == 1) {
            tetramesh_line(mesh, first, curr, (struct vec3){ 0, 0, 1 });
        }
        else {
            tetramesh_line_to(mesh, curr);
        }
    }

    for (mc_rind_t i = (mc_rind_t) sample_count - 1; i >= 0; --i) {
        double const t = (double) i / (sample_count - 1);
        double const u = start.value.doub * (1 - t) + stop.value.doub * t;

        struct vector_field in = double_init(executor, u);
        function_call(executor, g, 1, &in);
        struct vector_field out = vector_field_extract_type(
            executor, &executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!executor->return_register.vtable) {
            VECTOR_FIELD_FREE(executor, field);
            mc_free(f_vals);
            mc_free(g_vals);
            return;
        }
        g_vals[i] = (float) out.value.doub;

        tetramesh_line_to(
            mesh, (struct vec3){ (float) u, (float) out.value.doub, 0 }
        );
    }
    tetramesh_line_to(mesh, first);
    tetramesh_line_close(mesh);

    if (libmc_tag(executor, mesh, *tags)) {
        mc_free(f_vals);
        mc_free(g_vals);
        return;
    }

    if (tetramesh_uprank(mesh, 0) != MC_STATUS_SUCCESS) {
        VECTOR_FIELD_ERROR(executor, "Error upranking");
        mc_free(f_vals);
        mc_free(g_vals);
        return;
    }

    for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
        mesh->lins[i].a.col = VEC4_0;
        mesh->lins[i].b.col = VEC4_0;
    }

    for (mc_ind_t i = 0; i < mesh->tri_count; ++i) {
        float const average_x = (mesh->tris[i].a.pos.x + mesh->tris[i].b.pos.x +
                                 mesh->tris[i].c.pos.x) /
                                3;
        /* this might give the wrong value on a switching point in rare
         * circumstances? */
        mc_ind_t const ind = (mc_ind_t) round(
            (sample_count - 1) * ((double) average_x - start.value.doub) /
            (stop.value.doub - start.value.doub)
        );
        struct vec4 const col = f_vals[ind] > g_vals[ind] ? pos_fill : neg_fill;
        mesh->tris[i].a.col = col;
        mesh->tris[i].b.col = col;
        mesh->tris[i].c.col = col;
    }

    mc_free(f_vals);
    mc_free(g_vals);

    executor->return_register = field;
}

static struct vec3
find_point(
    int32_t index, mc_count_t rows, mc_count_t cols, double xmin, double ymin,
    double xmax, double ymax, double xstep, double ystep
)
{
    mc_bool_t const is_up = index % 2 == 0;
    mc_ind_t const row = (mc_count_t) (index / 2) / (cols + 2);
    mc_ind_t const col = (mc_count_t) (index / 2) % (cols + 2);

    double x, y;
    if (row == 0) {
        y = ymin;
    }
    else if (row == rows) {
        y = ymin + (rows - 1) * ystep;
    }
    else if (is_up) {
        y = ymin + ((double) row - 0.5f) * ystep;
    }
    else {
        y = ymin + ((double) row - 1.0f) * ystep;
    }

    if (col == 0) {
        x = xmin;
    }
    else if (col == cols) {
        x = xmin + (cols - 1) * xstep;
    }
    else if (is_up) {
        x = xmin + ((double) col - 1.0f) * xstep;
    }
    else {
        x = xmin + ((double) col - 0.5f) * xstep;
    }

    return (struct vec3){ (float) x, (float) y, 0 };
}

/* convention, we will form a boundary contour of all <= 0 */
void
lib_mc_implicit_func_2d(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(config, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(x_min, 1, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(x_max, 2, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(y_min, 3, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(y_max, 4, VECTOR_FIELD_TYPE_DOUBLE);
    double xmin, xmax, ymin, ymax, xstep, ystep;
    xmin = x_min.value.doub;
    xmax = x_max.value.doub;
    ymin = y_min.value.doub;
    ymax = y_max.value.doub;
    if (config.value.doub == 1) {
        LIBMC_FULL_CAST(x_step, 5, VECTOR_FIELD_TYPE_DOUBLE);
        LIBMC_FULL_CAST(y_step, 6, VECTOR_FIELD_TYPE_DOUBLE);
        xstep = x_step.value.doub;
        ystep = y_step.value.doub;
        if (xstep < (xmax - xmin) / MAX_IMPLICIT_SAMPLE) {
            VECTOR_FIELD_ERROR(executor, "Too small x_step size");
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        if (ystep < (ymax - ymin) / MAX_IMPLICIT_SAMPLE) {
            VECTOR_FIELD_ERROR(executor, "Too small y_step size");
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }
    else {
        xstep = (xmax - xmin) / DEFAULT_IMPLICIT_SAMPLE;
        ystep = (ymax - ymin) / DEFAULT_IMPLICIT_SAMPLE;
    }

    long long const rows_ = 1 + (long long) ((ymax - ymin) / ystep);
    long long const cols_ = 1 + (long long) ((xmax - xmin) / xstep);
    if (rows_ <= 0 || cols_ <= 0) {
        VECTOR_FIELD_ERROR(
            executor, "Expected at least one row and one column"
        );
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    mc_count_t const rows = (mc_count_t) rows_;
    mc_count_t const cols = (mc_count_t) cols_;

    LIBMC_FULL_CAST(func, 7, VECTOR_FIELD_TYPE_FUNCTION);

    /* r and c are valid */
#define POINT_X(c) (float) (xmin + xstep * (c))
#define POINT_Y(r) (float) (ymin + ystep * (r))

    mc_bool_t *const sign =
        mc_malloc(sizeof(mc_bool_t) * (rows + 2) * (cols + 2));

    memset(sign, 0, sizeof(mc_bool_t) * (cols + 2));
    memset(sign + (rows + 1) * (cols + 2), 0, sizeof(mc_bool_t) * (cols + 2));

    for (mc_ind_t r = 0; r < rows; ++r) {
        sign[(r + 1) * (cols + 2)] = 0;
        sign[(r + 2) * (cols + 2) - 1] = 0;

        for (mc_ind_t c = 0; c < cols; ++c) {
            struct vector_field args[2];
            args[0] = double_init(executor, POINT_X(c));
            args[1] = double_init(executor, POINT_Y(r));

            function_call(executor, func, 2, args);

            vector_field_extract_type(
                executor, &executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
            );
            if (!executor->return_register.vtable) {
                mc_free(sign);
                return;
            }

            sign[(r + 1) * (cols + 2) + c + 1] =
                executor->return_register.value.doub <= 0;
            executor->return_register = VECTOR_FIELD_NULL;
        }
    }

    // index msb: bottom left, bottom right, top right, top left
    // value msb: reverse (default is ccw from center), v (up), h (left to
    // right), bottom left, bottom right, top right, top left
#define VAL(reverse, v, h, tl, tr, bl, br)                                     \
    (reverse << 6) | (v << 5) | (h << 4) | (tl << 3) | (tr << 2) | (bl << 1) | \
        (br)
    mc_bitmask_t res[16] = {
        VAL(0, 0, 0, 0, 0, 0, 0), // 0b0000
        VAL(1, 0, 0, 0, 0, 0, 1), // 0b0001
        VAL(1, 0, 0, 0, 0, 1, 0), // 0b0010
        VAL(0, 0, 1, 0, 0, 0, 0), // 0b0011
        VAL(1, 0, 0, 0, 1, 0, 0), // 0b0100
        VAL(1, 0, 0, 0, 1, 0, 1), // 0b0101
        VAL(1, 1, 0, 0, 0, 0, 0), // 0b0110
        VAL(0, 0, 0, 1, 0, 0, 0), // 0b0111
        VAL(1, 0, 0, 1, 0, 0, 0), // 0b1000
        VAL(0, 1, 0, 0, 0, 0, 0), // 0b1001
        VAL(1, 0, 0, 1, 0, 1, 0), // 0b1010
        VAL(0, 0, 0, 0, 1, 0, 0), // 0b1011
        VAL(1, 0, 1, 0, 0, 0, 0), // 0b1100
        VAL(0, 0, 0, 0, 0, 1, 0), // 0b1101
        VAL(0, 0, 0, 0, 0, 0, 1), // 0b1110
        VAL(1, 0, 0, 0, 0, 0, 0), // 0b1111
    };
#undef VAL
    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const mesh = field.value.pointer;
    mesh->uniform = STANDARD_UNIFORM;

    // up = parity 0, right = parity 1
    int32_t *next = mc_malloc(2 * (rows + 2) * (cols + 2) * sizeof(int32_t));
    memset(next, -1, 2 * (rows + 2) * (cols + 2) * sizeof(int32_t));

    struct vec3 const norm = { 0, 0, 1 };
    for (mc_ind_t r = 0; r <= rows; ++r) {
        for (mc_ind_t c = 0; c <= cols; ++c) {
            mc_bool_t const w = sign[r * (cols + 2) + c];
            mc_bool_t const x = sign[r * (cols + 2) + c + 1];
            mc_bool_t const y = sign[(r + 1) * (cols + 2) + c + 1];
            mc_bool_t const z = sign[(r + 1) * (cols + 2) + c];
            mc_bitmask_t const mask =
                (mc_bitmask_t) ((w << 3) | (x << 2) | (y << 1) | (z << 0));
            mc_bitmask_t const result = res[mask];

            mc_count_t args[4] = {
                2 * (r * (cols + 2) + c),
                2 * (r * (cols + 2) + c) + 1,
                2 * (r * (cols + 2) + c + 1),
                2 * ((r + 1) * (cols + 2) + c) + 1,
            };

            mc_bool_t const reverse = result & (1 << 6);
            for (mc_ind_t i = 0; i < 4; ++i) {
                if (result & (1ull << (3 - i))) {
                    int32_t lin[2] = { (int32_t) args[i],
                                       (int32_t) args[(i + 1) % 4] };

                    if (reverse) {
                        int32_t const aux = lin[0];
                        lin[0] = lin[1];
                        lin[1] = aux;
                    }

                    next[lin[0]] = lin[1];
                }
            }

            for (mc_ind_t i = 0; i < 2; ++i) {
                if (result & (1 << (i + 4))) {
                    int32_t lin[2] = { (int32_t) args[i],
                                       (int32_t) args[(i + 2) % 4] };

                    if (reverse) {
                        int32_t const aux = lin[0];
                        lin[0] = lin[1];
                        lin[1] = aux;
                    }

                    next[lin[0]] = lin[1];
                }
            }
        }
    }

    for (mc_ind_t q = 0; q < 2 * (rows + 2) * (cols + 2); ++q) {
        if (next[q] < 0) {
            continue;
        }

        int32_t p = (int32_t) q;

        do {
            int32_t const n = next[p];
            if (p == (int32_t) q) {
                tetramesh_line(
                    mesh,
                    find_point(
                        p, rows, cols, xmin, ymin, xmax, ymax, xstep, ystep
                    ),
                    find_point(
                        n, rows, cols, xmin, ymin, xmax, ymax, xstep, ystep
                    ),
                    norm
                );
            }
            else {
                tetramesh_line_to(
                    mesh,
                    find_point(
                        n, rows, cols, xmin, ymin, xmax, ymax, xstep, ystep
                    )
                );
            }
            next[p] = -1;
            p = n;
        } while (p != (int32_t) q);

        tetramesh_line_close(mesh);
    }

    mc_free(sign);
    mc_free(next);
#undef POINT_X
#undef POINT_Y

    if (libmc_tag_and_color2(executor, mesh, &fields[8])) {
        return;
    }

    tetramesh_assert_invariants(mesh);

    executor->return_register = field;
}
