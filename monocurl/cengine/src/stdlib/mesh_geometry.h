//
//  mesh_geometry.h
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
void
lib_mc_general_rect(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields, mc_bool_t force_up_rank
);

LIBMC_DEC_FUNC(dot_mesh);
LIBMC_DEC_FUNC(circle);
LIBMC_DEC_FUNC(annulus);
LIBMC_DEC_FUNC(rect);
LIBMC_DEC_FUNC(regular_polygon);
LIBMC_DEC_FUNC(polygon);
LIBMC_DEC_FUNC(polyline);
LIBMC_DEC_FUNC(line);
LIBMC_DEC_FUNC(triangle);
LIBMC_DEC_FUNC(capsule);

LIBMC_DEC_FUNC(sphere);
LIBMC_DEC_FUNC(rectangular_prism);
LIBMC_DEC_FUNC(cylinder);

LIBMC_DEC_FUNC(bezier);
LIBMC_DEC_FUNC(color_grid);
LIBMC_DEC_FUNC(field);

LIBMC_DEC_FUNC(arc);
LIBMC_DEC_FUNC(arrow);

void
vector_like(
    struct timeline_execution_context *executor, struct vec3 tail,
    struct vec3 delta, struct vec3 normal, float path_arc, mc_bool_t half,
    struct vector_field *tags
);

LIBMC_DEC_FUNC(half_vector);
LIBMC_DEC_FUNC(vector);
LIBMC_DEC_FUNC(plane);
#endif
