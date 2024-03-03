//
//  anim_util_h.h
//  Monocurl
//
//  Created by Manu Bhat on 2/19/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_lib_helpers.h"

#if MC_INTERNAL
double
anim_smooth(double t);

struct vector_field
general_lerp(
    struct timeline_execution_context *executor, struct vector_field *fields
);

LIBMC_DEC_FUNC(lerp);
LIBMC_DEC_FUNC(keyframe_lerp);
LIBMC_DEC_FUNC(smooth);
LIBMC_DEC_FUNC(smooth_in);
LIBMC_DEC_FUNC(smooth_out);
#endif
