//
//  function.h
//  Monocurl
//
//  Created by Manu Bhat on 12/10/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "timeline_execution_context.h"
#include "timeline_instruction.h"
#include "vector_field.h"

struct function {
    // the cache of fields it's going to use. These are pushed onto the stack
    // each time it's used sooo how do we want the capture creation to work. We
    // can just see if a variable is going to be used like that, in which case
    // we
    mc_count_t cache_count;
    //    struct vector_field_wrapper {
    //        struct vector_field field;
    //        /*
    //          If capturing an lvalue, the original lvalue will point to an
    //          executor buffer (if not already) instead of it's current value.
    //          We only capture the buffered value, to ensure longetivity.
    //          Whenever this is freed, the ref count on the executor buffer
    //          will be decremented
    //         */
    //        struct timeline_capture_pointee *capture;
    //        /* when freed, the capture count will decrease by 1. This is only
    //        when we are capturing a reference to a variable */
    //    }
    struct vector_field *caches;
    mc_hash_t hash_cache;

    struct timeline_instruction *head;
};

#if MC_INTERNAL
struct vector_field
function_init(
    struct timeline_execution_context *executor,
    struct timeline_instruction *head, mc_count_t cache_count,
    struct vector_field *caches
);
struct vector_field
function_copy(
    struct timeline_execution_context *executor, struct vector_field func
);
mc_hash_t
function_hash(
    struct timeline_execution_context *executor, struct vector_field func
);
struct vector_field
function_comp(
    struct timeline_execution_context *executor, struct vector_field func,
    struct vector_field *rhs
);
void
function_call(
    struct timeline_execution_context *executor, struct vector_field v,
    mc_count_t count, struct vector_field *args
);
mc_count_t
function_bytes(
    struct timeline_execution_context *executor, struct vector_field v
);

void
function_free(
    struct timeline_execution_context *executor, struct vector_field function
);
#endif
