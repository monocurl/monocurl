//
//  file_manager+encode.c
//  monocurl
//
//  Created by Manu Bhat on 11/30/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "callback.h"
#include "config.h"
#include "constructor.h"
#include "file_manager.h"
#include "interpreter.h"
#include "mc_memory.h"
#include "mc_threading.h"
#include "scene_handle.h"
#include "strutil.h"

#define MC_LOG_TAG "file_manager+encode"
#include "mc_log.h"

#define BUFFER_SIZE 2048

static mc_status_t
sync_file_write(char const *serial, char const *path)
{
    FILE *const file = fopen(path, "w");
    if (!file) {
        return MC_STATUS_FAIL;
    }

    //    mc_logn_static("save scene", "path: '%s'", path);
    mc_count_t const count = strlen(serial);
    if (fwrite(serial, 1, count, file) != count) {
        fclose(file);
        return MC_STATUS_FAIL;
    }

    if (fclose(file)) {
        return MC_STATUS_FAIL;
    }

    return 0;
}

static void
serial_slide(
    struct raw_slide_model const *slide, struct str_dynamic *str, char *buffer
)
{
    str_dynamic_append(str, "[slide name: \"");
    str_dynamic_append_esc(str, slide->title);
    str_dynamic_append(str, "\"]\n");

    char *run = slide->buffer, *next;
    while ((next = strchr(run, '\n'))) {
        *next = '\0';
        str_dynamic_append(str, "\t");
        str_dynamic_append(str, run);
        str_dynamic_append(str, "\n");
        *next = '\n'; // revert
        run = next + 1;
    }

    if (*run) {
        str_dynamic_append(str, "\t");
        str_dynamic_append(str, run);
        str_dynamic_append(str, "\n");
    }
}

static void
serial_media(
    struct raw_media_model const *media, struct str_dynamic *str, char *buffer
)
{
    str_dynamic_append(str, "[media type: \"");
    str_dynamic_append_esc(str, MEDIA_TYPES[media->type]);
    str_dynamic_append(str, "\" name: \"");
    str_dynamic_append_esc(str, media->name);
    str_dynamic_append(str, "\" handle: \"");
    str_dynamic_append_esc(str, media->os_handle);
    str_dynamic_append(str, "\"]\n");
}

static char *
serial_scene(struct raw_scene_model *scene)
{
    char buffer[BUFFER_SIZE];
    struct str_dynamic ret = str_dynamic_init();
    /* monocurl v code, no need for escaping */
    str_dynamic_append(
        &ret, "[" APP_NAME " version: \"" APP_VERSION "\" type: \"scene\"]\n\n"
    );

    /* media items */
    for (mc_ind_t i = 0; i < scene->media_count; ++i) {
        struct raw_media_model *const media = scene->media[i];
        serial_media(media, &ret, buffer);

        if (i == scene->media_count - 1) {
            str_dynamic_append(&ret, "\n");
        }
    }

    for (mc_ind_t i = 0; i < scene->slide_count; ++i) {
        struct raw_slide_model *const slide = scene->slides[i];
        serial_slide(slide, &ret, buffer);
    }

    ret.pointer[ret.offset] = '\0';

    return ret.pointer;
}

void
file_write_model(struct scene_handle *scene)
{
    mc_rwlock_reader_lock(scene->model->lock);

    char *const serial = serial_scene(scene->model);
    sync_file_write(serial, scene->path);
    mc_free(serial);

    mc_rwlock_reader_unlock(scene->model->lock);
}

mc_status_t
file_write_default_scene(char const *path)
{
    char const *const default_scene = default_scene_path();

    char *const buffer = file_read_bytes(default_scene);
    if (!buffer) {
        mc_log_errorn_static(
            "fail read default-scene", "path: '%s'", default_scene
        );
        return MC_STATUS_FAIL;
    }

    mc_status_t const ret = sync_file_write(buffer, path);
    mc_free(buffer);

    return ret;
}
