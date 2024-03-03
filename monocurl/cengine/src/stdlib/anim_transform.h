//
//  anim_transform.h
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_anims.h"
#include "mc_env.h"

#if MC_INTERNAL
LIBMC_DEC_FUNC(transform);
LIBMC_DEC_FUNC(tag_transform);
LIBMC_DEC_FUNC(bend);
LIBMC_DEC_FUNC(tag_bend);

mc_status_t
match_tree(
    struct timeline_execution_context *executor, struct vector_field a,
    struct vector_field b, struct vector_field a_dmp,
    struct vector_field dmp_dmp, struct vector_field b_dmp,
    mc_ind_t tag_map_index, struct vector_field tag_mapping
);

void
match_group(
    struct timeline_execution_context *executor, struct vector_field *a_src,
    struct vector_field *b_src, struct vector_field a, struct vector_field dmp,
    struct vector_field b, mc_count_t a_count, mc_count_t b_count
);

void
copy_lin(struct tetra_lin *dst, struct tetra_lin const *src, float u, float v);

LIBMC_DEC_FUNC(rotate_transform);

#endif
