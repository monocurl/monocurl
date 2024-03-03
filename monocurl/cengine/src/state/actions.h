//
//  actions.h
//  Monocurl
//
//  Created by Manu Bhat on 10/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_types.h"
#include "timeline.h"
#include "tree.h"

mc_bool_t
can_toggle_play(struct timeline const *timeline);

mc_bool_t
can_prev_slide(struct timeline const *timeline);

mc_bool_t
can_next_slide(struct timeline const *timeline);

mc_bool_t
can_revert_full(struct timeline const *timeline);

void
insert_slide_after(struct raw_slide_model *entry);

void
delete_slide(struct raw_slide_model *slide);
