//
//  timeline_exeuction_context.c
//  monocurl
//
//  Created by Manu Bhat on 12/2/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "animation.h"
#include "callback.h"
#include "expression_tokenizer.h"
#include "file_manager.h"
#include "functor.h"
#include "lvalue.h"
#include "mc_memory.h"
#include "primitives.h"
#include "slide.h"
#include "state_converter.h"
#include "timeline.h"
#include "timeline_execution_context.h"
#include "timeline_instruction.h"

#include "mc_log.h"

#define CHECK_RATE 48

struct timeline_execution_context *
timeline_executor_init(struct timeline *timeline)
{
    struct timeline_execution_context *ret =
        mc_calloc(1, sizeof(struct timeline_execution_context));
    ret->slide_count = 1;
    ret->slides = mc_calloc(1, sizeof(struct timeline_slide));
    ret->timeline = timeline;

    ret->symbol_index_map = unowned_map_init();

    /* initialize first slide */
    // mc_logn("tl caller", "", ret);
    struct raw_slide_model *std = file_read_std();

    if (!std) {
        timeline_executor_free(ret);
        return NULL;
    }

    pre_modify(NULL);

    timeline_executor_parse(ret, 0, std, 1);
    ret->slides[0].slide = NULL;
    slide_free(std);
    post_filewriter_modify();

    if (ret->state == TIMELINE_EXECUTOR_STATE_ERROR) {
        timeline_executor_free(ret);
        return NULL;
    }

    ret->curr_slide = -1;
    timeline_executor_execute(ret, ret->slides[0].instructions, 0);

    if (ret->state == TIMELINE_EXECUTOR_STATE_ERROR) {
        timeline_executor_free(ret);
        return NULL;
    }

    ret->curr_slide = -1;
    timeline_executor_blit_cache(ret);
    return ret;
}

static void
init_media_cache(
    struct timeline_execution_context *executor, struct raw_scene_model *scene
)
{
    for (mc_ind_t i = 0; i < executor->media_count; ++i) {
        media_value_free(executor->media_cache[i]);
    }
    executor->media_cache = mc_reallocf(
        executor->media_cache,
        sizeof(struct raw_media_model) * scene->media_count
    );
    for (mc_ind_t i = 0; i < scene->media_count; ++i) {
        executor->media_cache[i] = media_copy(scene->media[i]);
    }
    executor->media_count = scene->media_count;
}

/* i is an absolute index*/
static void
clear_slide_trailing_cache(
    struct timeline_execution_context *executor, mc_ind_t i
)
{
    if (executor->slides[i].trailing_valid) {
        executor->slides[i].trailing_valid = 0;
        for (mc_ind_t j = 0; j < executor->slides[i].stack_frame; ++j) {
            VECTOR_FIELD_FREE(executor, executor->slides[i].stack[j]);
            VECTOR_FIELD_FREE(
                executor, executor->slides[i].creation_follower_stack[j]
            );
        }
        mc_free(executor->slides[i].stack);
        mc_free(executor->slides[i].follower_stack);
        mc_free(executor->slides[i].creation_follower_stack);
        mc_free(executor->slides[i].stack_jump_to);
        mc_free(executor->slides[i].creation_follower_jump_to);
        executor->slides[i].stack = NULL;
        executor->slides[i].creation_follower_stack = NULL;

        for (mc_ind_t j = 0; j < executor->slides[i].capture_count; ++j) {
            VECTOR_FIELD_FREE(executor, executor->slides[i].capture_frame[j]);
        }
        mc_free(executor->slides[i].capture_frame);
        mc_free(executor->slides[i].capture_jump_to);
        executor->slides[i].capture_frame = NULL;

        for (mc_ind_t j = 0; j < executor->slides[i].mesh_count; ++j) {
            VECTOR_FIELD_FREE(executor, executor->slides[i].meshes[j]);
        }
        mc_free(executor->slides[i].meshes);
        mc_free(executor->slides[i].mesh_jump_to);
        mc_free(executor->slides[i].mesh_hashes);
        executor->slides[i].meshes = NULL;
    }
}

void
timeline_executor_invalidate(
    struct timeline_execution_context *executor, struct raw_slide_model *slide,
    mc_bool_t modify
)
{

    mc_ind_t const index = slide_index_in_parent(slide);
    for (mc_ind_t i = executor->slide_count - 1; i > index; --i) {
        /* instructions */
        if (executor->slides[i].instructions) {
            timeline_instruction_unref(
                executor, executor->slides[i].instructions
            );
            executor->slides[i].instructions = NULL;
        }

        /* cache */
        clear_slide_trailing_cache(executor, i);
    }

    /* index is previous, confusingly */
    if (executor->slides[index].instructions) {
        while (executor->symbol_count > executor->slides[index].symbol_count) {
            timeline_executor_symbol_pop(executor, 1);
        }

        /* recompile necessary portions */
        for (mc_ind_t i = index; i < executor->slide_count - 1; ++i) {
            timeline_executor_parse(
                executor, i + 1, slide->scene->slides[i], modify
            );
            /* abort */
            if (executor->state == TIMELINE_EXECUTOR_STATE_ERROR) {
                return;
            }
        }
    }
}

// invalidates everything
void
timeline_executor_resize(
    struct timeline_execution_context *executor, struct raw_scene_model *scene,
    mc_bool_t modify
)
{
    init_media_cache(executor, scene);

    mc_rind_t last_safe = -1;
    for (mc_ind_t i = executor->slide_count - 1; i > 0; --i) {
        if (i - 1 < scene->slide_count &&
            scene->slides[i - 1]->scene_modify_safe) {
            last_safe = (mc_rind_t) i - 1;
            break;
        }

        if (executor->slides[i].instructions) {
            timeline_instruction_unref(
                executor, executor->slides[i].instructions
            );
            executor->slides[i].instructions = NULL;
        }

        clear_slide_trailing_cache(executor, i);
    }

    executor->slides = mc_reallocf(
        executor->slides,
        (scene->slide_count + 1) * sizeof(struct timeline_slide)
    );

    if (executor->slide_count < scene->slide_count + 1) {
        memset(
            &executor->slides[executor->slide_count], 0,
            (scene->slide_count + 1 - executor->slide_count) *
                sizeof(struct timeline_slide)
        );
    }

    for (mc_ind_t i = 0; i < scene->slide_count; i++) {
        mc_free((char *) executor->slides[i + 1].title);
        executor->slides[i + 1].title = mc_strdup(scene->slides[i]->title);
    }

    executor->slide_count = scene->slide_count + 1;
    if (last_safe + 1 < (mc_rind_t) scene->slide_count) {
        timeline_executor_invalidate(
            executor, scene->slides[last_safe + 1], modify
        );
    }
}

static mc_bool_t
verify_play_list(
    struct timeline_execution_context *executor, struct vector_field *curr
)
{
    struct vector_field const gen = vector_field_functor_elide(executor, curr);
    if (!gen.vtable) {
        return 0;
    }

    if (gen.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vec = gen.value.pointer;
        for (mc_ind_t i = 0; i < vec->field_count; ++i) {
            if (!verify_play_list(executor, &vec->fields[i])) {
                return 0;
            }
        }
        return 1;
    }
    else if (gen.vtable->type & VECTOR_FIELD_TYPE_ANIMATION) {
        return 1;
    }
    else {
        char buffer[VECTOR_FIELD_TYPE_STR_BUFFER];
        vector_field_type_to_a(gen.vtable->type, buffer);
        VECTOR_FIELD_ERROR(
            executor,
            "Expected `play` to be an animation tree. Found node of type %s",
            buffer
        );
        return 0;
    }
}

static mc_bool_t
mesh_is_flat(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    if (!(field.vtable->type & VECTOR_FIELD_TYPE_VECTOR)) {
        return 0;
    }
    struct vector *const vector = field.value.pointer;
    for (mc_ind_t i = 0; i < vector->field_count; ++i) {
        if (!(vector->fields[i].vtable->type & VECTOR_FIELD_TYPE_MESH)) {
            return 0;
        }
    }

    return 1;
}

#pragma message(                                                               \
    "TODO, many copies here are very expensive and can become moves instead"   \
)
static mc_status_t
mesh_flatten(
    struct timeline_execution_context *executor, struct vector_field out_vector,
    struct vector_field *curr
)
{
    struct vector_field const cast = vector_field_extract_type_message(
        executor, curr, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_MESH,
        "Invalid mesh tree. Found node of type %s, expected %s"
    );

    if (!cast.vtable) {
        return MC_STATUS_FAIL;
    }

    if (cast.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vector = cast.value.pointer;
        for (mc_ind_t i = 0; i < vector->field_count; ++i) {
            if (mesh_flatten(executor, out_vector, &vector->fields[i]) !=
                MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }
        }
        return MC_STATUS_SUCCESS;
    }
    else if (cast.vtable->type & VECTOR_FIELD_TYPE_MESH) {
        vector_plus(executor, out_vector, curr);
        return MC_STATUS_SUCCESS;
    }
    else {
        return MC_STATUS_FAIL;
    }
}

static mc_bool_t
verify_mesh(
    struct timeline_execution_context *executor, struct vector_field *curr
)
{
    if (mesh_is_flat(executor, *curr)) {
        return 1;
    }

    struct vector_field vector = vector_init(executor);
    if (mesh_flatten(executor, vector, curr) != MC_STATUS_SUCCESS) {
        return 0;
    }
    VECTOR_FIELD_FREE(executor, *curr);
    *curr = vector;

    return 1;
}

#pragma message(                                                                                                                   \
    "TODO don't hard code everything/make camera and other scene objects its own type/make a utility file for dealing with camera" \
)
static mc_bool_t
verify_scene_options(struct timeline_execution_context *executor)
{
    /* verify background */
    struct vector_field const background = vector_field_nocopy_extract_type(
        executor,
        executor->capture_frame
            [executor->follower_stack[BACKGROUND_VARIABLE_INDEX]],
        VECTOR_FIELD_TYPE_VECTOR
    );
    if (!background.vtable) {
        goto invalid_background;
    }
    struct vector *const bg_vector = background.value.pointer;
    if (bg_vector->field_count != 4) {
        goto invalid_background;
    }

    float floats[4];
    for (mc_ind_t i = 0; i < 4; ++i) {
        struct vector_field const tmp = vector_field_nocopy_extract_type(
            executor, bg_vector->fields[i], VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!tmp.vtable) {
            goto invalid_background;
        }
        floats[i] = (float) tmp.value.doub;
    }
    executor->background_cache =
        (struct vec4){ floats[0], floats[1], floats[2], floats[3] };

    struct vector_field const camera = vector_field_nocopy_extract_type(
        executor,
        executor
            ->capture_frame[executor->follower_stack[CAMERA_VARIABLE_INDEX]],
        VECTOR_FIELD_TYPE_VECTOR
    );
    if (!camera.vtable) {
        goto invalid_camera;
    }

    struct vector *const camera_vector = camera.value.pointer;
    if (camera_vector->field_count != 5) {
        goto invalid_camera;
    }

    struct vector_field const z_near = vector_field_nocopy_extract_type(
        executor, camera_vector->fields[0], VECTOR_FIELD_TYPE_DOUBLE
    );
    struct vector_field const z_far = vector_field_nocopy_extract_type(
        executor, camera_vector->fields[1], VECTOR_FIELD_TYPE_DOUBLE
    );
    struct vector_field const up = vector_field_nocopy_extract_type(
        executor, camera_vector->fields[2], VECTOR_FIELD_TYPE_VECTOR
    );
    struct vector_field const origin = vector_field_nocopy_extract_type(
        executor, camera_vector->fields[3], VECTOR_FIELD_TYPE_VECTOR
    );
    struct vector_field const forward = vector_field_nocopy_extract_type(
        executor, camera_vector->fields[4], VECTOR_FIELD_TYPE_VECTOR
    );

    if (!z_near.vtable) {
        goto invalid_camera;
    }
    if (!z_far.vtable) {
        goto invalid_camera;
    }
    if (!up.vtable) {
        goto invalid_camera;
    }

    if (!origin.vtable) {
        goto invalid_camera;
    }
    if (!forward.vtable) {
        goto invalid_camera;
    }

    executor->camera_cache.z_near = (float) z_near.value.doub;
    executor->camera_cache.z_far = (float) z_far.value.doub;

    struct vector *const origin_v = origin.value.pointer;
    struct vector *const up_v = up.value.pointer;
    struct vector *const forward_v = forward.value.pointer;

    if (origin_v->field_count != 3) {
        goto invalid_camera;
    }
    if (up_v->field_count != 3) {
        goto invalid_camera;
    }
    if (forward_v->field_count != 3) {
        goto invalid_camera;
    }

    struct vector_field const a = vector_field_nocopy_extract_type(
        executor, origin_v->fields[0], VECTOR_FIELD_TYPE_DOUBLE
    );
    struct vector_field const b = vector_field_nocopy_extract_type(
        executor, origin_v->fields[1], VECTOR_FIELD_TYPE_DOUBLE
    );
    struct vector_field const c = vector_field_nocopy_extract_type(
        executor, origin_v->fields[2], VECTOR_FIELD_TYPE_DOUBLE
    );

    struct vector_field const d = vector_field_nocopy_extract_type(
        executor, forward_v->fields[0], VECTOR_FIELD_TYPE_DOUBLE
    );
    struct vector_field const e = vector_field_nocopy_extract_type(
        executor, forward_v->fields[1], VECTOR_FIELD_TYPE_DOUBLE
    );
    struct vector_field const f = vector_field_nocopy_extract_type(
        executor, forward_v->fields[2], VECTOR_FIELD_TYPE_DOUBLE
    );

    struct vector_field const g = vector_field_nocopy_extract_type(
        executor, up_v->fields[0], VECTOR_FIELD_TYPE_DOUBLE
    );
    struct vector_field const h = vector_field_nocopy_extract_type(
        executor, up_v->fields[1], VECTOR_FIELD_TYPE_DOUBLE
    );
    struct vector_field const i = vector_field_nocopy_extract_type(
        executor, up_v->fields[2], VECTOR_FIELD_TYPE_DOUBLE
    );

    if (!a.vtable || !b.vtable || !c.vtable || !d.vtable || !e.vtable ||
        !f.vtable || !g.vtable || !h.vtable || !i.vtable) {
        goto invalid_camera;
    }

    executor->camera_cache.origin = (struct vec3){
        (float) a.value.doub,
        (float) b.value.doub,
        (float) c.value.doub,
    };
    executor->camera_cache.forward = (struct vec3){
        (float) d.value.doub,
        (float) e.value.doub,
        (float) f.value.doub,
    };
    executor->camera_cache.up = (struct vec3){
        (float) g.value.doub,
        (float) h.value.doub,
        (float) i.value.doub,
    };

    return 0;

invalid_background:
    VECTOR_FIELD_ERROR(executor, "Invalid background");
    return 1;

invalid_camera:
    VECTOR_FIELD_ERROR(executor, "Invalid camera");
    return 1;
}

static mc_status_t
did_assign(
    struct timeline_execution_context *executor, struct vector_field *old,
    struct vector_field new
)
{
    VECTOR_FIELD_FREE(executor, *old);
    *old = vector_field_lvalue_copy(executor, new);

    return MC_STATUS_SUCCESS;
}

static struct vector_field
effective_slide_stack(
    struct timeline_execution_context *executor, struct timeline_slide *slide,
    mc_ind_t i
)
{
    return executor->slides[slide->stack_jump_to[i]].stack[i];
}
static struct vector_field
effective_slide_follower_stack(
    struct timeline_execution_context *executor, struct timeline_slide *slide,
    mc_ind_t i
)
{
    return executor->slides[slide->creation_follower_jump_to[i]]
        .creation_follower_stack[i];
}

static struct vector_field
effective_slide_capture(
    struct timeline_execution_context *executor, struct timeline_slide *slide,
    mc_ind_t i
)
{
    return executor->slides[slide->capture_jump_to[i]].capture_frame[i];
}
static struct vector_field
effective_slide_mesh(
    struct timeline_execution_context *executor, struct timeline_slide *slide,
    mc_ind_t i
)
{
    return executor->slides[slide->mesh_jump_to[i]].meshes[i];
}

#pragma message("TODO lots of room for optimization!")
mc_status_t
timeline_executor_startup(
    struct timeline_execution_context *executor, mc_ind_t slide_num,
    mc_bool_t context_switch
)
{
    executor->state = TIMELINE_EXECUTOR_STATE_INITIALIZATION;

    /* bad */
    if (!executor->slides[slide_num + 1].instructions) {
        return MC_STATUS_FAIL;
    }

    /* seed random */
    srand((unsigned int) slide_num);

    /* plus 1 for offset, - 1 since we want previous*/
    struct timeline_slide *const slide = executor->slides + slide_num;

    executor->curr_seconds = 0;
    executor->curr_slide = (mc_rind_t) slide_num;

    // init registers, free old ones, clear garbage, if it exists
    if (context_switch) {
        /* clear in place */
        while (executor->stack_frame > slide->stack_frame) {
            VECTOR_FIELD_FREE(
                executor, executor->stack[executor->stack_frame - 1]
            );
            VECTOR_FIELD_FREE(
                executor,
                executor->creation_follower_stack[executor->stack_frame - 1]
            );
            executor->creation_follower_stack[executor->stack_frame - 1] =
                VECTOR_FIELD_NULL;
            executor->stack[--executor->stack_frame] = VECTOR_FIELD_NULL;
        }

        while (executor->stack_frame < slide->stack_frame) {
            executor->creation_follower_stack[executor->stack_frame] =
                VECTOR_FIELD_NULL;
            executor->stack[executor->stack_frame++] = VECTOR_FIELD_NULL;
        }

        for (mc_ind_t i = 0; i < slide->stack_frame; ++i) {
            struct vector_field comp =
                effective_slide_stack(executor, slide, i);

            if (comp.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
                VECTOR_FIELD_FREE(executor, executor->stack[i]);
                executor->stack[i] = comp;
            }
            else {
#pragma message(                                                                                                                       \
    "TODO, ig this is technically a (good) bloom filter, but there might be collisions honestly, but then that kills the point... hmm" \
)
                if (!comp.vtable) {
                    VECTOR_FIELD_FREE(executor, executor->stack[i]);
                    executor->stack[i] = VECTOR_FIELD_NULL;
                }
                else if (!executor->stack[i].vtable) {
                    executor->stack[i] =
                        vector_field_lvalue_copy(executor, comp);
                }
                else if (did_assign(executor, &executor->stack[i], comp) != MC_STATUS_SUCCESS) {
                    return MC_STATUS_FAIL;
                }
            }

            comp = effective_slide_follower_stack(executor, slide, i);

            if (!comp.vtable || comp.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
                VECTOR_FIELD_FREE(
                    executor, executor->creation_follower_stack[i]
                );
                executor->creation_follower_stack[i] = comp;
            }
            else {
                if (!comp.vtable) {
                    VECTOR_FIELD_FREE(
                        executor, executor->creation_follower_stack[i]
                    );
                    executor->creation_follower_stack[i] = VECTOR_FIELD_NULL;
                }
                else if (!executor->creation_follower_stack[i].vtable) {
                    executor->creation_follower_stack[i] =
                        vector_field_lvalue_copy(executor, comp);
                }
                else if (did_assign(executor, &executor->creation_follower_stack[i], comp) != MC_STATUS_SUCCESS) {
                    return MC_STATUS_FAIL;
                }
            }

            if (timeline_executor_check_interrupt(executor, 1)) {
                return MC_STATUS_FAIL;
            }
        }
        memcpy(
            executor->follower_stack, slide->follower_stack,
            sizeof(mc_ind_t) * slide->stack_frame
        );

        while (executor->capture_count > slide->capture_count) {
            VECTOR_FIELD_FREE(
                executor, executor->capture_frame[executor->capture_count - 1]
            );
            executor->capture_frame[--executor->capture_count] =
                VECTOR_FIELD_NULL;
        }

        while (executor->capture_count < slide->capture_count) {
            executor->capture_frame[executor->capture_count++] =
                VECTOR_FIELD_NULL;
        }

        for (mc_ind_t i = 0; i < slide->capture_count; ++i) {
            struct vector_field const comp =
                effective_slide_capture(executor, slide, i);

            if (comp.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
                VECTOR_FIELD_FREE(executor, executor->capture_frame[i]);
                executor->capture_frame[i] = comp;
            }
            else {
                if (!comp.vtable) {
                    VECTOR_FIELD_FREE(executor, executor->capture_frame[i]);
                    executor->capture_frame[i] = VECTOR_FIELD_NULL;
                }
                else if (!executor->capture_frame[i].vtable) {
                    executor->capture_frame[i] =
                        vector_field_lvalue_copy(executor, comp);
                }
                else if (did_assign(executor, &executor->capture_frame[i], comp) != MC_STATUS_SUCCESS) {
                    return MC_STATUS_FAIL;
                }
            }

            if (timeline_executor_check_interrupt(executor, 1)) {
                return MC_STATUS_FAIL;
            }
        }

        for (mc_ind_t i = 0; i < executor->mesh_count; ++i) {
            VECTOR_FIELD_FREE(executor, executor->meshes[i]);
        }
        executor->mesh_count = slide->mesh_count;
        executor->mesh_capacity = slide->mesh_count;
        executor->meshes = mc_reallocf(
            executor->meshes, sizeof(struct vector_field) * executor->mesh_count
        );
        executor->mesh_hashes = mc_reallocf(
            executor->mesh_hashes, sizeof(mc_ind_t) * executor->mesh_count
        );
        for (mc_ind_t i = 0; i < executor->mesh_count; ++i) {
            struct vector_field comp = effective_slide_mesh(executor, slide, i);
            executor->meshes[i] = vector_field_lvalue_copy(executor, comp);
            executor->mesh_hashes[i] = slide->mesh_hashes[i];
        }
    }

    if (timeline_executor_check_interrupt(executor, 1)) {
        return MC_STATUS_FAIL;
    }

    VECTOR_FIELD_FREE(executor, executor->stack[PLAY_VARIABLE_INDEX]);
    executor->stack[PLAY_VARIABLE_INDEX] = vector_init(executor);

    timeline_executor_execute(
        executor, executor->slides[slide_num + 1].instructions, 0
    );

    if (executor->curr_slide == 0) {
        /* blit scene variables to followers, special case for config slide */
#define SET(dst, val)                                                          \
    VECTOR_FIELD_FREE(executor, dst);                                          \
    dst = val
        SET(executor->capture_frame
                [executor->follower_stack[CAMERA_VARIABLE_INDEX]],
            VECTOR_FIELD_COPY(executor, executor->stack[CAMERA_VARIABLE_INDEX])
        );
        SET(executor->capture_frame
                [executor->follower_stack[BACKGROUND_VARIABLE_INDEX]],
            VECTOR_FIELD_COPY(
                executor, executor->stack[BACKGROUND_VARIABLE_INDEX]
            ));
        SET(executor->creation_follower_stack[CAMERA_VARIABLE_INDEX],
            VECTOR_FIELD_COPY(executor, executor->stack[CAMERA_VARIABLE_INDEX])
        );
        SET(executor->creation_follower_stack[BACKGROUND_VARIABLE_INDEX],
            VECTOR_FIELD_COPY(
                executor, executor->stack[BACKGROUND_VARIABLE_INDEX]
            ));
#undef SET
    }

    verify_scene_options(executor);
    if (slide_num > 0) {
        verify_play_list(executor, &executor->stack[PLAY_VARIABLE_INDEX]);
    }

    if (executor->state == TIMELINE_EXECUTOR_STATE_ERROR) {
        return MC_STATUS_FAIL;
    }

    executor->state = TIMELINE_EXECUTOR_STATE_IDLE;
    return MC_STATUS_SUCCESS;
}

static mc_bool_t
anim_sticky(
    struct timeline_execution_context *executor, struct vector_field curr
)
{
    if (curr.vtable->type & VECTOR_FIELD_TYPE_ANIMATION) {
        return 0;
    }
    else {
        struct vector *const vector = curr.value.pointer;
        if (!vector->field_count) {
            return 0;
        }
        else {
            if (vector->fields[0].vtable->type & VECTOR_FIELD_TYPE_ANIMATION) {
                struct animation *anim = vector->fields[0].value.pointer;
                return anim->sticky;
            }
            return 0;
        }
    }
}

#pragma message(                                                                                          \
    "TODO duplicate code + repeat searches across time. Build physics tree instead of leaf tree instead?" \
)
static enum animation_sentinel
anim_sentinel(
    struct timeline_execution_context *executor, struct vector_field curr
)
{
    if (curr.vtable->type & VECTOR_FIELD_TYPE_ANIMATION) {
        struct animation *const anim = curr.value.pointer;
        return anim->sentinel_state;
    }
    else {
        struct vector *const vector = curr.value.pointer;
        if (!vector->field_count) {
            return ANIMATION_SENTINEL_FORCE;
        }

        for (mc_ind_t i = 0;;) {
            mc_ind_t j;
            for (j = i + 1; j < vector->field_count; ++j) {
                if (!anim_sticky(executor, vector->fields[j])) {
                    break;
                }
            }

            mc_count_t num_playing = 0, num_waiting = 0;
            for (mc_ind_t k = i; k < j; ++k) {
                enum animation_sentinel const state =
                    anim_sentinel(executor, vector->fields[k]);

                if (state == ANIMATION_SENTINEL_FORCE) {
                    continue;
                }
                else if (state == ANIMATION_SENTINEL_IF_OTHERS) {
                    ++num_waiting;
                }
                else {
                    ++num_playing;
                }
            }

            if (num_playing) {
                return ANIMATION_PLAYING;
            }
            else if (num_waiting) {
                return ANIMATION_SENTINEL_IF_OTHERS;
            }
            else if (j < vector->field_count) {
                i = j;
            }
            else {
                return ANIMATION_SENTINEL_FORCE;
            }
        }
    }
}

/* returns if finished */
static enum animation_sentinel
sub_step(
    struct timeline_execution_context *executor, struct vector_field play,
    double dt
)
{
    /* literally go through every single animation to figure out which one we're
     * in */
    if (play.vtable->type & VECTOR_FIELD_TYPE_ANIMATION) {
        struct animation *const anim = play.value.pointer;
        if (animation_step(executor, anim, dt) != MC_STATUS_SUCCESS) {
            return ANIMATION_ERROR;
        }
        return anim->sentinel_state;
    }
    else {
        /* series and parallel decomp since it's a vector, essentially find
         * current frame */
        struct vector *const vector = play.value.pointer;
        if (!vector->field_count) {
            return ANIMATION_SENTINEL_FORCE;
        }

        for (mc_ind_t i = 0;;) {
            mc_ind_t j;
            for (j = i + 1; j < vector->field_count; ++j) {
                if (!anim_sticky(executor, vector->fields[j])) {
                    break;
                }
            }

            mc_count_t num_playing = 0;
            mc_count_t num_waiting = 0;

            for (mc_ind_t k = i; k < j; ++k) {
                enum animation_sentinel state =
                    anim_sentinel(executor, vector->fields[k]);

                if (state == ANIMATION_SENTINEL_FORCE) {
                    continue;
                }

                if (sub_step(executor, vector->fields[k], dt) ==
                    ANIMATION_ERROR) {
                    return ANIMATION_ERROR;
                }

                state = anim_sentinel(executor, vector->fields[k]);

                if (state == ANIMATION_SENTINEL_FORCE) {
                    continue;
                }
                else if (state == ANIMATION_SENTINEL_IF_OTHERS) {
                    ++num_waiting;
                }
                else {
                    ++num_playing;
                }
            }

            if (num_waiting && j < vector->field_count) {
                VECTOR_FIELD_ERROR(
                    executor, "An animation with a conditional sentinel (i.e. "
                              "a `passive` animation) must "
                              "appear at the very end of a nested sequential "
                              "animation (a passive animation "
                              "may be placed in the middle of a sequential "
                              "animation only if it is at root "
                              "level). This is most easily achieved by making "
                              "sure passive animations are "
                              "fully parallel and at the root level."
                );
                return ANIMATION_ERROR;
            }

            if (num_playing) {
                return ANIMATION_PLAYING;
            }
            else if (num_waiting) {
                return ANIMATION_SENTINEL_IF_OTHERS;
            }
            else if (j < vector->field_count) {
                i = j;
            }
            else {
                return ANIMATION_SENTINEL_FORCE;
            }
        }
    }
}

/* root level handles conditional sentinels slightly differently */
static enum animation_sentinel
really_step(
    struct timeline_execution_context *executor, struct vector_field play,
    double dt
)
{
    if (play.vtable->type & VECTOR_FIELD_TYPE_ANIMATION) {
        struct animation *const anim = play.value.pointer;
        if (animation_step(executor, anim, dt) != MC_STATUS_SUCCESS) {
            return ANIMATION_ERROR;
        }
        return anim->sentinel_state;
    }
    else {
        struct vector *const vector = play.value.pointer;
        if (!vector->field_count) {
            return ANIMATION_SENTINEL_FORCE;
        }

        mc_ind_t i = 0;
        for (;;) {
            mc_ind_t j;
            for (j = i + 1; j < vector->field_count; ++j) {
                if (!anim_sticky(executor, vector->fields[j])) {
                    break;
                }
            }

            mc_count_t num_playing = 0;
            for (mc_ind_t k = i; k < j; ++k) {
                enum animation_sentinel state =
                    anim_sentinel(executor, vector->fields[k]);
                if (state == ANIMATION_SENTINEL_FORCE) {
                    continue;
                }

                if (sub_step(executor, vector->fields[k], dt) ==
                    ANIMATION_ERROR) {
                    return ANIMATION_ERROR;
                }

                state = anim_sentinel(executor, vector->fields[k]);
                if (state != ANIMATION_SENTINEL_FORCE &&
                    state != ANIMATION_SENTINEL_IF_OTHERS) {
                    num_playing++;
                }
            }

            // after check
            if (!num_playing) {
                if (j < vector->field_count) {
                    i = j;
                    continue;
                }
                else {
                    return ANIMATION_SENTINEL_FORCE;
                }
            }

            return ANIMATION_PLAYING;
        }
    }
}

// error or positive for it finished
mc_ternary_status_t
timeline_executor_step(struct timeline_execution_context *executor, double dt)
{
    executor->state = TIMELINE_EXECUTOR_STATE_ANIMATION;
    executor->curr_seconds += dt;

    // run each animation
    enum animation_sentinel const finished =
        really_step(executor, executor->stack[PLAY_VARIABLE_INDEX], dt);

    /* update viewport... */
    verify_scene_options(executor);

    if (executor->state == TIMELINE_EXECUTOR_STATE_ERROR ||
        finished == ANIMATION_ERROR) {
        return MC_TERNARY_STATUS_FAIL;
    }
    else if (executor->curr_seconds > executor->slides[executor->curr_slide + 1].seconds) {
        executor->slides[executor->curr_slide + 1].seconds =
            executor->curr_seconds;
    }

    executor->state = TIMELINE_EXECUTOR_STATE_IDLE;

    return finished == ANIMATION_SENTINEL_FORCE ||
                   finished == ANIMATION_SENTINEL_IF_OTHERS
               ? MC_TERNARY_STATUS_FINISH
               : MC_TERNARY_STATUS_CONTINUE;
}

/* we dont need to worry about functor case because */
/* when blitting cache, we're guaranteed no errors */
static mc_bool_t
effectively_different(
    struct timeline_execution_context *executor, struct vector_field a,
    struct vector_field b, mc_bool_t *early_exit
)
{
    if (a.vtable != b.vtable) {
        return 1;
    }
    else if (!a.vtable && !b.vtable) {
        return 1;
    }

    mc_hash_t const a_hash = VECTOR_FIELD_HASH(executor, a);
    if (!a_hash) {
        *early_exit = 1;
        return 0;
    }
    mc_hash_t const b_hash = VECTOR_FIELD_HASH(executor, b);
    if (!b_hash) {
        *early_exit = 1;
        return 0;
    }

    return a_hash != b_hash;
}

/* cut down on repetition at some point */
void
timeline_executor_blit_cache(struct timeline_execution_context *executor)
{
    VECTOR_FIELD_FREE(executor, executor->stack[PLAY_VARIABLE_INDEX]);
    executor->stack[PLAY_VARIABLE_INDEX] = vector_init(executor);

    struct timeline_slide *const slide =
        executor->slides + executor->curr_slide + 1;

    clear_slide_trailing_cache(executor, (mc_ind_t) (executor->curr_slide + 1));

    slide->seconds = executor->curr_seconds;

    /* interrupted */
    mc_bool_t early_exit = 0;

    /* compilation cache is cached after compilation, this is only for runtime
     */

    /* stack */
    slide->stack_frame = 0;
    slide->stack =
        mc_malloc(sizeof(struct vector_field) * executor->stack_frame);
    slide->follower_stack = mc_malloc(sizeof(mc_ind_t) * executor->stack_frame);
    slide->stack_jump_to = mc_malloc(sizeof(mc_ind_t) * executor->stack_frame);
    slide->creation_follower_stack =
        mc_malloc(sizeof(struct vector_field) * executor->stack_frame);
    slide->creation_follower_jump_to =
        mc_malloc(sizeof(mc_ind_t) * executor->stack_frame);
    for (mc_ind_t i = 0; i < executor->stack_frame; ++i) {
        if (executor->stack[i].vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
            slide->stack[i] = executor->stack[i];
            slide->stack_jump_to[i] = (mc_ind_t) (executor->curr_slide + 1);
        }
        else {
            if (executor->curr_slide == -1 ||
                i >= executor->slides[executor->curr_slide].stack_frame ||
                effectively_different(
                    executor, executor->stack[i],
                    effective_slide_stack(
                        executor, &executor->slides[executor->curr_slide], i
                    ),
                    &early_exit
                )) {
                slide->stack[i] =
                    vector_field_lvalue_copy(executor, executor->stack[i]);
                slide->stack_jump_to[i] = (mc_ind_t) (executor->curr_slide + 1);
            }
            else {
                slide->stack_jump_to[i] =
                    executor->slides[executor->curr_slide].stack_jump_to[i];
                slide->stack[i] = VECTOR_FIELD_NULL;
            }
        }
        slide->stack_frame++;

        if (early_exit) {
            slide->creation_follower_stack[i] = VECTOR_FIELD_NULL;
            goto exit_stack;
        }

        if (!executor->creation_follower_stack[i].vtable ||
            executor->creation_follower_stack[i].vtable->type &
                VECTOR_FIELD_TYPE_LVALUE) {
            slide->creation_follower_stack[i] =
                executor->creation_follower_stack[i];
            slide->creation_follower_jump_to[i] =
                (mc_ind_t) (executor->curr_slide + 1);
        }
        else {
            if (executor->curr_slide == -1 ||
                i >= executor->slides[executor->curr_slide].stack_frame ||
                effectively_different(
                    executor, executor->creation_follower_stack[i],
                    effective_slide_follower_stack(
                        executor, &executor->slides[executor->curr_slide], i
                    ),
                    &early_exit
                )) {
                slide->creation_follower_stack[i] = vector_field_lvalue_copy(
                    executor, executor->creation_follower_stack[i]
                );
                slide->creation_follower_jump_to[i] =
                    (mc_ind_t) (executor->curr_slide + 1);
            }
            else {
                slide->creation_follower_jump_to[i] =
                    executor->slides[executor->curr_slide].stack_jump_to[i];
                slide->creation_follower_stack[i] = VECTOR_FIELD_NULL;
            }
        }

        if (early_exit) {
            goto exit_stack;
        }
    }
    memcpy(
        slide->follower_stack, executor->follower_stack,
        sizeof(mc_ind_t) * executor->stack_frame
    );

    /* captures */
    slide->capture_count = 0;
    slide->capture_frame =
        mc_malloc(sizeof(struct vector_field) * executor->capture_count);
    slide->capture_jump_to =
        mc_malloc(sizeof(mc_ind_t) * executor->capture_count);
    for (mc_ind_t i = 0; i < executor->capture_count; ++i) {
        if (executor->curr_slide == -1 ||
            i >= executor->slides[executor->curr_slide].capture_count ||
            effectively_different(
                executor, executor->capture_frame[i],
                effective_slide_capture(
                    executor, &executor->slides[executor->curr_slide], i
                ),
                &early_exit
            )) {
            slide->capture_frame[i] =
                vector_field_lvalue_copy(executor, executor->capture_frame[i]);
            slide->capture_jump_to[i] = (mc_ind_t) (executor->curr_slide + 1);
        }
        else {
            slide->capture_jump_to[i] =
                executor->slides[executor->curr_slide].capture_jump_to[i];
            slide->capture_frame[i] = VECTOR_FIELD_NULL;
        }

        slide->capture_count++;

        if (early_exit) {
            goto exit_capture;
        }
    }

    /* meshes*/
    slide->mesh_count = 0;
    slide->meshes =
        mc_malloc(sizeof(struct vector_field) * executor->mesh_count);
    slide->mesh_hashes = mc_malloc(sizeof(mc_ind_t) * executor->mesh_count);
    slide->mesh_jump_to = mc_malloc(sizeof(mc_ind_t) * executor->mesh_count);

    for (mc_ind_t i = 0; i < executor->mesh_count; ++i) {
        if (executor->curr_slide == -1 ||
            i >= executor->slides[executor->curr_slide].mesh_count ||
            effectively_different(
                executor, executor->meshes[i],
                effective_slide_mesh(
                    executor, &executor->slides[executor->curr_slide], i
                ),
                &early_exit
            )) {
            slide->meshes[i] =
                vector_field_lvalue_copy(executor, executor->meshes[i]);
            slide->mesh_jump_to[i] = (mc_ind_t) (executor->curr_slide + 1);
        }
        else {
            slide->mesh_jump_to[i] =
                executor->slides[executor->curr_slide].mesh_jump_to[i];
            slide->meshes[i] = VECTOR_FIELD_NULL;
        }
        slide->mesh_hashes[i] = executor->mesh_hashes[i];
        slide->mesh_count++;

        if (early_exit) {
            goto exit_mesh;
        }
    }

    slide->trailing_valid = 1;
    return;

exit_mesh:
    for (mc_ind_t j = 0; j < slide->mesh_count; ++j) {
        VECTOR_FIELD_FREE(executor, slide->meshes[j]);
    }
    mc_free(slide->meshes);
    mc_free(slide->mesh_jump_to);
    mc_free(slide->mesh_hashes);
    slide->meshes = NULL;

exit_capture:
    for (mc_ind_t j = 0; j < slide->capture_count; ++j) {
        VECTOR_FIELD_FREE(executor, slide->capture_frame[j]);
    }
    mc_free(slide->capture_frame);
    mc_free(slide->capture_jump_to);
    slide->capture_frame = NULL;

exit_stack:
    for (mc_ind_t j = 0; j < slide->stack_frame; ++j) {
        VECTOR_FIELD_FREE(executor, slide->stack[j]);
        VECTOR_FIELD_FREE(executor, slide->creation_follower_stack[j]);
    }
    mc_free(slide->stack);
    mc_free(slide->creation_follower_stack);
    mc_free(slide->stack_jump_to);
    mc_free(slide->creation_follower_jump_to);
    slide->stack = NULL;
}

void
timeline_executor_report_error(struct timeline_execution_context *executor, ...)
{
    if (executor->state == TIMELINE_EXECUTOR_STATE_ERROR) {

        return;
    }

    va_list args, args_copy;
    va_start(args, executor);
    char const *const error = va_arg(args, char const *);
    va_copy(args_copy, args);

    struct slide_error dump;
    dump.type = executor->state == TIMELINE_EXECUTOR_STATE_COMPILING
                    ? SLIDE_ERROR_SYNTAX
                    : SLIDE_ERROR_RUNTIME;
    dump.line = executor->execution_line;

    char buffer[1];
    int const len = vsnprintf(buffer, 1, error, args);
    if (len >= 0) {
        dump.message = mc_malloc((mc_count_t) len + 1);
        vsprintf(dump.message, error, args_copy);
    }
    else {
        char const *const src = "Unable to format error";
        dump.message = mc_malloc(strlen(src) + 1);
        strcpy(dump.message, src);
    }

    va_end(args);
    va_end(args_copy);

    /* propagate error */
    if (!executor->execution_slide) {
        executor->execution_slide =
            executor->slides[executor->curr_slide + 1].slide;
        executor->execution_line = 0;
    }
    slide_write_error(
        executor->execution_slide, dump,
        executor->state == TIMELINE_EXECUTOR_STATE_COMPILING
    );

    executor->execution_slide = NULL;
    executor->execution_line = 0;

    executor->state = TIMELINE_EXECUTOR_STATE_ERROR;
}

struct vector_field *
timeline_get_follower(
    struct timeline_execution_context *executor, struct vector_field *ptr
)
{
    intptr_t const res = (intptr_t) ptr;
    int const delta = (int) (res - (intptr_t) executor->stack);

    if (delta < 0 ||
        delta >= (int) sizeof(executor->stack[0]) * MAX_STACK_FRAME) {
        return NULL;
    }
    else {
        mc_ind_t const index = (mc_ind_t) delta / sizeof(executor->stack[0]);
        if (executor->follower_stack[index] == SIZE_MAX) {
            return NULL;
        }
        return &executor->capture_frame[executor->follower_stack[index]];
    }
}

mc_status_t
timeline_executor_ref_capture(
    struct timeline_execution_context *executor, mc_ind_t index
)
{
    if (executor->capture_count == MAX_CAPTURES) {
        VECTOR_FIELD_ERROR(executor, "Capture limit exceeded");
        return MC_STATUS_FAIL;
    }

    executor->follower_stack[index] = executor->capture_count;
    executor->capture_frame[executor->capture_count++] = vector_init(executor);
    executor->creation_follower_stack[index] = vector_init(executor);

    return MC_STATUS_SUCCESS;
}

mc_bool_t
timeline_executor_check_interrupt(
    struct timeline_execution_context *executor, mc_bool_t force
)
{
    if (executor->byte_alloc >= MAX_HEAP) {
        // we don't really distringuish between errors and interrupts
        // both are handled by an 'exit' of simulation
        VECTOR_FIELD_ERROR(executor, "Memory limit exceeded");
        return 1;
    }

    if (++executor->check_nonce >= CHECK_RATE - 1 || force) {
        executor->check_nonce = 0;

        struct timeline *const timeline = executor->timeline;

        mc_rwlock_writer_unlock(timeline->state_lock);
        mc_rwlock_writer_lock(timeline->state_lock);
        if (timeline->tasks->count > 1) {
            timeline->executor->state = TIMELINE_EXECUTOR_STATE_ERROR;
            return 1;
        }
    }

    return 0;
}

void
timeline_executor_pre_interrupt(struct timeline_execution_context *executor)
{
    executor->check_nonce = 0;

    struct timeline *const timeline = executor->timeline;

    mc_rwlock_writer_unlock(timeline->state_lock);
}

mc_bool_t
timeline_executor_post_interrupt(struct timeline_execution_context *executor)
{
    struct timeline *const timeline = executor->timeline;

    mc_rwlock_writer_lock(timeline->state_lock);

    if (timeline->tasks->count > 1) {
        timeline->executor->state = TIMELINE_EXECUTOR_STATE_ERROR;
        return 1;
    }

    return 0;
}

struct vector_field *
timeline_executor_temporary_push(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    /* always push it to make sure that */
    executor->temporaries_stack[executor->tmp_count++] = field;

    if (executor->tmp_count >= MAX_TEMPORARIES) {
        VECTOR_FIELD_ERROR(executor, "Recursion limit reached");
        return NULL;
    }

    return &executor->temporaries_stack[executor->tmp_count - 1];
}

struct vector_field *
timeline_executor_var_push(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    if (executor->stack_frame >= MAX_STACK_FRAME) {
        VECTOR_FIELD_ERROR(executor, "Recursion limit reached");
        return NULL;
    }

    executor->stack[executor->stack_frame] = field;
    executor->creation_follower_stack[executor->stack_frame] =
        VECTOR_FIELD_NULL;
    /* declare no follower right now */
    executor->follower_stack[executor->stack_frame] = SIZE_MAX;
    return &executor->stack[executor->stack_frame++];
}

static void
prune(
    struct timeline_execution_context *executor, struct vector_field *curr,
    mc_bool_t out_of_frame
)
{
    if (!curr->vtable) {
        executor->return_register = VECTOR_FIELD_NULL;
        VECTOR_FIELD_ERROR(executor, "Uninitialized data");
        return;
    }

    if (curr->vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        if (curr->vtable->out_of_frame_like) {
            if (curr->vtable != &derived_persistent_vtable) {
                struct vector_field const next =
                    *(struct vector_field *) curr->value.pointer;
                if (next.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
                    *curr = next;
                }
                else {
                    *curr = VECTOR_FIELD_COPY(executor, next);
                }
            }
        }
        else if (!out_of_frame) {
            *curr = VECTOR_FIELD_COPY(
                executor, *(struct vector_field *) curr->value.pointer
            );
        }
    }
    else if (curr->vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vector = curr->value.pointer;
        for (mc_ind_t i = 0; i < vector->field_count; ++i) {
            prune(executor, &vector->fields[i], out_of_frame);
        }
    }
}

void
timeline_executor_prune_register(struct timeline_execution_context *executor)
{
    prune(executor, &executor->return_register, 0);
}

struct vector_field
timeline_executor_execute(
    struct timeline_execution_context *executor,
    struct timeline_instruction *instruction, mc_bool_t prune
)
{
    if (executor->func_depth >= MAX_FUNCTION_CALL) {
        VECTOR_FIELD_ERROR(executor, "Max recursion depth exceeded!");
        VECTOR_FIELD_FREE(executor, executor->return_register);
        executor->return_register = VECTOR_FIELD_NULL;
        return VECTOR_FIELD_NULL;
    }

    if (executor->curr_slide != -1 &&
        timeline_executor_check_interrupt(executor, 0)) {
        VECTOR_FIELD_FREE(executor, executor->return_register);
        executor->return_register = VECTOR_FIELD_NULL;
        return VECTOR_FIELD_NULL;
    }

    mc_count_t const org_temporaries = executor->tmp_count;
    mc_count_t org = executor->stack_frame;
    unsigned int var_count = 0;

    int unravel = (int) executor->stack_depth;
    executor->stack_depth++;
    executor->func_depth++;

    struct raw_slide_model *const model = executor->execution_slide;
    mc_count_t line = executor->execution_line;

    do {
        if (executor->curr_slide != -1 &&
            instruction->slide ==
                executor->slides[executor->curr_slide + 1].slide) {
            executor->execution_slide = instruction->slide;
            executor->execution_line = instruction->line_no;
        }

        VECTOR_FIELD_FREE(executor, executor->return_register);
        executor->return_register = VECTOR_FIELD_NULL;
        /* increase stack size */
        var_count += instruction->var_count;
        for (mc_ind_t i = 0; i < instruction->var_count; ++i) {
            if (!timeline_executor_var_push(executor, VECTOR_FIELD_NULL)) {
                goto free;
            }
        }

        if (!(executor->return_register =
                  timeline_instruction_full_execute(instruction, executor))
                 .vtable) {
            goto free;
        }
    } while ((instruction = instruction->next));

    if (!unravel) {
        org = executor->stack_frame;
    }

    /* if it's pointing to the current stack frame, then unwrap it */
    /* map, vectors, and functors handle heap memory cases */
    if (prune) {
        timeline_executor_prune_register(executor);
    }
    else if (executor->return_register.vtable) {
        VECTOR_FIELD_FREE(executor, executor->return_register);
        executor->return_register = double_init(executor, 0);
    }

free:
    executor->execution_slide = model;
    executor->execution_line = line;
    executor->stack_depth--;
    executor->func_depth--;

    while (executor->tmp_count > org_temporaries) {
        --executor->tmp_count;
        VECTOR_FIELD_FREE(
            executor, executor->temporaries_stack[executor->tmp_count]
        );
    }

    while (executor->stack_frame > org) {
        --executor->stack_frame;
        VECTOR_FIELD_FREE(executor, executor->stack[executor->stack_frame]);
        VECTOR_FIELD_FREE(
            executor, executor->creation_follower_stack[executor->stack_frame]
        );
    }

    struct vector_field const ret = executor->return_register;
    executor->return_register = VECTOR_FIELD_NULL;
    return ret;
}

static struct timeline_instruction *
timeline_executor_entry(
    struct timeline_execution_context *executor,
    struct timeline_instruction *prev, struct aux_entry_model *entry,
    mc_bool_t modify
)
{
    executor->execution_line++;
    struct timeline_instruction *const ret =
        timeline_instruction_parse(executor, prev, entry, modify);

    return ret;
}

static struct timeline_instruction *
timeline_executor_group(
    struct timeline_execution_context *executor,
    struct timeline_instruction *prev, struct aux_group_model *group,
    mc_bool_t modify
)
{
    /* walk and parse */
    struct timeline_instruction *root = NULL;
    struct timeline_instruction *head = prev;

    struct aux_group_model_mode *const mode = aux_group_mode(group);
    for (mc_ind_t i = 0; i < mode->aux_entry_count; i++) {
        struct timeline_instruction *const curr =
            timeline_executor_entry(executor, head, mode->entries + i, modify);

        if (!curr) {
            if (prev) {
                prev->in_order_next = NULL;
            }
            timeline_instruction_unref(executor, root);
            return NULL;
        }

        curr->in_order_prev = head;

        /* apply else if and if statements, not necessarily relevant on a group
         * level..., but still done  */
        struct timeline_instruction *reverse;
        for (reverse = head;
             reverse &&
             (reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE ||
              reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE_IF);
             reverse = reverse->in_order_prev) {
            reverse->conditional_next = curr;
        }

        if (reverse) {
            reverse->conditional_next = curr;
        }

        if (!root) {
            root = curr;
        }

        if (!head) {
            head = curr;
        }
        else {
            head->next = head->in_order_next = curr;
        }

        while (head->next) {
            head = head->next;
        }
    }

    /* enum entry */
    if (!mode->aux_entry_count) {
        executor->execution_line++;

        for (mc_ind_t i = 0; i < group->child_count; ++i) {
            executor->execution_line++;
            if (!group->children[i]->modes[0].entries[0].is_empty) {
                VECTOR_FIELD_ERROR(executor, "Illegal indentation");

                return NULL;
            }
        }

        struct timeline_instruction *const ret =
            timeline_instruction_identity(executor);
        ret->in_order_prev = head;

        struct timeline_instruction *reverse;
        for (reverse = head;
             reverse &&
             (reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE ||
              reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE_IF);
             reverse = reverse->in_order_prev) {
            reverse->conditional_next = ret;
        }
        if (reverse) {
            reverse->conditional_next = ret;
        }

        if (head) {
            head->next = head->in_order_next = ret;
        }

        return ret;
    }

    if (root) {
        struct timeline_instruction *reverse;
        for (reverse = head;
             reverse &&
             (reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE ||
              reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE_IF);
             reverse = reverse->in_order_prev) {
            reverse->conditional_next = NULL;
        }
        if (reverse) {
            reverse->conditional_next = NULL;
        }
    }

    return root;
}

static inline void
timeline_executor_symbol_frame_push(struct timeline_execution_context *executor)
{
    ++executor->symbol_depth;
}

static void
timeline_executor_symbol_frame_pop(struct timeline_execution_context *executor)
{
    /* depth of 1 is not popped since that can be used for future slides as well
     */
    if (executor->symbol_depth > 1) {
        while (executor->symbol_count &&
               executor->symbol_stack[executor->symbol_count - 1].depth ==
                   executor->symbol_depth) {
            timeline_executor_symbol_pop(executor, 1);
        }
    }
    --executor->symbol_depth;
}

struct timeline_instruction *
timeline_executor_parse_frame(
    struct timeline_execution_context *executor, mc_count_t count,
    struct aux_group_model **groups, mc_bool_t modify
)
{
    struct timeline_instruction *root = NULL;
    struct timeline_instruction *head = NULL;

    timeline_executor_symbol_frame_push(executor);

    for (mc_ind_t i = 0; i < count; i++) {
        struct timeline_instruction *const curr =
            timeline_executor_group(executor, head, groups[i], modify);

        if (!curr) {
            timeline_instruction_unref(executor, root);
            root = NULL;
            break;
        }

        curr->in_order_prev = head;

        struct timeline_instruction *reverse;
        for (reverse = head;
             reverse &&
             (reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE ||
              reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE_IF);
             reverse = reverse->in_order_prev) {
            reverse->conditional_next = curr;
        }
        if (reverse) {
            reverse->conditional_next = curr;
        }

        if (!head) {
            root = head = curr;
        }
        else {
            head->next = head->in_order_next = curr;
        }
        while (head->next) {
            head = head->next;
        }
    }

    if (!count) {
        root = timeline_instruction_identity(executor);
    }
    else if (root) {
        struct timeline_instruction *reverse;
        for (reverse = head;
             reverse &&
             (reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE ||
              reverse->conditional == INSTRUCTION_CONDITIONAL_ELSE_IF);
             reverse = reverse->in_order_prev) {
            reverse->conditional_next = NULL;
        }
        if (reverse) {
            reverse->conditional_next = NULL;
        }
    }

    timeline_executor_symbol_frame_pop(executor);

    return root;
}

void
timeline_executor_parse(
    struct timeline_execution_context *executor, mc_ind_t index,
    struct raw_slide_model *slide, mc_bool_t modify
)
{

    executor->state = TIMELINE_EXECUTOR_STATE_COMPILING;
    /* don't want slide to be persisted for the stdlib */
    if (index) {
        executor->execution_slide = slide;
    }
    else {
        executor->execution_slide = NULL;
    }
    executor->execution_line = 0;

    struct aux_slide_model *converted = raw_to_aux(executor, slide);
    if (!converted) {
        return;
    }

    if (!slide->scene) {
        converted->is_std = 1;
    }

    executor->execution_line = (mc_count_t) -1;

    struct timeline_instruction *const root = timeline_executor_parse_frame(
        executor, converted->child_count, converted->children, modify
    );

    executor->execution_slide = NULL;
    executor->execution_line = 0;

    aux_to_raw(executor, converted, slide);

    executor->slides[index].symbol_delta = executor->symbol_delta;
    executor->slides[index].symbol_count = executor->symbol_count;
    executor->slides[index].instructions = root;
    executor->slides[index].slide = slide;

    if (executor->state == TIMELINE_EXECUTOR_STATE_COMPILING) {
        executor->state = TIMELINE_EXECUTOR_STATE_IDLE;
    }
}

struct viewport_camera
timeline_camera(struct timeline_execution_context *executor)
{
    /* assume to be in right state... */
    return executor->camera_cache;
}

struct vec4
timeline_background(struct timeline_execution_context *executor)
{
    /* assume to be in right state... */
    return executor->background_cache;
}

static struct tetramesh **
mesh_dump(
    struct vector_field curr, struct tetramesh **vector, mc_count_t *mesh_count
)
{
    if (curr.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const mesh = curr.value.pointer;
        for (mc_ind_t i = 0; i < mesh->field_count; ++i) {
            vector = mesh_dump(mesh->fields[i], vector, mesh_count);
        }
    }
    else {
        MC_MEM_RESERVE(vector, *mesh_count);
        vector[(*mesh_count)++] = curr.value.pointer;
    }

    return vector;
}

struct tetramesh **
timeline_meshes(
    struct timeline_execution_context *executor, mc_count_t *mesh_count
)
{
    /* mesh_count and executor mesh count aren't the same because one is tree
     * mesh other is puremesh*/
    *mesh_count = 0;
    struct tetramesh **vector = NULL;
    mc_ind_t j = 0;
    for (mc_ind_t i = 0; i < executor->mesh_count; ++i) {
        mc_count_t const old = *mesh_count;
        vector = mesh_dump(executor->meshes[i], vector, mesh_count);
        if (*mesh_count != old) {
            executor->meshes[j] = executor->meshes[i];
            executor->mesh_hashes[j] = executor->mesh_hashes[i];
            ++j;
        }
    }
    executor->mesh_count = j;

    return vector;
}

static struct vector_field *
find_mesh(struct timeline_execution_context *executor, mc_ind_t follower_ind)
{
    for (mc_ind_t i = executor->mesh_count; i-- > 0;) {
        if (executor->mesh_hashes[i] == follower_ind) {
            return &executor->meshes[i];
        }
    }
    return NULL;
}

mc_ind_t
timeline_follower_ind_of(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    if (field.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        uintptr_t const flat_pointer = (uintptr_t) field.value.pointer;
        /* persistent... */
        if (flat_pointer >= (uintptr_t) executor->capture_frame &&
            flat_pointer <
                (uintptr_t) executor->capture_frame +
                    sizeof(struct vector_field) * executor->capture_count) {
            return (flat_pointer - (uintptr_t) executor->capture_frame) /
                   sizeof(executor->capture_frame[0]);
        }
        else {
            return timeline_follower_ind_of(
                executor, *(struct vector_field *) field.value.pointer
            );
        }
    }
    else {
        return SIZE_MAX;
    }
}

mc_bool_t
timeline_is_scene_variable(
    struct timeline_execution_context *executor, mc_ind_t follower_ind
)
{
    return follower_ind == executor->follower_stack[CAMERA_VARIABLE_INDEX] ||
           follower_ind == executor->follower_stack[BACKGROUND_VARIABLE_INDEX];
}

/* precondition: field is a reference var */
mc_bool_t
timeline_is_reference_var_a_vector(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    while (field.vtable == &reference_vtable) {
        if (field.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
            return 1;
        }
        field = *(struct vector_field *) field.value.pointer;
    }
    return 0;
}

static mc_status_t
show_recurse(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    if (field.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vector = field.value.pointer;
        for (mc_ind_t i = 0; i < vector->field_count; ++i) {
            if (show_recurse(executor, vector->fields[i])) {
                return MC_STATUS_FAIL;
            }
        }

        return MC_STATUS_SUCCESS;
    }
    else if (field.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        mc_ind_t const follower_ind = timeline_follower_ind_of(executor, field);
        if (follower_ind == SIZE_MAX) {
            return show_recurse(
                executor, *(struct vector_field *) (field.value.pointer)
            );
        }
        else if (find_mesh(executor, follower_ind)) {
            return MC_STATUS_SUCCESS;
        }
        else if (timeline_is_scene_variable(executor, follower_ind)) {
            return MC_STATUS_SUCCESS;
        }

        if (executor->mesh_count >= executor->mesh_capacity) {
            executor->mesh_capacity =
                MC_MEM_NEXT_CAPACITY(executor->mesh_count);
            executor->meshes = mc_reallocf(
                executor->meshes,
                sizeof(struct vector_field) * executor->mesh_capacity
            );
            executor->mesh_hashes = mc_reallocf(
                executor->mesh_hashes,
                sizeof(mc_ind_t) * executor->mesh_capacity
            );
        }

        executor->mesh_hashes[executor->mesh_count] = follower_ind;
        executor->meshes[executor->mesh_count] =
            VECTOR_FIELD_COPY(executor, field);

        /* make flat*/
        if (!verify_mesh(executor, &executor->meshes[executor->mesh_count++])) {
            return MC_STATUS_FAIL;
        }

        return MC_STATUS_SUCCESS;
    }
    else {
        VECTOR_FIELD_ERROR(executor, "Illegal mesh tree");
        executor->return_register = VECTOR_FIELD_NULL;
        return MC_STATUS_FAIL;
    }
}

mc_status_t
timeline_mesh_show(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    return show_recurse(executor, field);
}

static mc_status_t
hide_recurse(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    if (field.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vector = field.value.pointer;
        for (mc_ind_t i = 0; i < vector->field_count; ++i) {
            if (hide_recurse(executor, vector->fields[i])) {
                return MC_STATUS_FAIL;
            }
        }

        return MC_STATUS_SUCCESS;
    }
    else if (field.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        mc_ind_t const follower_ind = timeline_follower_ind_of(executor, field);
        if (follower_ind == SIZE_MAX) {
            return hide_recurse(
                executor, *(struct vector_field *) (field.value.pointer)
            );
        }
        else if (timeline_is_scene_variable(executor, follower_ind)) {
            return MC_STATUS_SUCCESS;
        }

        struct vector_field *const curr = find_mesh(executor, follower_ind);
        if (!curr) {
            return MC_STATUS_SUCCESS;
        }

        VECTOR_FIELD_FREE(executor, *curr);
        ptrdiff_t const index = curr - executor->meshes;
        for (mc_ind_t i = (mc_ind_t) index; i < executor->mesh_count - 1; ++i) {
            executor->meshes[i] = executor->meshes[i + 1];
            executor->mesh_hashes[i] = executor->mesh_hashes[i + 1];
        }

        --executor->mesh_count;
        return MC_STATUS_SUCCESS;
    }
    else {
        VECTOR_FIELD_ERROR(executor, "Illegal mesh tree");
        executor->return_register = VECTOR_FIELD_NULL;
        return MC_STATUS_FAIL;
    }
}

mc_status_t
timeline_mesh_hide(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    return hide_recurse(executor, field);
}

static void
aux_group_string(struct str_dynamic *dynamic, struct timeline_arg_group e)
{
    if (e.index) {
        str_dynamic_append(dynamic, "[");
        str_dynamic_append(dynamic, e.index);
        str_dynamic_append(dynamic, "] {");
    }

    for (mc_ind_t i = 0; i < e.mode_count; ++i) {
        if (e.index) {
            str_dynamic_append(dynamic, "[");
            str_dynamic_append(dynamic, e.modes[i].label);
            str_dynamic_append(dynamic, "] {");
        }

        for (mc_ind_t j = 0; j < e.modes[i].used_count; ++j) {
            timeline_symbol_aux_entry_string(dynamic, e.modes[i].real_args[j]);
            if (j < e.modes[i].used_count - 1) {
                str_dynamic_append(dynamic, ", ");
            }
        }

        if (e.index) {
            str_dynamic_append(dynamic, "}");
        }

        if (i < e.mode_count - 1) {
            str_dynamic_append(dynamic, ", ");
        }
    }

    if (e.index) {
        str_dynamic_append(dynamic, "}");
    }
}

void
timeline_symbol_aux_entry_string(
    struct str_dynamic *dynamic, struct timeline_symbol_entry entry
)
{
    str_dynamic_append(dynamic, entry.name);
    if (entry.reference_var) {
        str_dynamic_append(dynamic, "&");
    }
    else if (entry.group_count) {
        str_dynamic_append(dynamic, "(");
        for (mc_ind_t i = 0; i < entry.group_count; ++i) {
            aux_group_string(dynamic, entry.arg_groups[i]);
            if (i < entry.group_count - 1) {
                str_dynamic_append(dynamic, ", ");
            }
        }
        str_dynamic_append(dynamic, ")");
    }
}

void
timeline_executor_symbol_mode_free(struct timeline_arg_mode e)
{
    for (mc_ind_t k = 0; k < e.used_count; ++k) {
        timeline_executor_symbol_free(e.real_args[k]);
    }
    mc_free(e.real_args);
    mc_free((char *) e.label);
}

void
timeline_executor_symbol_aux_group_free(struct timeline_arg_group e)
{
    for (mc_ind_t j = 0; j < e.mode_count; ++j) {
        timeline_executor_symbol_mode_free(e.modes[j]);
    }
    mc_free(e.modes);
    mc_free((char *) e.index);
}

void
timeline_executor_symbol_free(struct timeline_symbol_entry e)
{
    for (mc_ind_t i = 0; i < e.group_count; ++i) {
        timeline_executor_symbol_aux_group_free(e.arg_groups[i]);
    }
    mc_free(e.arg_groups);
    mc_free((char *) e.name);
    mc_free(e.captures);
}

static mc_bool_t
symbol_equals(struct timeline_symbol_entry a, struct timeline_symbol_entry b)
{
    if (strcmp(a.name, b.name)) {
        return 0;
    }
    else if (a.reference_var != b.reference_var) {
        return 0;
    }
    else if (a.group_count != b.group_count) {
        return 0;
    }

    for (mc_ind_t i = 0; i < a.group_count; ++i) {
        if (a.arg_groups[i].index && b.arg_groups[i].index) {
            if (a.arg_groups[i].mode_count != b.arg_groups[i].mode_count) {
                return 0;
            }
        }
        for (mc_ind_t j = 0; j < a.arg_groups[i].mode_count; ++j) {
            if (strcmp(
                    a.arg_groups[i].modes[j].label,
                    b.arg_groups[i].modes[j].label
                )) {
                return 0;
            }
            if (a.arg_groups[i].modes[j].used_count !=
                b.arg_groups[i].modes[j].used_count) {
                return 0;
            }
            for (mc_ind_t k = 0; k < a.arg_groups[i].modes[j].used_count; ++k) {
                if (!symbol_equals(
                        a.arg_groups[i].modes[j].real_args[k],
                        b.arg_groups[i].modes[j].real_args[k]
                    )) {
                    return 0;
                }
            }
        }
    }

    return 1;
}

void
timeline_executor_symbol_pop(
    struct timeline_execution_context *executor, mc_bool_t free
)
{
    struct timeline_symbol_entry const e =
        executor->symbol_stack[--executor->symbol_count];
    executor->symbol_delta =
        (mc_count_t) ((mc_rind_t) executor->symbol_delta - e.delta);

    if (e.name) {
        if (e.prev_index != SIZE_MAX) {
            unowned_map_set(
                &executor->symbol_index_map, e.name,
                (void *) (uintptr_t) (e.prev_index + 1)
            );
        }
        else {
            unowned_map_del(&executor->symbol_index_map, e.name);
        }
    }

    if (free) {
        timeline_executor_symbol_free(e);
    }
}

static mc_bool_t
valid_var_name(char const *name)
{
    if (!*name) {
        return 0;
    }
    else if ('0' <= *name && *name <= '9') {
        return 0;
    }

    for (char const *x = name; *x; ++x) {
        if (*x >= '0' && *x <= '9') {
            continue;
        }
        else if (*x >= 'a' && *x <= 'z') {
            continue;
        }
        else if (*x >= 'A' && *x <= 'Z') {
            continue;
        }
        else if (*x == '_') {
            continue;
        }

        return 0;
    }

    if (!strcmp(name, "let")) {
        return 0;
    }
    if (!strcmp(name, "var")) {
        return 0;
    }
    if (!strcmp(name, "for")) {
        return 0;
    }
    if (!strcmp(name, "if")) {
        return 0;
    }
    if (!strcmp(name, "else")) {
        return 0;
    }
    if (!strcmp(name, "in")) {
        return 0;
    }
    if (!strcmp(name, "native")) {
        return 0;
    }
    if (!strcmp(name, "func")) {
        return 0;
    }
    if (!strcmp(name, "tree")) {
        return 0;
    }
    if (!strcmp(name, "sticky")) {
        return 0;
    }

    return 1;
}

/* succesful if it's unique on the same level, OR it's a function and same
 * function declaration OR the indices are same*/
mc_status_t
timeline_executor_symbol_push(
    struct timeline_execution_context *executor,
    struct timeline_symbol_entry entry
)
{

    mc_ind_t prev_index = SIZE_MAX;
    if (entry.name) {
        if (!valid_var_name(entry.name)) {
            VECTOR_FIELD_ERROR(executor, "Invalid var name `%s`", entry.name);
            return MC_STATUS_FAIL;
        }

        uintptr_t const one_index = (uintptr_t
        ) unowned_map_get(&executor->symbol_index_map, entry.name);
        if (one_index) {
            prev_index = one_index - 1;

            if (executor->symbol_stack[prev_index].depth ==
                    executor->symbol_depth &&
                (executor->symbol_stack[prev_index].index !=
                     executor->symbol_delta ||
                 !symbol_equals(executor->symbol_stack[prev_index], entry))) {
                VECTOR_FIELD_ERROR(
                    executor, "Duplicate declaration of var `%s` on same level",
                    entry.name
                );
                return MC_STATUS_FAIL;
            }
        }
    }

    entry.prev_index = prev_index;
    entry.index = executor->symbol_delta;
    entry.depth = executor->symbol_depth;

    executor->symbol_delta =
        (mc_count_t) ((mc_diff_t) executor->symbol_delta + entry.delta);

    MC_MEM_RESERVE(executor->symbol_stack, executor->symbol_count);
    executor->symbol_stack[executor->symbol_count++] = entry;

    if (entry.name) {
        unowned_map_set(
            &executor->symbol_index_map, entry.name,
            (void *) (uintptr_t) (executor->symbol_count)
        );
    }

    return MC_STATUS_SUCCESS;
}

struct timeline_symbol_entry
timeline_executor_symbol_search(
    struct timeline_execution_context *executor,
    struct expression_tokenizer const *tokenizer, mc_bool_t elide_functor_args
)
{
    char const *tmp = tokenizer_dup(tokenizer);

    uintptr_t one_index =
        (uintptr_t) unowned_map_get(&executor->symbol_index_map, tmp);
    /* elide functor args */
    while (one_index && executor->symbol_stack[one_index - 1].functor_arg) {
        one_index = executor->symbol_stack[one_index - 1].prev_index + 1;
    }

    mc_ind_t const prev_index = one_index - 1;

    if (!one_index) {
        goto error;
    }
    else {
        mc_free((char *) tmp);
    }

    mc_count_t captures_encounted = 0;
    for (mc_ind_t i = 0; i < executor->func_count; ++i) {
        if (executor->func_cut_stack
                [executor->func_count - 1 - captures_encounted] >= prev_index) {
            ++captures_encounted;
        }
        else {
            break;
        }
    }

    if (captures_encounted > 0 &&
        !executor->symbol_stack[prev_index].constant) {
        VECTOR_FIELD_ERROR(
            executor, "Cannot capture `%s`, which is a mutable variable",
            executor->symbol_stack[prev_index].name
        );
        return (struct timeline_symbol_entry){ 0 };
    }

    long long previous_ind =
        (long long) executor->symbol_stack[prev_index].index;
    for (mc_ind_t j = executor->func_count - captures_encounted;
         j < executor->func_count; ++j) {
        struct timeline_symbol_entry *const func =
            executor->symbol_stack + executor->func_stack[j];
        struct timeline_symbol_entry *const cut =
            executor->symbol_stack + executor->func_cut_stack[j];

        /* current neg gets really weird due to functor functional arguments*/
        long long const current_neg =
            (long long) cut->index + cut->delta - previous_ind;
        mc_bool_t found = 0;
        /* see if it already exists at this function level otherwise insert */
        for (mc_ind_t k = 0; k < func->capture_count; ++k) {
            if (func->captures[k] == current_neg) {
                /* we dont use func->delta because func->delta could technially
                 * be 2 due to it being the last of a functor argument group,
                 * but even in that case when the actual function call is being
                 * made, only 1 item will be laid out on the stack (regardless
                 * of it's true delta in the first pass). Therefore, we always
                 * use 1 */
                previous_ind = (long long) cut->index + cut->delta - 1 -
                               (long long) (k + 1);
                found = 1;
                break;
            }
        }
        if (!found) {
            /* recursive capture is handled slightly differently */
            if (&executor->symbol_stack[prev_index] == func) {
                previous_ind = (long long) func->index;
            }
            else {
                /* push */
                MC_MEM_RESERVE(func->captures, func->capture_count);
                func->captures[func->capture_count++] = current_neg;
                previous_ind = (long long) cut->index + cut->delta - 1 -
                               (long long) func->capture_count;
            }
        }
    }
    //
    /* last capture */
    struct timeline_symbol_entry copy = executor->symbol_stack[prev_index];
    copy.index = (mc_ind_t) previous_ind;
    return copy;

error:
    VECTOR_FIELD_ERROR(executor, "Variable lookup for `%s` failed", tmp);
    mc_free((char *) tmp);

    return (struct timeline_symbol_entry){ 0 };
}

/* allows functor args! normal search does not */
struct timeline_symbol_entry *
timeline_executor_symbol_pointer(
    struct timeline_execution_context *executor, char const *name
)
{
    uintptr_t const one_index =
        (uintptr_t) unowned_map_get(&executor->symbol_index_map, name);

    if (one_index) {
        return &executor->symbol_stack[one_index - 1];
    }

    return NULL;
}

/* precondition that it already exists*/
long long
timeline_executor_symbol_negindex(
    struct timeline_execution_context *executor, char const *name
)
{
    return (long long) executor->symbol_delta -
           (long long) timeline_executor_symbol_pointer(executor, name)->index;
}

void
timeline_executor_free(struct timeline_execution_context *executor)
{

    for (mc_ind_t i = 0; i < executor->media_count; ++i) {
        media_value_free(executor->media_cache[i]);
    }
    mc_free(executor->media_cache);

    for (mc_ind_t i = 0; i < executor->slide_count; ++i) {
        clear_slide_trailing_cache(executor, i);
        timeline_instruction_unref(executor, executor->slides[i].instructions);
        mc_free((char *) executor->slides[i].title);
    }

    while (executor->symbol_count) {
        timeline_executor_symbol_pop(executor, 1);
    }
    unowned_map_free(executor->symbol_index_map);
    mc_free(executor->symbol_stack);

    mc_free(executor->func_stack);
    mc_free(executor->func_cut_stack);

    while (executor->tmp_count) {
        --executor->tmp_count;
        VECTOR_FIELD_FREE(
            executor, executor->temporaries_stack[executor->tmp_count]
        );
    }

    while (executor->stack_frame) {
        --executor->stack_frame;
        VECTOR_FIELD_FREE(executor, executor->stack[executor->stack_frame]);
        VECTOR_FIELD_FREE(
            executor, executor->creation_follower_stack[executor->stack_frame]
        );
    }

    while (executor->capture_count) {
        --executor->capture_count;
        VECTOR_FIELD_FREE(
            executor, executor->capture_frame[executor->capture_count]
        );
    }

    while (executor->mesh_count) {
        --executor->mesh_count;
        VECTOR_FIELD_FREE(executor, executor->meshes[executor->mesh_count]);
    }
    mc_free(executor->meshes);
    mc_free(executor->mesh_hashes);

    mc_free(executor->slides);
    mc_free(executor);
}
