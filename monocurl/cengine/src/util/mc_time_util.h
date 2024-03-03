#pragma once

#include "mc_env.h"
#include <stdio.h>

typedef size_t mc_timestamp_t;
typedef long long mc_timeinterval_t;

#if MC_INTERNAL

// should only be used relatively
// not guaranteed to be epoch time or anythig
mc_timestamp_t
mc_timestamp_now(void);

mc_timeinterval_t
mc_timediff(mc_timestamp_t a, mc_timestamp_t b);

long long
mc_timeinterval_to_seconds(mc_timeinterval_t interval);

long long
mc_timeinterval_to_millis(mc_timeinterval_t interval);

mc_timeinterval_t
mc_timeinterval_from_millis(long long millis);

#endif
