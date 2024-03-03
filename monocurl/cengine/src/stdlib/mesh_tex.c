//
//  mesh_tex.c
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include "mesh_tex.h"
#include "mc_file_ops.h"
#include "mc_svg.h"
#include "mesh_util.h"

#if MC_ENV_OS & MC_ENV_OS_WINDOWS
#include <Windows.h>
#include <tchar.h>
#endif

#define MAX_FILE_NAME 32
#define SAMPLE_RATE 64
#define NUMBER_BUFFER_SIZE 64
#define DEFAULT_BRACE_BUFFER 0.1f
#define DEFAULT_MEASURE_BUFFER 0.15f
#define DEFAULT_MEASURE_EXTRUSION 0.05f

#define PREAMBLE                                                               \
    "\\documentclass[preview]{standalone}\n"                                   \
    "\\usepackage{amsmath}\n"                                                  \
    "\\usepackage{amssymb}\n"                                                  \
    "\\usepackage{amsfonts}\n"                                                 \
    "\\usepackage{color}\n"                                                    \
    "\\newcommand{\\pin}[2]{{\\color[RGB]{#1, 255, 255} #2}}\n"                \
    "\\renewcommand{\\P}[2]{{\\color[RGB]{#1, 255, 255} #2}}\n"                \
    "\\newcommand{\\rowpin}[3]{{\\color[RGB]{#1, #2, 255} #3}}\n"              \
    "\\newcommand{\\RP}[3]{{\\color[RGB]{#2, #1, 255} #3}}\n"                  \
    "\\begin{document}\n"

#define POSTAMBLE                                                              \
    "\n"                                                                       \
    "\\end{document}"

static mc_status_t
write_tex(char const *content, char const *path)
{
    FILE *fp = fopen(path, "w");
    if (!fp) {
        return MC_STATUS_FAIL;
    }
    fwrite(content, sizeof(*content), strlen(content), fp);

    if (ferror(fp)) {
        fclose(fp);
        return MC_STATUS_FAIL;
    }
    fclose(fp);

    return MC_STATUS_SUCCESS;
}

static mc_status_t
to_dvi(char const *path, char const *out)
{
    char const *base = tex_binary_path();
    char const *output = tex_intermediate_path();
    if (mc_file_exists(out)) {
        return MC_STATUS_SUCCESS;
    }

    struct str_dynamic arg = str_dynamic_init();
    str_dynamic_append(&arg, "\"");
    str_dynamic_append(&arg, base);
    str_dynamic_append(&arg, "latex\"");
    str_dynamic_append(
        &arg, "  -interaction=nonstopmode -halt-on-error -output-directory=\""
    );
    str_dynamic_append(&arg, output);
#if MC_ENV_OS & MC_ENV_OS_WINDOWS
    // trailing slash causs problems?
    if (arg.pointer[arg.offset - 1] == '\\') {
        arg.offset--;
    }
#endif
    str_dynamic_append(&arg, "\" \"");
    str_dynamic_append(&arg, path);
    str_dynamic_append(&arg, "\"");

    int ret;
#if MC_ENV_OS & MC_ENV_OS_WINDOWS
    STARTUPINFO si;
    PROCESS_INFORMATION pi;

    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));
    if (CreateProcessA(
            NULL, arg.pointer, NULL, NULL, FALSE, DETACHED_PROCESS, NULL, NULL,
            &si, &pi
        )) {
        WaitForSingleObject(pi.hProcess, INFINITE);

        DWORD ec;
        GetExitCodeProcess(pi.hProcess, &ec);
        ret = (int) ec;

        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
    }
    else {
        ret = 1;
    }
#else
    ret = system(arg.pointer);
#endif

    mc_free(arg.pointer);
    return !ret ? MC_STATUS_SUCCESS : MC_STATUS_FAIL;
}

static mc_status_t
to_svg(char const *path, char const *out)
{
    if (mc_file_exists(out)) {
        return MC_STATUS_SUCCESS;
    }

    char const *base = tex_binary_path();
    struct str_dynamic arg = str_dynamic_init();
    str_dynamic_append(&arg, "\"");
    str_dynamic_append(&arg, base);
    str_dynamic_append(&arg, "dvisvgm\" \"");
    str_dynamic_append(&arg, path);
    str_dynamic_append(&arg, "\" -v 0 -n -o \"");
    str_dynamic_append(&arg, out);
    str_dynamic_append(&arg, "\"");

    int ret;
#if MC_ENV_OS & MC_ENV_OS_WINDOWS
    STARTUPINFO si;
    PROCESS_INFORMATION pi;

    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));
    if (CreateProcessA(
            NULL, arg.pointer, NULL, NULL, FALSE, DETACHED_PROCESS, NULL, NULL,
            &si, &pi
        )) {
        WaitForSingleObject(pi.hProcess, INFINITE);

        DWORD ec;
        GetExitCodeProcess(pi.hProcess, &ec);
        ret = (int) ec;

        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
    }
    else {
        ret = 1;
    }
#else
    ret = system(arg.pointer);
#endif

    mc_free(arg.pointer);
    return !ret ? MC_STATUS_SUCCESS : MC_STATUS_FAIL;
}

/* technically have this somewhere else, but this has dynamic point sampling,
 * but yes definitely should recombine them at some point TODO */
static void
dump_bezier(struct tetramesh *mesh, float *points_2d, mc_bool_t start)
{
    struct vec3 const v0 = { points_2d[0], points_2d[1], 0 };
    struct vec3 const v1 = { points_2d[2], points_2d[3], 0 };
    struct vec3 const v2 = { points_2d[4], points_2d[5], 0 };
    struct vec3 const v3 = { points_2d[6], points_2d[7], 0 };

    float const dist = vec3_norm(vec3_sub(v0, v1)) +
                       vec3_norm(vec3_sub(v1, v2)) +
                       vec3_norm(vec3_sub(v2, v3));
    mc_count_t sample_count = (mc_count_t) (dist * SAMPLE_RATE);
    if (sample_count < 2) {
        sample_count = 2;
    }

    for (mc_ind_t i = 1; i <= sample_count; ++i) {
        float const t = 1 - (float) i / sample_count;
        float const t_prime = 1 - t;

        float a = t * t * t;
        float b = 3 * t * t * t_prime;
        float c = 3 * t * t_prime * t_prime;
        float d = t_prime * t_prime * t_prime;

        struct vec3 const curr = vec3_add(
            vec3_add(vec3_mul_scalar(a, v0), vec3_mul_scalar(b, v1)),
            vec3_add(vec3_mul_scalar(c, v2), vec3_mul_scalar(d, v3))
        );

        if (i == 1 && start) {
            tetramesh_line(mesh, v0, curr, (struct vec3){ 0, 0, 1 });
        }
        else {
            tetramesh_line_to(mesh, curr);
        }
    }
}

static struct vector_field
from_svg(
    struct timeline_execution_context *executor, char const *svg_path,
    float line_height, struct vector_field *fields
)
{
    LIBMC_FULL_CAST_RETURN(
        color_type, 1, VECTOR_FIELD_TYPE_DOUBLE, return VECTOR_FIELD_NULL
    );

    struct NSVGimage *const image = nsvgParseFromFile(svg_path, "px", 96);
    if (!image) {
        return VECTOR_FIELD_NULL;
    }

    float const scale = line_height / 36.0f;

    struct vector_field ret = vector_init(executor);

    for (NSVGshape *shape = image->shapes; shape; shape = shape->next) {
        struct vector_field sub = tetramesh_init(executor);
        struct tetramesh *mesh = sub.value.pointer;
        mesh->uniform = STANDARD_UNIFORM;

        for (NSVGpath *path = shape->paths; path; path = path->next) {
            for (int i = 0; i < path->npts; ++i) {
                path->pts[2 * i] *= scale;
                path->pts[2 * i + 1] *= -scale;
            }

            for (int i = 0; i < path->npts - 3; i += 3) {
                dump_bezier(mesh, &path->pts[2 * i], i == 0);
            }

            struct vec3 const a = { path->pts[0], path->pts[1], 0 };
            tetramesh_line_to(mesh, a);
            tetramesh_line_close(mesh);
        }

        mc_bool_t const pin_tag = !fields[0].vtable;
        if (pin_tag) {
            struct vector_field tag = vector_init(executor);
            /* custom tag */
            if ((shape->fill.color & 0xFFFF00) == 0xFFFF00) {
                struct vector_field elem =
                    double_init(executor, (double) (shape->fill.color & 0xFF));
                vector_plus(executor, tag, &elem);
            }
            else if ((shape->fill.color & 0xFF0000) == 0xFF0000) {
                struct vector_field elem = double_init(
                    executor, (double) ((shape->fill.color & 0xFF00) >> 8)
                );
                vector_plus(executor, tag, &elem);
                elem =
                    double_init(executor, (double) (shape->fill.color & 0xFF));
                vector_plus(executor, tag, &elem);
            }

            fields[0] = tag;
        }

        if (libmc_tag_and_color2_forceuprank(executor, mesh, fields) !=
                MC_STATUS_SUCCESS ||
            timeline_executor_check_interrupt(executor, 1)) {
            VECTOR_FIELD_FREE(executor, ret);
            VECTOR_FIELD_FREE(executor, fields[0]);
            nsvgDelete(image);
            fields[0] = VECTOR_FIELD_NULL;
            return VECTOR_FIELD_NULL;
        }

        if (pin_tag) {
            VECTOR_FIELD_FREE(executor, fields[0]);
            fields[0] = VECTOR_FIELD_NULL;
        }

        /* manually set color*/
        mesh->uniform.stroke_radius = 0.5f;
        if (color_type.value.doub == 0) {
            for (mc_ind_t i = 0; i < mesh->lin_count; ++i) {
                mesh->lins[i].a.col.w = 0;
                mesh->lins[i].b.col.w = 0;
            }
        }

        vector_plus(executor, ret, &sub);
    }

    nsvgDelete(image);

    return ret;
}

void
get_tex(
    struct timeline_execution_context *executor, char const *str,
    struct vector_field *fields
)
{
    LIBMC_FULL_CAST(scale, 0, VECTOR_FIELD_TYPE_DOUBLE);
    if (scale.value.doub > 7) {
        VECTOR_FIELD_ERROR(executor, "Text scale too large");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    else if (scale.value.doub < 0.05) {
        VECTOR_FIELD_ERROR(executor, "Text scale too small");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    /* create tex string, file, and hash */
    struct str_dynamic tex = str_dynamic_init();
    str_dynamic_append(&tex, PREAMBLE);
    str_dynamic_append(&tex, str);
    str_dynamic_append(&tex, POSTAMBLE);

    tex.pointer[tex.offset] = 0;

    mc_hash_t const hash =
        str_null_terminated_hash((unsigned char const *) tex.pointer);

    char const *base = tex_intermediate_path();
    mc_count_t const base_len = strlen(base) + 1;

    char *tex_path = malloc(sizeof(char) * (base_len + MAX_FILE_NAME));
    char *dvi_path = malloc(sizeof(char) * (base_len + MAX_FILE_NAME));
    char *svg_path = malloc(sizeof(char) * (base_len + MAX_FILE_NAME));
    char *log_path = malloc(sizeof(char) * (base_len + MAX_FILE_NAME));
    struct vector_field ret = VECTOR_FIELD_NULL;

    snprintf(tex_path, base_len + MAX_FILE_NAME, "%s%zu.tex", base, hash);
    snprintf(dvi_path, base_len + MAX_FILE_NAME, "%s%zu.dvi", base, hash);
    snprintf(svg_path, base_len + MAX_FILE_NAME, "%s%zu.svg", base, hash);
    snprintf(log_path, base_len + MAX_FILE_NAME, "%s%zu.log", base, hash);

    if (timeline_executor_check_interrupt(executor, 0)) {
        mc_free(tex.pointer);
        goto free;
    }

    if (write_tex(tex.pointer, tex_path) != MC_STATUS_SUCCESS) {
        mc_free(tex.pointer);
        VECTOR_FIELD_ERROR(executor, "Latex error");
        executor->return_register = VECTOR_FIELD_NULL;
        goto free;
    }
    mc_free(tex.pointer);

    if (timeline_executor_check_interrupt(executor, 0)) {
        executor->return_register = VECTOR_FIELD_NULL;
        goto free;
    }

    /* convert to dvi */
    if (to_dvi(tex_path, svg_path) != MC_STATUS_SUCCESS) {
        VECTOR_FIELD_ERROR(executor, "Latex error");
        executor->return_register = VECTOR_FIELD_NULL;
        goto free;
    }

    if (timeline_executor_check_interrupt(executor, 0)) {
        executor->return_register = VECTOR_FIELD_NULL;
        goto free;
    }

    /* convert to svg */
    if (to_svg(dvi_path, svg_path) != MC_STATUS_SUCCESS) {
        VECTOR_FIELD_ERROR(executor, "Latex error");
        executor->return_register = VECTOR_FIELD_NULL;
        goto free;
    }

    if (timeline_executor_check_interrupt(executor, 0)) {
        executor->return_register = VECTOR_FIELD_NULL;
        goto free;
    }

    /* convert svg to mesh */
    ret = from_svg(executor, svg_path, (float) scale.value.doub, &fields[1]);
    if (!ret.vtable) {
        VECTOR_FIELD_FREE(executor, ret);
        goto free;
    }

    executor->return_register = ret;

free:
    if (mc_file_exists(log_path)) {
        /* ignore errors */
        remove(log_path);
    }

    mc_free(tex_path);
    mc_free(dvi_path);
    mc_free(svg_path);
    mc_free(log_path);
}

void
lib_mc_mesh_text(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    /* convert to an appropriate string */
    char const *str = vector_field_str(executor, fields[0]);
    if (!str) {
        return;
    }

    /* use pin tags */
    fields[2] = VECTOR_FIELD_NULL;
    get_tex(executor, str, &fields[1]);

    mc_free((char *) str);
}

static struct vec3
apply_trans(
    struct vec3 org, float targ_w, float src_w, float targ_cutoff,
    float src_cutoff, struct vec3 dir, struct vec3 ihat, struct vec3 delta
)
{
    float y_scale = targ_w / src_w;
    if (y_scale < 0.5f) {
        y_scale = 0.5f;
    }
    else if (y_scale > 1.25f) {
        y_scale = 1.25f;
    }
    struct vec3 const centered = { targ_w * (org.x - src_w / 2) / src_w,
                                   org.y * y_scale, org.z };
    // -^j goes to dir
    // ^i goes to {0,0,1} cross dir
    struct vec3 const rotated = vec3_add(
        vec3_mul_scalar(centered.x, ihat), vec3_mul_scalar(-centered.y, dir)
    );
    struct vec3 const translated = vec3_add(rotated, delta);

    return translated;
}
/* https://github.com/3b1b/manim/blob/d8428585f84681055fed8aa3fabfb6ae95e4a0ff/manimlib/mobject/svg/brace.py
 */
void
lib_mc_mesh_brace(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 direction;
    LIBMC_VEC3(direction, 3);
    direction = vec3_unit(direction);
    LIBMC_SELECT(mesh, 0);

    get_tex(executor, "\\[\\underbrace{\\qquad}\\]", &fields[4]);
    if (!executor->return_register.vtable) {
        mesh_subset_free(mesh);
        return;
    }

    /* guaranteed */
    struct vector_field aux[3];
    aux[0] = double_init(executor, 0);
    aux[1] = executor->return_register;
    fields = aux;
    LIBMC_SELECT(brace, 0);

    struct vec3 const right = vec3_cross((struct vec3){ 0, 0, 1 }, direction);
    struct vec3 const left = vec3_mul_scalar(-1, right);

    float const reverse_cutoff =
        mesh_direction(brace, (struct vec3){ 0, 1, 0 });
    float const reverse_width =
        mesh_direction(brace, (struct vec3){ 1, 0, 0 }) +
        mesh_direction(brace, (struct vec3){ -1, 0, 0 });

    float const cutoff = mesh_direction(mesh, direction);
    float const right_d = mesh_direction(mesh, right);
    float const left_d = mesh_direction(mesh, left);
    float const width = right_d + left_d;

    struct vec3 delta = vec3_mul_scalar(
        (reverse_cutoff + cutoff + DEFAULT_BRACE_BUFFER), direction
    );
    struct vec3 const ortho = vec3_mul_scalar((right_d - left_d) / 2, right);
    delta = vec3_add(delta, ortho);

    /* in place modification since we own it */
    for (mc_ind_t i = 0; i < brace.subset_count; ++i) {
        struct tetramesh *const curr = brace.meshes[i];
        // p -> scale * (p.x - reverse_width / 2), rotated,
        for (mc_ind_t q = 0; q < curr->tri_count; ++q) {
            curr->tris[q].a.pos = apply_trans(
                curr->tris[q].a.pos, width, reverse_width, cutoff,
                reverse_cutoff, direction, right, delta
            );
            curr->tris[q].b.pos = apply_trans(
                curr->tris[q].b.pos, width, reverse_width, cutoff,
                reverse_cutoff, direction, right, delta
            );
            curr->tris[q].c.pos = apply_trans(
                curr->tris[q].c.pos, width, reverse_width, cutoff,
                reverse_cutoff, direction, right, delta
            );
        }

        for (mc_ind_t q = 0; q < curr->lin_count; ++q) {
            curr->lins[q].a.pos = apply_trans(
                curr->lins[q].a.pos, width, reverse_width, cutoff,
                reverse_cutoff, direction, right, delta
            );
            curr->lins[q].b.pos = apply_trans(
                curr->lins[q].b.pos, width, reverse_width, cutoff,
                reverse_cutoff, direction, right, delta
            );
        }

        /* will not have dots so can drop that */
    }

    mesh_subset_free(brace);
    mesh_subset_free(mesh);
}

void
lib_mc_mesh_number(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    // get string, and then convert each individual one, then create a stack
    LIBMC_FULL_CAST(value, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(precision, 1, VECTOR_FIELD_TYPE_DOUBLE);
    if (precision.value.doub > 9 || precision.value.doub < 0) {
        VECTOR_FIELD_ERROR(executor, "Invalid precision, expected 0 to 9");
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    char format[] = "%.xf";
    format[2] = (char) ('0' + precision.value.doub);
    char buffer[NUMBER_BUFFER_SIZE];
    snprintf(buffer, sizeof(buffer), format, value.value.doub);

    get_tex(executor, buffer, &fields[2]);
}

void
lib_mc_mesh_measure(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 direction;
    LIBMC_VEC3(direction, 3);
    direction = vec3_unit(direction);
    LIBMC_SELECT(mesh, 0);

    struct vec3 norm = { 0, 0, 1 };

    struct vector_field const field = tetramesh_init(executor);
    struct tetramesh *const ret = field.value.pointer;
    ret->uniform = STANDARD_UNIFORM;

    struct vec3 const right = vec3_cross(norm, direction);
    struct vec3 const left = vec3_mul_scalar(-1, right);

    float const cutoff = mesh_direction(mesh, direction);
    float const right_d = mesh_direction(mesh, right);
    float const left_d = mesh_direction(mesh, left);

    struct vec3 forw_delta = vec3_mul_scalar(
        cutoff + DEFAULT_MEASURE_BUFFER - DEFAULT_MEASURE_EXTRUSION, direction
    );
    struct vec3 delta =
        vec3_mul_scalar(cutoff + DEFAULT_MEASURE_BUFFER, direction);
    struct vec3 back_delta = vec3_mul_scalar(
        cutoff + DEFAULT_MEASURE_BUFFER + DEFAULT_MEASURE_EXTRUSION, direction
    );

    struct vec3 const r_ortho = vec3_mul_scalar(right_d, right);
    struct vec3 const l_ortho = vec3_mul_scalar(left_d, left);

    struct vec3 const right_pivot = vec3_add(delta, r_ortho);
    struct vec3 const right_forw = vec3_add(forw_delta, r_ortho);
    struct vec3 const right_back = vec3_add(back_delta, r_ortho);

    struct vec3 const left_pivot = vec3_add(delta, l_ortho);
    struct vec3 const left_forw = vec3_add(forw_delta, l_ortho);
    struct vec3 const left_back = vec3_add(back_delta, l_ortho);

    tetramesh_line(ret, right_back, right_forw, norm);
    tetramesh_line_to(ret, right_pivot);
    tetramesh_line_to(ret, left_pivot);
    tetramesh_line_to(ret, left_forw);
    tetramesh_line_to(ret, left_back);
    tetramesh_line_to(ret, left_pivot);
    tetramesh_line_to(ret, right_pivot);
    tetramesh_line_to(ret, right_back);
    tetramesh_line_close(ret);

    if (libmc_tag_and_color1(executor, ret, &fields[4])) {
        mesh_subset_free(mesh);
        return;
    }

    mesh_subset_free(mesh);
    executor->return_register = field;
}
