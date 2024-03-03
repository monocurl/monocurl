//
//  mesh_image.c
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <string.h>

#include "callback.h"
#include "mesh_geometry.h"
#include "mesh_image.h"
#include "scene_handle.h"

void
lib_mc_image(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    char const *str = vector_field_str(executor, fields[0]);
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

    lib_mc_general_rect(executor, caller, fc - 1, &fields[1], 1);
    if (!executor->return_register.vtable) {
        return;
    }

    struct tetramesh *const mesh = executor->return_register.value.pointer;
    mesh->texture_handle = poll_texture(path);
}
