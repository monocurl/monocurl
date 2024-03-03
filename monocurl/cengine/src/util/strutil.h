//
//  strutil.h
//  Monocurl
//
//  Created by Manu Bhat on 11/16/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_env.h"
#include "mc_types.h"
#include <stdio.h>

struct str_dynamic {
    char *pointer;
    mc_count_t offset, capacity;
};

#if MC_INTERNAL
struct str_dynamic
str_dynamic_init(void);

void
str_dynamic_append(struct str_dynamic *dst, char const *src);

void
str_dynamic_append_esc(struct str_dynamic *dst, char const *src);

char const *
str_first_non_var_name(char const *str);

/* http://www.cse.yorku.ca/~oz/hash.html */
mc_hash_t
str_hash(unsigned char const *bytes, mc_count_t count);
mc_hash_t
str_null_terminated_hash(unsigned char const *bytes);

char *
mc_strdup(char const *src);
#endif
