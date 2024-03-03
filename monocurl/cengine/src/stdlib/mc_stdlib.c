//
//  mcstdlib.c
//  Monocurl
//
//  Created by Manu Bhat on 1/14/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include "mc_stdlib.h"
#include "mc_lib_helpers.h"
#include "mc_util.h"
#include "monocurl.h"

#include "anim_indication.h"
#include "anim_invariants.h"
#include "anim_transform.h"
#include "anim_util.h"
#include "mc_anims.h"
#include "mc_meshes.h"
#include "mesh_geometry.h"
#include "mesh_graphs.h"
#include "mesh_image.h"
#include "mesh_operators.h"
#include "mesh_tex.h"
#include "mesh_util.h"

#include "expression_tokenizer.h"
#include "lvalue.h"
#include "primitives.h"
#include "unowned_map.h"

#define MC_LOG_TAG "libmc"
#include "mc_log.h"

static void
lib_mc_unconst(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    executor->return_register = lvalue_init(executor, fields[0].value.pointer);
}

static void
lib_mc_printd(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(doub, 0, VECTOR_FIELD_TYPE_DOUBLE);
    mc_logn("printd", " %f", executor, doub.value.doub);
    executor->return_register = double_init(executor, 0);
}

#define NATIVE_ENTRY(name)                                                     \
    {                                                                          \
        #name, &lib_mc_##name                                                  \
    }

static struct monocurl_native_entry {
    char const *name;
    void (*func)(
        struct timeline_execution_context *executor, struct vector_field caller,
        mc_count_t fc, struct vector_field *fields
    );
} entries[] = {
    /* debugging symbols */
    NATIVE_ENTRY(unconst),
    NATIVE_ENTRY(printd),

    /* util */
    NATIVE_ENTRY(sort),
    NATIVE_ENTRY(left_key),
    NATIVE_ENTRY(right_key),
    NATIVE_ENTRY(reverse),
    NATIVE_ENTRY(zip),
    NATIVE_ENTRY(map),
    NATIVE_ENTRY(reduce),
    NATIVE_ENTRY(len),
    NATIVE_ENTRY(depth),
    NATIVE_ENTRY(count),
    NATIVE_ENTRY(filter),
    NATIVE_ENTRY(sum),
    NATIVE_ENTRY(product),
    NATIVE_ENTRY(all),
    NATIVE_ENTRY(any),
    NATIVE_ENTRY(map_keys),
    NATIVE_ENTRY(map_values),
    NATIVE_ENTRY(map_items),

    NATIVE_ENTRY(mean),
    NATIVE_ENTRY(std_dev),

    NATIVE_ENTRY(integrate),
    NATIVE_ENTRY(derivative),
    NATIVE_ENTRY(limit),
    NATIVE_ENTRY(ln),
    NATIVE_ENTRY(log2),
    NATIVE_ENTRY(log10),
    NATIVE_ENTRY(log),

    NATIVE_ENTRY(sin),
    NATIVE_ENTRY(cos),
    NATIVE_ENTRY(tan),
    NATIVE_ENTRY(cot),
    NATIVE_ENTRY(sec),
    NATIVE_ENTRY(csc),
    NATIVE_ENTRY(arcsin),
    NATIVE_ENTRY(arccos),
    NATIVE_ENTRY(arctan),

    NATIVE_ENTRY(factorial),
    NATIVE_ENTRY(choose),
    NATIVE_ENTRY(permute),
    NATIVE_ENTRY(gcd),
    NATIVE_ENTRY(max),
    NATIVE_ENTRY(min),
    NATIVE_ENTRY(abs),
    NATIVE_ENTRY(clamp),
    NATIVE_ENTRY(is_prime),
    NATIVE_ENTRY(sign),
    NATIVE_ENTRY(mod),
    NATIVE_ENTRY(floor),
    NATIVE_ENTRY(round),
    NATIVE_ENTRY(ceil),
    NATIVE_ENTRY(trunc),
    NATIVE_ENTRY(random),
    NATIVE_ENTRY(randint),

    NATIVE_ENTRY(norm),
    NATIVE_ENTRY(normalize),
    NATIVE_ENTRY(dot),
    NATIVE_ENTRY(cross),
    NATIVE_ENTRY(proj),
    NATIVE_ENTRY(vec_add),
    NATIVE_ENTRY(vec_mul),
    NATIVE_ENTRY(str_replace),
    NATIVE_ENTRY(reference_map),
    NATIVE_ENTRY(read_followers),
    NATIVE_ENTRY(set_followers),
    NATIVE_ENTRY(is_scene_variable),

    NATIVE_ENTRY(not_implemented_yet),

    /* interpolation */
    NATIVE_ENTRY(lerp),
    NATIVE_ENTRY(keyframe_lerp),
    NATIVE_ENTRY(smooth),
    NATIVE_ENTRY(smooth_in),
    NATIVE_ENTRY(smooth_out),

    /* meshes */
    NATIVE_ENTRY(dot_mesh),
    NATIVE_ENTRY(circle),
    NATIVE_ENTRY(annulus),
    NATIVE_ENTRY(regular_polygon),
    NATIVE_ENTRY(polygon),
    NATIVE_ENTRY(polyline),
    NATIVE_ENTRY(rect),
    NATIVE_ENTRY(triangle),
    NATIVE_ENTRY(line),
    NATIVE_ENTRY(rectangular_prism),
    NATIVE_ENTRY(sphere),
    NATIVE_ENTRY(cylinder),
    NATIVE_ENTRY(capsule),

    NATIVE_ENTRY(bezier),
    NATIVE_ENTRY(field),
    NATIVE_ENTRY(color_grid),
    NATIVE_ENTRY(vector),
    NATIVE_ENTRY(arc),
    NATIVE_ENTRY(arrow),
    NATIVE_ENTRY(half_vector),
    NATIVE_ENTRY(plane),

    NATIVE_ENTRY(parametric_func),
    NATIVE_ENTRY(explicit_func_diff),
    NATIVE_ENTRY(implicit_func_2d),
    NATIVE_ENTRY(axis_1d),
    NATIVE_ENTRY(axis_2d),
    NATIVE_ENTRY(axis_3d),

    NATIVE_ENTRY(image),

    NATIVE_ENTRY(mesh_text),
    NATIVE_ENTRY(mesh_brace),
    NATIVE_ENTRY(mesh_measure),
    NATIVE_ENTRY(mesh_number),

    NATIVE_ENTRY(mesh_contour_separated),
    NATIVE_ENTRY(mesh_contour_count),
    NATIVE_ENTRY(mesh_matched),
    NATIVE_ENTRY(mesh_dist),
    NATIVE_ENTRY(mesh_raycast),
    NATIVE_ENTRY(mesh_contains),
    NATIVE_ENTRY(mesh_left),
    NATIVE_ENTRY(mesh_right),
    NATIVE_ENTRY(mesh_up),
    NATIVE_ENTRY(mesh_down),
    NATIVE_ENTRY(mesh_forward),
    NATIVE_ENTRY(mesh_backward),
    NATIVE_ENTRY(mesh_direc),
    NATIVE_ENTRY(mesh_center),
    NATIVE_ENTRY(mesh_rank),
    NATIVE_ENTRY(mesh_uprank),
    NATIVE_ENTRY(mesh_downrank),
    NATIVE_ENTRY(mesh_select_tags),
    NATIVE_ENTRY(mesh_bend),
    NATIVE_ENTRY(mesh_lerp),
    NATIVE_ENTRY(mesh_sample),
    NATIVE_ENTRY(mesh_normal),
    NATIVE_ENTRY(mesh_tangent),
    NATIVE_ENTRY(mesh_wireframe),
    NATIVE_ENTRY(mesh_vertex_set),
    NATIVE_ENTRY(mesh_edge_set),
    NATIVE_ENTRY(mesh_triangle_set),
    NATIVE_ENTRY(mesh_hash),

    NATIVE_ENTRY(mesh_shift),
    NATIVE_ENTRY(mesh_retextured),
    NATIVE_ENTRY(mesh_rotate),
    NATIVE_ENTRY(mesh_scale),
    NATIVE_ENTRY(mesh_embed_in_space),
    NATIVE_ENTRY(mesh_project),
    NATIVE_ENTRY(mesh_faded),
    NATIVE_ENTRY(mesh_zindex),
    NATIVE_ENTRY(mesh_tag_apply),
    NATIVE_ENTRY(mesh_point_map),
    NATIVE_ENTRY(mesh_uv_map),
    NATIVE_ENTRY(mesh_color_map),
    NATIVE_ENTRY(mesh_retagged),
    NATIVE_ENTRY(mesh_tag_map),
    NATIVE_ENTRY(mesh_bounding_box),
    NATIVE_ENTRY(mesh_recolored),
    NATIVE_ENTRY(mesh_subdivided),
    NATIVE_ENTRY(mesh_line_subdivided),
    NATIVE_ENTRY(mesh_extruded),
    NATIVE_ENTRY(mesh_revolved),
    NATIVE_ENTRY(mesh_glossy),

    NATIVE_ENTRY(mesh_centered),
    NATIVE_ENTRY(mesh_to_side),
    NATIVE_ENTRY(mesh_matched_edge),
    NATIVE_ENTRY(mesh_next_to),
    NATIVE_ENTRY(mesh_stack),

    NATIVE_ENTRY(mesh_grid),
    NATIVE_ENTRY(mesh_table),

    /* anims */
    NATIVE_ENTRY(animation),

    NATIVE_ENTRY(set),

    NATIVE_ENTRY(showhide_decomp),
    NATIVE_ENTRY(grow),
    NATIVE_ENTRY(fade),
    NATIVE_ENTRY(write),

    NATIVE_ENTRY(transform),
    NATIVE_ENTRY(bend),

    NATIVE_ENTRY(lerp_anim),
    NATIVE_ENTRY(transfer),
    NATIVE_ENTRY(transfer_runtime),

    NATIVE_ENTRY(highlight),
    NATIVE_ENTRY(flash),
    /* null terminated*/
    { NULL, NULL }
};

static struct unowned_map func_map;

void
libmc_stdlib_init(void)
{
    func_map = unowned_map_init();

    for (struct monocurl_native_entry *entry = entries; entry->func; ++entry) {
        unowned_map_set(&func_map, entry->name, (void *) entry->func);
    }
}

void (*mc_find_stdlib(struct expression_tokenizer *tokenizer))(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    if (!tokenizer->entry->group->slide->is_std) {
        VECTOR_FIELD_ERROR(
            tokenizer->executor,
            "Native functions only available for libmc direct code"
        );
        return NULL;
    }

    /* can only operate on the root slide */
    char const *temporary = tokenizer_dup(tokenizer);
    void *const func = unowned_map_get(&func_map, temporary);
    mc_free((char *) temporary);

    if (!func) {
        VECTOR_FIELD_ERROR(
            tokenizer->executor, "Unable to find native function"
        );
    }

    return (void (*)(
        struct timeline_execution_context *executor, struct vector_field caller,
        mc_count_t fc, struct vector_field *fields
    )) func;
}
