#pragma once
#include "mc_env.h"

#if MC_INTERNAL
#define _MC_STRINGIFY(mcr) #mcr
#define MC_STRINGIFY(mcr) _MC_STRINGIFY(mcr)

#define _MC_CONCAT(lhs, rhs) lhs##rhs
#define MC_CONCAT(lhs, rhs) _MC_CONCAT(lhs, rhs)
#endif
