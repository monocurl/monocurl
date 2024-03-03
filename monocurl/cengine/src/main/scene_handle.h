//
//  scene_handle.h
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_time_util.h"
#include "timeline.h"
#include "tree.h"
#include "viewport.h"

struct scene_handle {
    struct raw_scene_model *model;

    struct timeline *timeline;

    struct viewport *viewport;

    char const *path;
    mc_timestamp_t last_auto_save;
};

struct scene_handle *
init_scene_handle(char const *path);

void
scene_handle_free(struct scene_handle *handle);

#if MC_INTERNAL
void
scene_handle_free_no_save(struct scene_handle *handle);
#endif
