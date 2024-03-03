//
//  expression_tokenizer.h
//  Monocurl
//
//  Created by Manu Bhat on 1/4/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include "mc_env.h"
#include "mc_types.h"
#include <stdio.h>

struct expression_tokenizer {
    char const *start;
    char const *end; /* exclusive */

    mc_bool_t mod_allowed;
    mc_bool_t remove_children;
    mc_bool_t block_functors;

    struct aux_entry_model *entry;
    struct timeline_execution_context *executor;
};

#if MC_INTERNAL
char *
tokenizer_dup(struct expression_tokenizer const *tokenizer);
void
tokenizer_read(struct expression_tokenizer *tokenizer);
mc_bool_t
tokenizer_equals(struct expression_tokenizer const *tokenizer, char const *cmp);
#endif
