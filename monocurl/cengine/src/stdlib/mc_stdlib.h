//
//  mcstdlib.h
//  Monocurl
//
//  Created by Manu Bhat on 1/14/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_env.h"
#include "vector_field.h"
#include <stdio.h>

#if MC_INTERNAL

void (*mc_find_stdlib(struct expression_tokenizer *tokenizer))(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
);

void
libmc_stdlib_init(void);

#endif
