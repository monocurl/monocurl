//
//  state_converter.h
//  Monocurl
//
//  Created by Manu Bhat on 1/8/24.
//  Copyright © 2024 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_env.h"
#include "mc_types.h"
#include "scene_handle.h"
#include "slide.h"
#include <stdio.h>

#if MC_INTERNAL

struct aux_slide_model *
raw_to_aux(
    struct timeline_execution_context *executor, struct raw_slide_model *raw
);

void
aux_to_raw(
    struct timeline_execution_context *executor, struct aux_slide_model *aux,
    struct raw_slide_model *dump
);

#endif
