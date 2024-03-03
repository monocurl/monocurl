//
//  mc_util.h
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
LIBMC_DEC_FUNC(sort);
LIBMC_DEC_FUNC(left_key);
LIBMC_DEC_FUNC(right_key);
LIBMC_DEC_FUNC(reverse);
LIBMC_DEC_FUNC(zip);
LIBMC_DEC_FUNC(map);
LIBMC_DEC_FUNC(reduce);
LIBMC_DEC_FUNC(len);
LIBMC_DEC_FUNC(depth);
LIBMC_DEC_FUNC(count);
LIBMC_DEC_FUNC(filter);
LIBMC_DEC_FUNC(sum);
LIBMC_DEC_FUNC(product);
LIBMC_DEC_FUNC(all);
LIBMC_DEC_FUNC(any);
LIBMC_DEC_FUNC(map_keys);
LIBMC_DEC_FUNC(map_values);
LIBMC_DEC_FUNC(map_items);

LIBMC_DEC_FUNC(mean);
LIBMC_DEC_FUNC(std_dev);

LIBMC_DEC_FUNC(integrate);
LIBMC_DEC_FUNC(derivative);
LIBMC_DEC_FUNC(limit);
LIBMC_DEC_FUNC(ln);
LIBMC_DEC_FUNC(log2);
LIBMC_DEC_FUNC(log10);
LIBMC_DEC_FUNC(log);

LIBMC_DEC_FUNC(sin);
LIBMC_DEC_FUNC(cos);
LIBMC_DEC_FUNC(tan);
LIBMC_DEC_FUNC(cot);
LIBMC_DEC_FUNC(sec);
LIBMC_DEC_FUNC(csc);
LIBMC_DEC_FUNC(arcsin);
LIBMC_DEC_FUNC(arccos);
LIBMC_DEC_FUNC(arctan);

LIBMC_DEC_FUNC(factorial);
LIBMC_DEC_FUNC(choose);
LIBMC_DEC_FUNC(permute);
LIBMC_DEC_FUNC(gcd);
LIBMC_DEC_FUNC(max);
LIBMC_DEC_FUNC(min);
LIBMC_DEC_FUNC(abs);
LIBMC_DEC_FUNC(clamp);
LIBMC_DEC_FUNC(is_prime);
LIBMC_DEC_FUNC(sign);
LIBMC_DEC_FUNC(mod);
LIBMC_DEC_FUNC(floor);
LIBMC_DEC_FUNC(round);
LIBMC_DEC_FUNC(ceil);
LIBMC_DEC_FUNC(trunc);
LIBMC_DEC_FUNC(random);
LIBMC_DEC_FUNC(randint);

LIBMC_DEC_FUNC(norm);
LIBMC_DEC_FUNC(normalize);
LIBMC_DEC_FUNC(dot);
LIBMC_DEC_FUNC(cross);
LIBMC_DEC_FUNC(proj);
LIBMC_DEC_FUNC(vec_add);
LIBMC_DEC_FUNC(vec_mul);

LIBMC_DEC_FUNC(str_replace);
LIBMC_DEC_FUNC(read_followers);
LIBMC_DEC_FUNC(set_followers);
LIBMC_DEC_FUNC(reference_map);
LIBMC_DEC_FUNC(is_scene_variable);

struct vector_field
vector_add(
    struct timeline_execution_context *executor, struct vector_field *fields
);

struct vector_field
vector_multiply(
    struct timeline_execution_context *executor, struct vector_field *fields
);

LIBMC_DEC_FUNC(not_implemented_yet);
#endif
