//
//  anim_show.h
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once
#include <stdio.h>

#include "mc_env.h"
#include "mc_lib_helpers.h"

#if MC_INTERNAL
LIBMC_DEC_FUNC(showhide_decomp);
LIBMC_DEC_FUNC(grow);
LIBMC_DEC_FUNC(fade);
LIBMC_DEC_FUNC(write);

void
write_interpolate(
    mc_ind_t i, mc_count_t subset, struct tetramesh *src, struct tetramesh *tag,
    float u, float v
);

#endif
