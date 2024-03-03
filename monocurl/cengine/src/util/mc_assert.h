#pragma once
#include "mc_env.h"
#include <assert.h>

#if MC_INTERNAL
#define mc_compile_assert(cond, message)                                       \
    typedef struct {                                                           \
        int MC_CONCAT(__static_assertion_failed, message) : !!(cond);          \
    } MC_CONCAT(__static_assert_failed, __COUNTER__)
#endif
