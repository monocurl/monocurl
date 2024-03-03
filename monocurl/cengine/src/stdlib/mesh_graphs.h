//
//  mesh_graphs.h
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once
#include <stdio.h>

#include "mc_env.h"
#include "mc_meshes.h"

#if MC_INTERNAL
LIBMC_DEC_FUNC(axis_1d);
LIBMC_DEC_FUNC(axis_2d);
LIBMC_DEC_FUNC(axis_3d);
// LIBMC_DEC_FUNC(polar_axis);
LIBMC_DEC_FUNC(parametric_func);
LIBMC_DEC_FUNC(explicit_func_diff);
LIBMC_DEC_FUNC(implicit_func_2d);

#endif
