//
//  mesh_tex.h
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_env.h"
#include "mc_meshes.h"

#if MC_INTERNAL

void
get_tex(
    struct timeline_execution_context *executor, char const *str,
    struct vector_field *fields
);

LIBMC_DEC_FUNC(mesh_text);
LIBMC_DEC_FUNC(mesh_brace);
LIBMC_DEC_FUNC(mesh_measure);
LIBMC_DEC_FUNC(mesh_number);

#endif
