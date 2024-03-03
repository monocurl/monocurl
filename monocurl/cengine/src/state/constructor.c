//
//  constructor.c
//  Monocurl
//
//  Created by Manu Bhat on 9/25/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "constructor.h"
#include "mc_memory.h"
#include "strutil.h"

// entries need to be manually linked upon placement in a tree
struct aux_group_model *
aux_group_custom_blank(char const *title)
{
    struct aux_entry_model entry = { 0 };
    entry.data = mc_calloc(1, sizeof(char));
    entry.title = title ? mc_strdup(title) : NULL;

    struct aux_group_model *ret = mc_calloc(1, sizeof(struct aux_group_model));
    entry.group = ret;

    ret->group_class = title ? mc_strdup(title) : NULL;
    ret->mode_key = mc_strdup("main");
    ret->mode_count = 1;
    ret->modes = mc_calloc(sizeof(struct aux_group_model_mode), 1);
    ret->modes[0].key = ret->mode_key;
    ret->modes[0].aux_entry_count = 1;
    ret->modes[0].entries = mc_malloc(sizeof(struct aux_entry_model));
    ret->modes[0].entries[0] = entry;

    return ret;
}

struct raw_slide_model *
slide_standard(char const *name)
{
    char const *const title = mc_strdup(name);

    struct raw_slide_model *const ret =
        mc_calloc(1, sizeof(struct raw_slide_model));
    ret->title = title;
    ret->buffer_size = 1;
    ret->buffer = mc_strdup("\n");

    return ret;
}
