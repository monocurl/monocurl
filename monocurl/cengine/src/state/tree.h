//
//  tree.h
//  Monocurl
//
//  Created by Manu Bhat on 10/21/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include "entry.h"
#include "group.h"
#include "media.h"
#include "scene.h"
#include "slide.h"

#include "mc_env.h"

// common functionality
#if MC_INTERNAL
void
tree_general_insert(
    struct aux_slide_model *slide, struct aux_group_model *parent,
    struct aux_group_model *entry, mc_ind_t index
);
#endif
