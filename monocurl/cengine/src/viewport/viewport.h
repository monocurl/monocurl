//
//  viewport.h
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "geo.h"
#include "mc_env.h"
#include "mc_threading.h"

struct viewport_camera {
    // near and far are taken by some weird windows macros....
    float z_near, z_far;
    struct vec3 origin, forward, up;
};

#include "tetramesh.h"

struct viewport {
    mc_hash_t mesh_count;
    struct tetramesh **meshes;

    struct vec4 background_color;

    // camera
    struct viewport_camera camera;

    enum viewport_state {
        VIEWPORT_IDLE,
        VIEWPORT_COMPILER_ERROR,
        VIEWPORT_RUNTIME_ERROR,
        VIEWPORT_LOADING,
        VIEWPORT_PLAYING
    } state;

    mc_rwlock_t *lock;

    /* use rationals? */
    double aspect_ratio; // x / y;
    struct scene_handle *handle;
};

#if MC_INTERNAL
struct viewport *
init_viewport(struct scene_handle *handle);

void
viewport_set_unordered_mesh(
    struct viewport *viewport, struct vec4 background,
    struct viewport_camera camera, struct tetramesh **mesh, mc_count_t count
);
void
viewport_set_state(struct viewport *viewport, enum viewport_state state);

void
viewport_free(struct viewport *viewport);
#endif

void
viewport_read_lock(struct viewport *viewport);

void
viewport_read_unlock(struct viewport *viewport);
