//
//  file_manager+decode.c
//  monocurl
//
//  Created by Manu Bhat on 11/30/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <ctype.h>
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "callback.h"
#include "config.h"
#include "file_manager.h"
#include "interpreter.h"
#include "mc_memory.h"
#include "scene_handle.h"
#include "strutil.h"

#define MC_LOG_TAG "file_manager+decode"
#include "mc_log.h"

#define FILE_CHUNK_SIZE 1023

/* hm not really super great... i would probably want to upgrade this into the
 * future*/

// maybe we buffer output...?
char *
file_read_bytes(char const *path)
{
    FILE *file = fopen(path, "r");
    if (!file) {
        return NULL;
    }

    char *out = NULL;
    mc_count_t bytes, written = 0;
    do {
        /* implicit NULL */
        MC_MEM_RESERVEN(out, written, FILE_CHUNK_SIZE + 1);
        bytes = fread(out + written, sizeof(char), FILE_CHUNK_SIZE, file);
        written += bytes;
    } while (bytes == FILE_CHUNK_SIZE);

    if (ferror(file)) {
        mc_free(out);
        fclose(file);
        mc_logn_static("close file", "path: '%s'", path);
        return NULL;
    }

    fclose(file);

    out[written] = '\0';
    return out;
}

struct parser_context {
    struct raw_scene_model *scene;
    char *run;
};

/* run is pointing to -> '[name ...]'*/
static mc_bool_t
try_header(struct parser_context *context, char const *name)
{
    if (*context->run != '[') {
        return 0;
    }

    char *start;
    for (start = context->run + 1; *start == ' '; ++start) {
        if (!*start) {
            return 0;
        }
    }

    char *ret, tmp;
    for (ret = start; *ret != ' '; ret++) {
        if (!*ret) {
            return 0;
        }
    }
    tmp = *ret;
    *ret = 0;

    if (strcmp(start, name)) {
        *ret = tmp;
        return 0;
    }

    context->run = ret + 1;
    return 1;
}

/* run is pointing to -> '...] */
static inline mc_bool_t
try_end_header(struct parser_context *context)
{
    char *ret;
    for (ret = context->run; *ret != ']'; ret++) {
        if (!*ret) {
            return 0;
        }
    }
    context->run = ret + 1;
    return 1;
}

/* run is pointing to -> '[space]*field:"value", quotes are escaped as \",
 * backslashes as \\ */
static mc_bool_t
try_field(struct parser_context *context, char const *name)
{
    char *start;
    for (start = context->run; *start == ' '; ++start) {
        if (!*start) {
            return 0;
        }
    }

    char *ret, tmp;
    for (ret = start; *ret != ':'; ret++) {
        if (!*ret) {
            return 0;
        }
    }
    tmp = *ret;
    *ret = 0;

    if (strcmp(start, name)) {
        *ret = tmp;
        return 0;
    }

    /* skip to first quote */
    for (ret = ret + 1; *ret != '"'; ++ret) {
        if (!*ret) {
            return 0;
        }
    }
    start = ret + 1; // start of value (inside quote)

    /* unescape contents in place */
    mc_count_t escapes = 0;
    for (ret = start; *ret != '"'; ++ret) {
        if (*ret == '\\') {
            ++escapes;
            ++ret; // skip the backslash
        }

        if (!*ret) {
            return 0;
        }

        *(ret - escapes) = *ret;
    }

    // fill in unused values with null, will be skipped over eventually
    for (mc_ind_t i = 0; i < escapes + 1; ++i) {
        *(ret - i) = '\0';
    }

    context->run = start;

    return 1;
}

static inline void
skip_max_newlines(struct parser_context *context, mc_count_t n)
{
    char *ret;
    for (ret = context->run; (mc_ind_t) (ret - context->run) < n; ++ret) {
        if (*ret != '\n') {
            break;
        }
    }
    context->run = ret;
}

/* field: ---> "value"\0\0\0 next_field: " value ", will take us to the space
 * after the nulls */
/* must be a valid field! */
static void
skip_field_value(struct parser_context *context)
{
    char *ret;
    for (ret = context->run; *ret; ++ret)
        ;

    for (; !*ret; ++ret)
        ;

    context->run = ret;
}

static mc_status_t
decode_slide(struct raw_slide_model *slide, struct parser_context *context)
{
    struct str_dynamic ret = str_dynamic_init();
    /* all depth of 1*/
    while (*context->run == '\t') {
        char *next = strchr(context->run, '\n');
        char *true_next = next ? next : strchr(context->run, '\0');
        *true_next = '\0';
        str_dynamic_append(&ret, context->run + 1);
        str_dynamic_append(&ret, "\n");
        if (!next) {
            break;
        }
        else {
            context->run = next + 1;
        }
    }

    slide->buffer = ret.pointer;
    slide->buffer[ret.offset] = 0;
    slide->buffer_size = ret.offset;

    return MC_STATUS_SUCCESS;
}

// fill in pointers essentially as well as parents
static inline void
pp_media(struct raw_media_model *media, struct raw_scene_model *parent)
{
    media->scene = parent;
}

static void
pp_slide(struct raw_slide_model *slide, struct raw_scene_model *parent)
{
    slide->scene = parent;
}

static void
post_process_scene(struct raw_scene_model *scene)
{
    for (mc_ind_t i = 0; i < scene->slide_count; i++) {
        pp_slide(scene->slides[i], scene);
    }
    for (mc_ind_t i = 0; i < scene->media_count; ++i) {
        pp_media(scene->media[i], scene);
    }
}

// if their major is different from ours, we can't compile it for sure
// if their minor is ahead of ours, we can't compile it since there might be new
// features that we don't know how to handle if their minor is below ours, we
// should be fine since theres only stuff this or less old patch is irrelevant
static mc_bool_t
compatible_version(char *comp)
{
    for (char *x = comp; *x; x++) {
        if (*x == '.') {
            *x = ' ';
        }
    }

    char *next;
    long const major = strtol(comp, &next, 10);
    long const minor = strtol(next, &next, 10);
    /* long const patch = strtol(next, NULL, 10); */

    if (major != APP_MAJOR) {
        return 0;
    }
    if (minor > APP_MINOR) {
        return 0;
    }

    return 1;
}

static mc_status_t
decode_scene(struct raw_scene_model *scene, char *string)
{
    pre_modify(NULL);

    struct parser_context context = { 0 };
    context.run = string;
    context.scene = scene;

    /* ensure vcode matches */
    if (!try_header(&context, APP_NAME)) {
        return MC_STATUS_FAIL;
    }
    else {
        if (!try_field(&context, "version")) {
            return MC_STATUS_FAIL;
        }
        else if (!compatible_version(context.run)) {
            return MC_STATUS_FAIL;
        }
        skip_field_value(&context);

        if (!try_field(&context, "type")) {
            return MC_STATUS_FAIL;
        }
        else if (strcmp(context.run, SCENE_FILE_TYPE)) {
            return MC_STATUS_FAIL;
        }
        skip_field_value(&context);

        if (!try_end_header(&context)) {
            return MC_STATUS_FAIL;
        }
    }

    skip_max_newlines(&context, INT_MAX);

    while (*context.run) {
        if (try_header(&context, "media")) {
            struct raw_media_model *const media =
                mc_calloc(1, sizeof(struct raw_media_model));
            MC_MEM_RESERVE(scene->media, scene->media_count);
            scene->media[scene->media_count++] = media;

            if (!try_field(&context, "type")) {
                return MC_STATUS_FAIL;
            }

            enum raw_media_type const type = media_type_for(context.run);
            if (type == RAW_MEDIA_UNKNOWN) {
                return MC_STATUS_FAIL;
            }
            skip_field_value(&context);

            if (!try_field(&context, "name")) {
                return MC_STATUS_FAIL;
            }

            media->type = type;
            media->name = mc_strdup(context.run);
            skip_field_value(&context);

            if (!try_field(&context, "handle")) {
                return MC_STATUS_FAIL;
            }

            media->os_handle = mc_strdup(context.run);
            media->path = path_translation(media->os_handle);
            skip_field_value(&context);

            if (!try_end_header(&context)) {
                return MC_STATUS_FAIL;
            }
        }
        else if (try_header(&context, "slide")) {
            struct raw_slide_model *const slide =
                mc_calloc(1, sizeof(struct raw_slide_model));
            if (scene->slide_count == scene->slide_capacity) {
                scene->slide_capacity =
                    MC_MEM_NEXT_CAPACITY(scene->slide_count);
                scene->slides = mc_reallocf(
                    scene->slides,
                    sizeof(struct raw_slide_model *) * scene->slide_capacity
                );
            }
            scene->slides[scene->slide_count++] = slide;

            if (!try_field(&context, "name")) {
                return MC_STATUS_FAIL;
            }
            slide->title = mc_strdup(context.run);
            skip_field_value(&context);

            if (!try_end_header(&context)) {
                return MC_STATUS_FAIL;
            }

            skip_max_newlines(&context, 1);

            if (decode_slide(slide, &context) != 0) {
                return MC_STATUS_FAIL;
            }
        }
        else {
            return MC_STATUS_FAIL;
        }

        skip_max_newlines(&context, INT_MAX);
    }

    post_process_scene(scene);

    interpreter_scene(scene, 0, 0);

    post_filewriter_modify();

    return MC_STATUS_SUCCESS;
}

// null on error
// takes ownership of path
struct scene_handle *
file_read_sync(char const *path)
{
    char *const bytes = file_read_bytes(path);
    if (!bytes) {
        mc_log_errorn_static("fail read scene-like", "path: '%s'", path);
        mc_free((char *) path);
        return NULL;
    }

    // from here, handle takes ownership of path
    struct scene_handle *const handle = init_scene_handle(path);
    if (decode_scene(handle->model, bytes) != MC_STATUS_SUCCESS) {
        mc_log_errorn_static("fail decode file", "path: '%s'", path);

        mc_free(bytes);
        scene_handle_free_no_save(handle);
        return NULL;
    }
    mc_free(bytes);

    mc_logn_static("read file", "path: '%s'", path);

    return handle;
}

struct raw_slide_model *
file_read_std(void)
{
    char const *const stdlib = std_lib_path();

    char *const bytes = file_read_bytes(stdlib);
    if (!bytes) {
        mc_log_errorn_static("fail load std lib", "path: '%s'", stdlib);
        return NULL;
    }

    struct raw_slide_model *const slide =
        mc_calloc(1, sizeof(struct raw_slide_model));
    slide->buffer = bytes;
    slide->buffer_size = strlen(bytes);

    mc_logn_static("read std lib", "slide: %p", (void *) slide);

    return slide;
}
