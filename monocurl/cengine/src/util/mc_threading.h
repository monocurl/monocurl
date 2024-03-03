#pragma once

#include <plibsys.h>

#include "mc_env.h"
#include "mc_time_util.h"
#include "mc_types.h"

typedef PMutex mc_mutex_t;
typedef PRWLock mc_rwlock_t;
typedef PCondVariable mc_cond_variable_t;
typedef PUThread mc_thread_t;

mc_status_t
mc_rwlock_reader_lock(mc_rwlock_t *lock);

mc_status_t
mc_rwlock_reader_unlock(mc_rwlock_t *lock);

#if MC_INTERNAL
void
mc_threading_init(void);

void
mc_threading_free(void);

mc_thread_t *
mc_thread_init(
    void *(*func)(void *), void *argument, mc_bool_t joinable, char const *name
);

mc_status_t
mc_thread_wait(mc_timeinterval_t interval);

mc_status_t
mc_thread_join(mc_thread_t *thread);

mc_mutex_t *
mc_mutex_init(void);

mc_status_t
mc_mutex_lock(mc_mutex_t *mutex);

mc_status_t
mc_mutex_unlock(mc_mutex_t *mutex);

void
mc_mutex_free(mc_mutex_t *mutex);

mc_cond_variable_t *
mc_cond_variable_init(void);

mc_status_t
mc_cond_variable_wait(mc_cond_variable_t *cond, mc_mutex_t *mutex);

mc_status_t
mc_cond_variable_signal(mc_cond_variable_t *cond);

void
mc_cond_variable_free(mc_cond_variable_t *cond);

mc_rwlock_t *
mc_rwlock_init(void);

mc_status_t
mc_rwlock_writer_lock(mc_rwlock_t *lock);

mc_status_t
mc_rwlock_writer_unlock(mc_rwlock_t *lock);

void
mc_rwlock_free(mc_rwlock_t *lock);

#endif
