//
//  timeline_exeuction_context.h
//  monocurl
//
//  Created by Manu Bhat on 12/2/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "animation.h"
#include "group.h"
#include "mc_env.h"
#include "scene.h"
#include "strutil.h"
#include "unowned_map.h"
#include "vector.h"
#include "viewport.h"

#if MC_INTERNAL
#define CAMERA_VARIABLE_INDEX 0
#define PLAY_VARIABLE_INDEX 1
#define BACKGROUND_VARIABLE_INDEX 2

#define STACK_DEPTH_CHECK_RATE (1 << 5)
#endif

#define OVERFLOW_BUFFER (1 << 4)
#define MAX_STACK_FRAME (1 << 12)
#define MAX_FUNCTION_CALL (1 << 5)
#define MAX_TEMPORARIES (1 << 12)
#define MAX_CAPTURES (1 << 14)
#define MAX_HEAP (1 << 25)

struct expression_tokenizer;
struct timeline;

struct timeline_symbol_entry {
    char const *name;
    mc_ind_t index;
    mc_ind_t prev_index; /* for duplicate names on duplicate depths */
    mc_count_t depth;

    /* essentially the same thing with group mode. Only given if this is a
     * function */
    mc_count_t group_count;
    struct timeline_arg_group {
        char const *index;
        mc_count_t mode_count;
        mc_count_t union_size;
        struct timeline_arg_mode {
            char const *label;
            mc_count_t used_count;
            struct timeline_symbol_entry *real_args;
        } *modes;
    } *arg_groups;

    mc_count_t capture_count;
    mc_rind_t *captures; /* all references to main stack and unowned */

    /* might want to move to bitset??*/
    mc_diff_t delta;
    /* only applicable to nested function args*/
    mc_bool_t reference_var;
    mc_bool_t functor_arg; /* NOT the same as a function arg, used for proper
                              capture semantics */
    mc_bool_t function_arg;
    mc_bool_t constant;
    mc_bool_t tree;
};

struct timeline_execution_context {
    struct timeline *timeline;

    enum timeline_executor_state {
        TIMELINE_EXECUTOR_STATE_IDLE = 0,
        TIMELINE_EXECUTOR_STATE_COMPILING = 1,
        TIMELINE_EXECUTOR_STATE_INITIALIZATION = 2,
        TIMELINE_EXECUTOR_STATE_ANIMATION = 3,
        TIMELINE_EXECUTOR_STATE_ERROR = 4
    } state;

    struct raw_slide_model *execution_slide;
    mc_count_t execution_line;

    mc_count_t media_count;
    struct raw_media_model *media_cache;

    mc_count_t slide_count;
    struct timeline_slide {
        char const *title;

        // variable stack cache for compilation
        mc_count_t symbol_count, symbol_delta;

        // instruction set
        struct timeline_instruction *instructions;
        struct raw_slide_model *slide;

        // trailing cache
        mc_bool_t trailing_valid;
        double seconds;

        mc_count_t mesh_count;
        mc_ind_t *mesh_jump_to;
        struct vector_field *meshes;
        mc_ind_t *mesh_hashes;

        mc_count_t capture_count;
        mc_ind_t *capture_jump_to;
        struct vector_field *capture_frame;

        mc_count_t stack_frame;
        mc_ind_t *stack_jump_to;
        struct vector_field *stack;
        mc_ind_t *creation_follower_jump_to;
        struct vector_field *creation_follower_stack;
        mc_ind_t *follower_stack;
    } *slides;

    mc_rind_t curr_slide;
    double curr_seconds;

    mc_count_t symbol_delta;
    mc_count_t symbol_count;
    mc_count_t symbol_depth;
    struct timeline_symbol_entry *symbol_stack;
    struct unowned_map symbol_index_map;

    /* indices to the symbol stack, used to see what to capture */
    /* capture variables are inserted in reverse order of usage to ensure
     * invariant of neg index */
    mc_count_t func_count;
    mc_ind_t *func_stack;
    mc_ind_t *func_cut_stack;

    /* Persistent memory */
    mc_count_t mesh_nonce;
    mc_count_t mesh_capacity, mesh_count;
    struct vector_field *meshes;
    mc_ind_t *mesh_hashes;

    struct vec4 background_cache;
    struct viewport_camera camera_cache;

    mc_count_t capture_count;
    struct vector_field capture_frame
        [MAX_CAPTURES +
         OVERFLOW_BUFFER]; // lvalue references that might need to be
                           // sustained past the general stack frame..
#pragma message(                                                               \
    "TODO, this need not be in an elevated frame, somewhat inefficient "       \
)

    mc_count_t func_depth;
    mc_count_t stack_depth;
    mc_count_t stack_frame;

    struct vector_field
        stack[MAX_STACK_FRAME + OVERFLOW_BUFFER]; /* The root object contains
                                                     the camera, active meshes,
                                                     and what not */
    struct vector_field
        creation_follower_stack[MAX_STACK_FRAME + OVERFLOW_BUFFER];
    mc_ind_t follower_stack[MAX_STACK_FRAME + OVERFLOW_BUFFER];

    /* Temporaries and registers */
    mc_count_t tmp_count;
    struct vector_field temporaries_stack[MAX_TEMPORARIES + OVERFLOW_BUFFER];

    struct vector_field return_register;

    // rough estimator just to make sure we don't use over 1gb or something, not
    // necessarily 100% accurate
    mc_count_t byte_alloc;
    mc_count_t check_nonce;
};

#if MC_INTERNAL
struct timeline_execution_context *
timeline_executor_init(struct timeline *timeline);

void
timeline_executor_invalidate(
    struct timeline_execution_context *executor, struct raw_slide_model *slide,
    mc_bool_t modify
);
void
timeline_executor_resize(
    struct timeline_execution_context *executor, struct raw_scene_model *scene,
    mc_bool_t modify
);

void
timeline_executor_report_error(
    struct timeline_execution_context *executor, ...
);

mc_ind_t
timeline_follower_ind_of(
    struct timeline_execution_context *executor, struct vector_field field
);

struct vector_field *
timeline_get_follower(
    struct timeline_execution_context *executor, struct vector_field *ptr
);

mc_status_t
timeline_executor_ref_capture(
    struct timeline_execution_context *executor, mc_ind_t index
);

struct vector_field *
timeline_executor_temporary_push(
    struct timeline_execution_context *executor, struct vector_field field
);
struct vector_field *
timeline_executor_var_push(
    struct timeline_execution_context *executor, struct vector_field field
);

mc_bool_t
timeline_executor_check_interrupt(
    struct timeline_execution_context *executor, mc_bool_t force
);

void
timeline_executor_pre_interrupt(struct timeline_execution_context *executor);

mc_bool_t
timeline_executor_post_interrupt(struct timeline_execution_context *executor);

struct timeline_instruction *
timeline_executor_parse_frame(
    struct timeline_execution_context *executor, mc_count_t count,
    struct aux_group_model **groups, mc_bool_t modify
);
void
timeline_executor_parse(
    struct timeline_execution_context *executor, mc_ind_t index,
    struct raw_slide_model *slide, mc_bool_t modify
);

mc_status_t
timeline_executor_symbol_push(
    struct timeline_execution_context *executor,
    struct timeline_symbol_entry entry
);
struct timeline_symbol_entry
timeline_executor_symbol_search(
    struct timeline_execution_context *executor,
    struct expression_tokenizer const *tokenizer, mc_bool_t elide_functor_args
);
long long
timeline_executor_symbol_negindex(
    struct timeline_execution_context *executor, char const *name
);
struct timeline_symbol_entry *
timeline_executor_symbol_pointer(
    struct timeline_execution_context *executor, char const *name
);

void
timeline_symbol_aux_entry_string(
    struct str_dynamic *dynamic, struct timeline_symbol_entry entry
);

struct vec4
timeline_background(struct timeline_execution_context *executor);
struct viewport_camera
timeline_camera(struct timeline_execution_context *executor);
struct tetramesh **
timeline_meshes(
    struct timeline_execution_context *executor, mc_count_t *mesh_count
);

mc_bool_t
timeline_is_reference_var_a_vector(
    struct timeline_execution_context *executor, struct vector_field field
);

mc_bool_t
timeline_is_scene_variable(
    struct timeline_execution_context *executor, mc_ind_t follower_ind
);

mc_status_t
timeline_mesh_show(
    struct timeline_execution_context *executor, struct vector_field field
);
mc_status_t
timeline_mesh_hide(
    struct timeline_execution_context *executor, struct vector_field field
);

void
timeline_executor_symbol_mode_free(struct timeline_arg_mode e);
void
timeline_executor_symbol_aux_group_free(struct timeline_arg_group e);
void
timeline_executor_symbol_free(struct timeline_symbol_entry e);
void
timeline_executor_symbol_pop(
    struct timeline_execution_context *executor, mc_bool_t free
);

void
timeline_executor_prune_register(struct timeline_execution_context *executor);

struct vector_field
timeline_executor_execute(
    struct timeline_execution_context *executor,
    struct timeline_instruction *instruction, mc_bool_t prune
);

// error
mc_status_t
timeline_executor_startup(
    struct timeline_execution_context *executor, mc_ind_t slide_num,
    mc_bool_t context_switch
);
// error or positive for it finished
mc_ternary_status_t
timeline_executor_step(struct timeline_execution_context *executor, double dt);
void
timeline_executor_blit_cache(struct timeline_execution_context *executor);

void
timeline_executor_free(struct timeline_execution_context *executor);
#endif
