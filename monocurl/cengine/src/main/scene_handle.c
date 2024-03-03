//
//  scene_handle.c
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>

#include "file_manager.h"
#include "mc_time_util.h"
#include "scene_handle.h"

#define MC_LOG_TAG "scene_handle"
#include "mc_log.h"

struct scene_handle *
init_scene_handle(char const *path)
{
    struct scene_handle *handle = mc_calloc(1, sizeof(struct scene_handle));

    handle->path = path;

    handle->model = init_scene_session(handle);
    handle->viewport = init_viewport(handle);
    handle->timeline = timeline_init(handle);

    handle->last_auto_save = mc_timestamp_now();

    mc_logn("init", "", handle);

    return handle;
}

void
scene_handle_free(struct scene_handle *handle)
{
    file_write_model(handle);

    timeline_free(handle->timeline);
    viewport_free(handle->viewport);
    scene_free(handle->model);

    mc_logn("free", "", handle);

    mc_free((char *) handle->path);
    mc_free(handle);
}

void
scene_handle_free_no_save(struct scene_handle *handle)
{
    if (handle->timeline) {
        timeline_free(handle->timeline);
    }
    if (handle->viewport) {
        viewport_free(handle->viewport);
    }
    if (handle->model) {
        scene_free(handle->model);
    }

    mc_logn("free", "", handle);

    mc_free((char *) handle->path);
    mc_free(handle);
}
