//
//  viewport.c
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>

#include "callback.h"
#include "viewport.h"

#define MC_LOG_TAG "viewport"
#include "mc_log.h"

#define STANDARD_ASPECT_RATIO 16.0 / 9.0

static int
compare_mesh(void const *a, void const *b)
{
    struct tetramesh *const *const px = a;
    struct tetramesh *const *const py = b;

    struct tetramesh const *const x = *px;
    struct tetramesh const *const y = *py;

    double const ret = x->uniform.z_class - y->uniform.z_class;

    if (ret > 0) {
        return 1;
    }
    else if (ret < 0) {
        return -1;
    }
    else {
        return (int) x->payload - (int) y->payload;
    }
}

struct viewport *
init_viewport(struct scene_handle *handle)
{
    struct viewport *viewport = mc_calloc(1, sizeof(struct viewport));

    // buffers are not initialized in first pass
    // they're all done asynchronously...
    viewport->lock = mc_rwlock_init();
    viewport->handle = handle;
    viewport->aspect_ratio = STANDARD_ASPECT_RATIO;

    viewport->background_color = (struct vec4){ 0, 0, 0, 1 };

    viewport->camera.z_near = 0;
    viewport->camera.z_far = 1;
    viewport->camera.up = (struct vec3){ 0 };
    viewport->camera.origin = (struct vec3){ 0 };
    viewport->camera.forward = (struct vec3){ 0 };

    mc_logn("init", "", viewport);

    return viewport;
}

void
viewport_set_unordered_mesh(
    struct viewport *viewport, struct vec4 background,
    struct viewport_camera camera, struct tetramesh **mesh, mc_count_t count
)
{
    mc_rwlock_writer_lock(viewport->lock);

    for (mc_ind_t i = 0; i < viewport->mesh_count; ++i) {
        tetramesh_unref(viewport->meshes[i]);
    }
    mc_free(viewport->meshes);

    viewport->camera = camera;
    viewport->background_color = background;

    for (mc_ind_t i = 0; i < count; ++i) {
        mesh[i]->payload = i;
    }
    qsort(mesh, count, sizeof(*mesh), &compare_mesh);
    viewport->meshes = mesh;
    viewport->mesh_count = count;
    for (mc_ind_t i = 0; i < viewport->mesh_count; ++i) {
        tetramesh_ref(viewport->meshes[i]);
    }

    viewport_flush(viewport);

    mc_rwlock_writer_unlock(viewport->lock);
}

void
viewport_set_state(struct viewport *viewport, enum viewport_state state)
{
    mc_rwlock_writer_lock(viewport->lock);

    viewport->state = state;
    viewport_flush(viewport);

    mc_rwlock_writer_unlock(viewport->lock);
}

void
viewport_read_lock(struct viewport *viewport)
{
    mc_rwlock_reader_lock(viewport->lock);
}

void
viewport_read_unlock(struct viewport *viewport)
{
    mc_rwlock_reader_unlock(viewport->lock);
}

void
viewport_free(struct viewport *viewport)
{
    mc_rwlock_free(viewport->lock);

    /* only exception to rule that tetrameshes are freed on timeline thread */
    for (mc_ind_t i = 0; i < viewport->mesh_count; ++i) {
        tetramesh_unref(viewport->meshes[i]);
    }
    mc_free(viewport->meshes);

    mc_logn("free", "", viewport);
    mc_free(viewport);
}
