//
//  state.h
//  Monocurl
//
//  Created by Manu Bhat on 9/21/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_threading.h"

struct raw_slide_model;
struct raw_media_model;

struct raw_scene_model {
    mc_count_t slide_count, slide_capacity;
    struct raw_slide_model **slides;

    mc_count_t media_count;
    struct raw_media_model **media;

    struct history *history;

    struct scene_handle *handle;

    mc_rwlock_t *lock;

    mc_bool_t dirty;
};

#if MC_INTERNAL
struct raw_scene_model *
init_scene_session(struct scene_handle *handle);

void
scene_free(struct raw_scene_model *scene);
#endif
