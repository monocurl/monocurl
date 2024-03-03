#include "mc_threading.h"

void
mc_threading_init(void)
{
    p_libsys_init();
}

void
mc_threading_free(void)
{
    p_libsys_shutdown();
}

mc_thread_t *
mc_thread_init(
    void *(*func)(void *), void *argument, mc_bool_t joinable, char const *name
)
{
    PUThread *const p_thread = p_uthread_create(func, argument, joinable, name);

    if (!p_thread) {
        return NULL;
    }

    p_uthread_set_priority(p_thread, P_UTHREAD_PRIORITY_HIGHEST);
    return p_thread;
}

mc_status_t
mc_thread_wait(mc_timeinterval_t interval)
{
    return p_uthread_sleep((puint32) mc_timeinterval_to_millis(interval)) == 0
               ? MC_STATUS_FAIL
               : MC_STATUS_SUCCESS;
}

mc_status_t
mc_thread_join(mc_thread_t *thread)
{
    mc_status_t const ret =
        p_uthread_join(thread) == -1 ? MC_STATUS_FAIL : MC_STATUS_SUCCESS;
    p_uthread_unref(thread);
    return ret;
}

mc_mutex_t *
mc_mutex_init(void)
{
    return p_mutex_new();
}

mc_status_t
mc_mutex_lock(mc_mutex_t *mutex)
{
    return p_mutex_lock(mutex) ? MC_STATUS_SUCCESS : MC_STATUS_FAIL;
}

mc_status_t
mc_mutex_unlock(mc_mutex_t *mutex)
{
    return p_mutex_unlock(mutex) ? MC_STATUS_SUCCESS : MC_STATUS_FAIL;
}

void
mc_mutex_free(mc_mutex_t *mutex)
{
    p_mutex_free(mutex);
}

mc_cond_variable_t *
mc_cond_variable_init(void)
{
    return p_cond_variable_new();
}

mc_status_t
mc_cond_variable_wait(mc_cond_variable_t *cond, mc_mutex_t *mutex)
{
    return p_cond_variable_wait(cond, mutex) ? MC_STATUS_SUCCESS
                                             : MC_STATUS_FAIL;
}

mc_status_t
mc_cond_variable_signal(mc_cond_variable_t *cond)
{
    return p_cond_variable_signal(cond) ? MC_STATUS_SUCCESS : MC_STATUS_FAIL;
}
void
mc_cond_variable_free(mc_cond_variable_t *cond)
{
    p_cond_variable_free(cond);
}

mc_rwlock_t *
mc_rwlock_init(void)
{
    return p_rwlock_new();
}

mc_status_t
mc_rwlock_reader_lock(mc_rwlock_t *lock)
{
    return p_rwlock_reader_lock(lock) ? MC_STATUS_SUCCESS : MC_STATUS_FAIL;
}

mc_status_t
mc_rwlock_reader_unlock(mc_rwlock_t *lock)
{
    return p_rwlock_reader_unlock(lock) ? MC_STATUS_SUCCESS : MC_STATUS_FAIL;
}

mc_status_t
mc_rwlock_writer_lock(mc_rwlock_t *lock)
{
    return p_rwlock_writer_lock(lock) ? MC_STATUS_SUCCESS : MC_STATUS_FAIL;
}

mc_status_t
mc_rwlock_writer_unlock(mc_rwlock_t *lock)
{
    return p_rwlock_writer_unlock(lock) ? MC_STATUS_SUCCESS : MC_STATUS_FAIL;
}

void
mc_rwlock_free(mc_rwlock_t *lock)
{
    p_rwlock_free(lock);
}
