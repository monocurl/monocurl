//
//  media.h
//  Monocurl
//
//  Created by Manu Bhat on 11/7/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_env.h"
#include "mc_types.h"
#include <stdio.h>

#if MC_INTERNAL
#define MEDIA_TYPES_COUNT 1
extern char const *MEDIA_TYPES[];
#endif

struct raw_media_model {
    char const *name;
    char const *path;
    char const *os_handle;

    enum raw_media_type {
        RAW_MEDIA_IMAGE = 0,

        RAW_MEDIA_UNKNOWN = -1
    } type;

    struct raw_scene_model *scene;

    mc_bool_t dirty;
};

#if MC_INTERNAL
struct raw_media_model
media_copy(struct raw_media_model const *media);

enum raw_media_type
media_type_for(char const *name);

void
media_free(struct raw_media_model *media);
void
media_value_free(struct raw_media_model media);
#endif

struct raw_media_model *
media_insert_image(
    struct raw_scene_model *scene, char const *name, char const *handle
);

void
media_switch_name(struct raw_media_model *media, char *name);

void
media_switch_path(struct raw_media_model *media, char const *handle);

void
media_delete(struct raw_media_model *media);
