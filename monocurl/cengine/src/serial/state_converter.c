//
//  state_converter.c
//  Monocurl
//
//  Created by Manu Bhat on 1/8/24.
//  Copyright © 2024 Enigmadux. All rights reserved.
//

#include <string.h>

#include "constructor.h"
#include "interpreter.h"
#include "state_converter.h"
#include "strutil.h"

#define BUFFER_SIZE 1024

static void
serial_entry(
    struct aux_entry_model *entry, struct str_dynamic *str, char *buffer,
    mc_count_t depth
)
{
    entry->overall_start_index = str->offset;

    for (mc_ind_t i = 0; i < depth; ++i) {
        str_dynamic_append(str, "\t");
    }

    if (entry->title) {
        str_dynamic_append(str, entry->title);
        str_dynamic_append(str, ": ");
    }
    entry->title_end_index = str->offset;

    str_dynamic_append(str, entry->data);
    str_dynamic_append(str, "\n");
}

static void
serial_group(
    struct aux_group_model *group, struct str_dynamic *str, char *buffer,
    mc_count_t depth
)
{
    group->dump_start_index = str->offset;

    struct aux_group_model_mode *const mode = aux_group_mode(group);
    if (!mode->aux_entry_count) {
        for (mc_ind_t i = 0; i < group->tabs; ++i) {
            str_dynamic_append(str, "\t");
        }

        str_dynamic_append(str, group->group_class);
        str_dynamic_append(str, ": ");
        group->union_header_end_index = str->offset;
        str_dynamic_append(str, mode->key);
        str_dynamic_append(str, "\n");
    }
    else {
        for (mc_ind_t i = 0; i < mode->aux_entry_count; ++i) {
            serial_entry(&mode->entries[i], str, buffer, group->tabs);
        }
    }

    for (mc_ind_t i = 0; i < group->child_count; ++i) {
        serial_group(group->children[i], str, buffer, depth + 1);
    }
}

static mc_status_t
decode_entry(
    struct timeline_execution_context *executor, struct aux_group_model *group,
    struct aux_entry_model *entry, char **context, mc_count_t depth
)
{
    /* if it's custom mark group as custom*/
    char *ret;
    for (ret = *context; *ret && !strchr("/\n:\'\"=", *ret); ++ret)
        ;

    if (depth && ret[0] == ':' && ret[1] == ' ') {
        *ret = 0;
        if (!group->group_class) {
            group->group_class = mc_strdup(" ");
        }
        entry->title = mc_strdup(*context);
        *context = ret + 2; /* skip space and colon */
    }

    for (ret = *context;; ++ret) {
        if (!*ret) {
            entry->data = mc_strdup(*context);
            *context = ret;
            break;
        }
        else if (*ret == '\n') {
            *ret = 0;
            entry->data = mc_strdup(*context);
            *context = ret + 1;
            break;
        }
    }

    executor->execution_line++;

    return MC_STATUS_SUCCESS;
}

static mc_bool_t
read_tab(char **context)
{
    if (**context == '\t') {
        ++*context;
        return 1;
    }
    else {
        for (mc_ind_t i = 0; i < 4; ++i) {
            if ((*context)[i] != ' ') {
                return 0;
            }
        }
        *context += 4;

        return 1;
    }
}

static mc_bool_t
count_tabs_and_is_empty_line(char **context, mc_count_t *tab_count)
{
    *tab_count = 0;
    while (read_tab(context)) {
        ++*tab_count;
    }

    /* if all white space, exempt from indentation rules */
    mc_count_t count_2 = 0;
    while ((*context)[count_2] == '\t' || (*context)[count_2] == ' ') {
        ++count_2;
    }
    mc_bool_t const is_empty =
        (*context)[count_2] == '\n' || !(*context)[count_2];

    return is_empty;
}

/* consumes new lines directly or via entry */
static mc_status_t
decode_group(
    struct timeline_execution_context *executor, struct aux_group_model *group,
    char **context, mc_count_t depth
)
{
    group->mode_key = mc_strdup("main");
    group->mode_count = 1;
    group->modes = mc_calloc(1, sizeof(struct aux_group_model_mode));
    group->modes[0].key = group->mode_key;

    mc_count_t count = 0;
    mc_bool_t const is_empty = count_tabs_and_is_empty_line(context, &count);

    if (!is_empty) {
        if (count != depth) {
            VECTOR_FIELD_ERROR(executor, "Inconsistent indentation");
            return MC_STATUS_FAIL;
        }
    }

    group->tabs = count;

    MC_MEM_RESERVE(group->modes[0].entries, group->modes[0].aux_entry_count);
    struct aux_entry_model *const entry =
        &group->modes[0].entries[group->modes[0].aux_entry_count++];
    *entry = (struct aux_entry_model){ 0 };
    entry->is_empty = is_empty;

    if (decode_entry(executor, group, entry, context, count)) {
        return MC_STATUS_FAIL;
    }

    while (!is_empty && **context) {
        /* keep decoding group until tab count is less than or equal to depth */
        char *save = *context;

        mc_count_t tabs = 0;
        mc_bool_t is_curr_empty = count_tabs_and_is_empty_line(context, &tabs);

        *context = save;

        if (!is_curr_empty) {
            if (tabs <= depth) {
                break;
            }
        }

        struct aux_group_model *const child =
            mc_calloc(1, sizeof(struct aux_group_model));
        MC_MEM_RESERVE(group->children, group->child_count);
        group->children[group->child_count++] = child;

        if (decode_group(executor, child, context, depth + 1)) {
            return MC_STATUS_FAIL;
        }
    }

    return MC_STATUS_SUCCESS;
}

static inline void
pp_entry(struct aux_entry_model *entry, struct aux_group_model *parent)
{
    entry->group = parent;
}

static void
pp_group(
    struct aux_group_model *group, struct aux_group_model *group_parent,
    struct aux_slide_model *slide
)
{
    group->parent = group_parent;
    group->slide = slide;

    for (mc_ind_t i = 0; i < group->mode_count; ++i) {
        for (mc_ind_t j = 0; j < group->modes[i].aux_entry_count; j++) {
            pp_entry(group->modes[i].entries + j, group);
        }
    }
    for (mc_ind_t i = 0; i < group->child_count; ++i) {
        pp_group(group->children[i], group, slide);
    }
}

static void
pp_slide(struct aux_slide_model *slide)
{
    for (mc_ind_t i = 0; i < slide->child_count; ++i) {
        pp_group(slide->children[i], NULL, slide);
    }
}

struct aux_slide_model *
raw_to_aux(
    struct timeline_execution_context *executor, struct raw_slide_model *raw
)
{
    struct aux_slide_model *slide =
        mc_calloc(1, sizeof(struct aux_slide_model));

    char *dup = mc_strdup(raw->buffer), *run = dup;

    while (*run) {
        struct aux_group_model *const group =
            mc_calloc(1, sizeof(struct aux_group_model));
        MC_MEM_RESERVE(slide->children, slide->child_count);
        slide->children[slide->child_count++] = group;

        if (decode_group(executor, group, &run, 0)) {
            mc_free(dup);
            aux_slide_free(slide);
            return NULL;
        }
    }

    slide->child_capacity = mc_po2_ceil(slide->child_count);
    if (slide->child_count &&
        slide->child_capacity < (1ull << MC_MEM_INITIAL_COUNT_EXP)) {
        slide->child_capacity = 1ull << MC_MEM_INITIAL_COUNT_EXP;
    }
    pp_slide(slide);
    mc_free(dup);

    return slide;
}

static void
dump_group(
    struct aux_group_model *group, struct raw_slide_model *slide,
    mc_count_t *lines
)
{
    if (group->group_class) {
        MC_MEM_RESERVE(slide->functor_groups, slide->group_count);
        slide->functor_groups[slide->group_count].title =
            mc_strdup(group->group_class);
        slide->functor_groups[slide->group_count].mode_count =
            group->mode_count;
        slide->functor_groups[slide->group_count].overall_start_index =
            group->dump_start_index;

        slide->functor_groups[slide->group_count].current_mode =
            aux_group_mode_index(group);
        slide->functor_groups[slide->group_count].modes =
            mc_malloc(group->mode_count * sizeof(struct slide_functor_mode));
        slide->functor_groups[slide->group_count].tabs = group->tabs;
        slide->functor_groups[slide->group_count].line = *lines;

        for (mc_ind_t i = 0; i < group->mode_count; ++i) {
            slide->functor_groups[slide->group_count].modes[i] =
                (struct slide_functor_mode){
                    .title = mc_strdup(group->modes[i].key),
                    .arg_count = group->modes[i].aux_entry_count,
                    .arg_titles = mc_malloc(
                        group->modes[i].aux_entry_count * sizeof(char *)
                    ),
                };

            for (mc_ind_t j = 0; j < group->modes[i].aux_entry_count; ++j) {
                slide->functor_groups[slide->group_count]
                    .modes[i]
                    .arg_titles[j] =
                    mc_strdup(group->modes[i].entries[j].title);
            }
        }

        struct aux_group_model_mode *mode = aux_group_mode(group);
        if (mode->aux_entry_count) {
            for (mc_ind_t i = 0; i < mode->aux_entry_count; ++i) {
                MC_MEM_RESERVE(
                    slide->functor_arg_start, slide->total_functor_args
                );
                slide->functor_arg_start[slide->total_functor_args] =
                    mode->entries[i].overall_start_index;
                MC_MEM_RESERVE(
                    slide->functor_arg_end, slide->total_functor_args
                );
                slide->functor_arg_end[slide->total_functor_args] =
                    mode->entries[i].title_end_index;

                slide->total_functor_args++;
            }
        }
        else {
            MC_MEM_RESERVE(slide->functor_arg_start, slide->total_functor_args);
            slide->functor_arg_start[slide->total_functor_args] =
                group->dump_start_index;
            MC_MEM_RESERVE(slide->functor_arg_end, slide->total_functor_args);
            slide->functor_arg_end[slide->total_functor_args] =
                group->union_header_end_index;

            slide->total_functor_args++;
        }

        slide->group_count++;
    }

    mc_count_t delta = aux_group_mode(group)->aux_entry_count;
    *lines += delta ? delta : 1;

    for (mc_ind_t i = 0; i < group->child_count; ++i) {
        dump_group(group->children[i], slide, lines);
    }
}

void
aux_to_raw(
    struct timeline_execution_context *executor, struct aux_slide_model *aux,
    struct raw_slide_model *dump
)
{
    char buffer[BUFFER_SIZE];
    struct str_dynamic ret = str_dynamic_init();

    for (mc_count_t i = 0; i < aux->child_count; ++i) {
        serial_group(aux->children[i], &ret, buffer, 0);
    }

    mc_free(dump->buffer);
    dump->buffer = ret.pointer;
    dump->buffer[dump->buffer_size = ret.offset] = 0;

    /* create functor keepout */
    dump->total_functor_args = 0;
    mc_free(dump->functor_arg_start);
    mc_free(dump->functor_arg_end);
    dump->functor_arg_start = dump->functor_arg_end = NULL;

    for (mc_ind_t i = 0; i < dump->group_count; ++i) {
        slide_group_free(&dump->functor_groups[i]);
    }
    mc_free(dump->functor_groups);
    dump->group_count = 0;
    dump->functor_groups = NULL;

    mc_count_t lines = 0;
    for (mc_ind_t i = 0; i < aux->child_count; ++i) {
        dump_group(aux->children[i], dump, &lines);
    }

    aux_slide_free(aux);
}
