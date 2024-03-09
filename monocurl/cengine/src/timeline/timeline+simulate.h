//
//  timeline+simulate.h
//  monocurl
//
//  Created by Manu Bhat on 11/30/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_types.h"
#include "timeline.h"

#if MC_INTERNAL
mc_status_t
timeline_slide_startup(
    struct timeline *timeline, mc_ind_t slide_index, mc_bool_t context_switch
);

mc_ternary_status_t
timeline_step(struct timeline *timeline, double dt);

mc_ternary_status_t
timeline_frame(struct timeline *timeline, double dt, unsigned int upf);

mc_status_t
timeline_blit_trailing_cache(struct timeline *timeline);

void
timeline_play(struct timeline *timeline);

mc_status_t
timeline_really_seek_to(struct timeline *timeline);
#endif
