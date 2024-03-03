#include "mc_time_util.h"
#include "mc_env.h"

#define MC_LOG_TAG "time_util"
#include "mc_log.h"

#if MC_ENV_OS & MC_ENV_OS_WINDOWS
#include <sys/timeb.h>
#else
#include <sys/time.h>
#include <time.h>
#endif

// should only be used relatively
// not guaranteed to be epoch time or anythig
mc_timestamp_t
mc_timestamp_now(void)
{
#if MC_ENV_OS & MC_ENV_OS_WINDOWS
    struct _timeb tv;
    _ftime64_s(&tv);
    return (mc_timestamp_t) (tv.time * 1000 + tv.millitm);
#else
    struct timeval tv;
    gettimeofday(&tv, NULL);

    return (mc_timestamp_t) (tv.tv_sec * 1000 + tv.tv_usec / 1000);
#endif
}

mc_timeinterval_t
mc_timediff(mc_timestamp_t a, mc_timestamp_t b)
{
    return (mc_timeinterval_t) a - (mc_timeinterval_t) b;
}

long long
mc_timeinterval_to_seconds(mc_timeinterval_t interval)
{
    return (interval + 500) / 1000;
}

long long
mc_timeinterval_to_millis(mc_timeinterval_t interval)
{
    return interval;
}

mc_timeinterval_t
mc_timeinterval_from_millis(long long millis)
{
    return millis;
}
