//
//  media.c
//  Monocurl
//
//  Created by Manu Bhat on 11/7/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "callback.h"
#include "mc_memory.h"
#include "media.h"
#include "scene.h"
#include "tree.h"

char const *MEDIA_TYPES[] = { "image" };

static inline void
media_insert(struct raw_scene_model *scene, struct raw_media_model *media)
{
    pre_modify(scene);

    MC_MEM_RESERVE(scene->media, scene->media_count);
    scene->media[scene->media_count++] = media;
    media->scene = scene;

    post_scene_modify(scene, 1);
}

enum raw_media_type
media_type_for(char const *name)
{
    for (mc_ind_t i = 0; i < MEDIA_TYPES_COUNT; ++i) {
        if (!strcmp(name, MEDIA_TYPES[i])) {
            return (enum raw_media_type) i;
        }
    }

    return RAW_MEDIA_UNKNOWN;
}

struct raw_media_model
media_copy(struct raw_media_model const *media)
{
    return (struct raw_media_model){
        .name = mc_strdup(media->name),
        .path = media->path ? mc_strdup(media->path) : media->path,
        .os_handle =
            media->os_handle ? mc_strdup(media->os_handle) : media->os_handle,
        .type = media->type,
    };
}

struct raw_media_model *
media_insert_image(
    struct raw_scene_model *scene, char const *name, char const *handle
)
{
    char const *path = path_translation(handle);

    struct raw_media_model *const media =
        mc_calloc(1, sizeof(struct raw_media_model));
    *media = (struct raw_media_model){
        .name = name,
        .path = path,
        .os_handle = handle,
        .type = RAW_MEDIA_IMAGE,
        .scene = scene,
    };

    media_insert(scene, media);

    return media;
}

void
media_switch_name(struct raw_media_model *media, char *name)
{
    pre_modify(media->scene);

    mc_free((char *) media->name);
    media->name = name;

    struct raw_scene_model *const scene = media->scene;
    post_scene_modify(scene, 1);
}

void
media_switch_path(struct raw_media_model *media, char const *handle)
{
    pre_modify(media->scene);

    mc_free((char *) media->path);
    mc_free((char *) media->os_handle);
    media->os_handle = handle;
    media->path = path_translation(handle);

    struct raw_scene_model *const scene = media->scene;
    post_scene_modify(scene, 1);
}

void
media_delete(struct raw_media_model *media)
{
    pre_modify(media->scene);

    struct raw_scene_model *const scene = media->scene;

    for (mc_ind_t i = 0; i < scene->media_count; ++i) {
        if (scene->media[i] == media) {
            mc_buffer_remove(
                scene->media, sizeof(struct raw_media_model *), i,
                &scene->media_count
            );
            break;
        }
    }

    media_free(media);
    post_scene_modify(scene, 1);
}

void
media_value_free(struct raw_media_model media)
{
    mc_free((void *) media.name);
    mc_free((void *) media.path);
    mc_free((char *) media.os_handle);
}

void
media_free(struct raw_media_model *media)
{
    mc_free((void *) media->name);
    mc_free((void *) media->path);
    mc_free((char *) media->os_handle);
    mc_free(media);
}
