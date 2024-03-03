//
//  state.c
//  Monocurl
//
//  Created by Manu Bhat on 9/21/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "file_manager.h"
#include "mc_memory.h"
#include "media.h"
#include "scene.h"
#include "slide.h"

#define MC_LOG_TAG "raw_scene_model"
#include "mc_log.h"

struct raw_scene_model *
init_scene_session(struct scene_handle *handle)
{
    struct raw_scene_model *scene_tree =
        mc_calloc(1, sizeof(struct raw_scene_model));

    scene_tree->handle = handle;
    scene_tree->lock = mc_rwlock_init();

    mc_logn("init", "", scene_tree);

    return scene_tree;
}

void
scene_free(struct raw_scene_model *scene)
{
    mc_rwlock_free(scene->lock);

    for (mc_ind_t i = 0; i < scene->slide_count; ++i) {
        slide_free(scene->slides[i]);
    }
    mc_free(scene->slides);

    for (mc_ind_t i = 0; i < scene->media_count; ++i) {
        media_free(scene->media[i]);
    }
    mc_free(scene->media);

    mc_logn("free", "", scene);

    mc_free(scene);
}
