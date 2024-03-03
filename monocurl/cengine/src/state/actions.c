//
//  actions.c
//  Monocurl
//
//  Created by Manu Bhat on 10/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "actions.h"
#include "callback.h"
#include "config.h"
#include "constructor.h"

#define MC_LOG_TAG "actions"
#include "mc_log.h"

// how many digits to allocate when naming new slides
// log_10(2) ~= 1/3
#define DIGIT_BUFFER (sizeof(mc_ind_t) / 3 + 2)

// timeline state changes so much that it's better to just allow the action and
// do nothing than recompute every single frame
mc_bool_t
can_toggle_play(struct timeline const *timeline)
{
    if (!timeline) {
        return 0;
    }

    return 1;
}

mc_bool_t
can_prev_slide(struct timeline const *timeline)
{
    if (!timeline) {
        return 0;
    }

    return 1;
}

mc_bool_t
can_next_slide(struct timeline const *timeline)
{
    if (!timeline) {
        return 0;
    }

    return 1;
}

mc_bool_t
can_revert_full(struct timeline const *timeline)
{
    if (!timeline) {
        return 0;
    }

    return 1;
}

void
insert_slide_after(struct raw_slide_model *entry)
{
    mc_ind_t const i = slide_index_in_parent(entry);

    size_t const bytes = strlen("slide_") + DIGIT_BUFFER + 1;
    char *output = mc_malloc(bytes);
    snprintf(output, bytes, "slide_%zu", entry->scene->slide_count);

    struct raw_slide_model *new = slide_standard(output);
    slide_append_to_scene(new, entry->scene, i + 1);

    mc_logn("insert slide", " title: '%s'", new, output);

    mc_free(output);
}

void
delete_slide(struct raw_slide_model *slide)
{
    slide_delete(slide);
}
