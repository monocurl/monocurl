//
//  interpreter.h
//  Monocurl
//
//  Created by Manu Bhat on 10/23/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_types.h"
#include "tree.h"

#if MC_INTERNAL
void
interpreter_slide(
    struct raw_slide_model *slide, mc_bool_t maintain_tree_invariants
);

void
interpreter_scene(
    struct raw_scene_model *scene, mc_bool_t maintain_tree_invariants,
    mc_bool_t reseek
);
#endif
