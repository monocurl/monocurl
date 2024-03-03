//
//  thread_queue.c
//  Monocurl
//
//  Created by Manu Bhat on 11/13/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "mc_memory.h"
#include "thread_queue.h"

struct thread_q *
thread_q_init(mc_bool_t unary)
{
    struct thread_q *alloc = mc_calloc(1, sizeof(struct thread_q));
    alloc->unary = unary;

    return alloc;
}

enum timeline_task_type
thread_q_poll(struct thread_q *q)
{
    enum timeline_task_type const ret = q->tasks[q->read];

    if (q->unary) {
        q->count = 0;
    }
    else if (q->count) {
        q->read = (q->read + 1) % q->capacity;
        --q->count;
    }

    return ret;
}

extern inline enum timeline_task_type
thread_q_peek(struct thread_q *q);

enum timeline_task_type
thread_q_skip_old(struct thread_q *q)
{
    enum timeline_task_type const ret = q->tasks[q->write];

    // free unused
    //    const size_t fake_count = q->count - 1;
    //    const size_t pivot = q->write + fake_count < q->capacity ? fake_count
    //    : q->capacity - q->write; for (mc_ind_t i = 0; i < pivot; i++)
    //    free(q->tasks[i + q->write]); for (mc_ind_t i = 0; i < fake_count -
    //    pivot; i++) free((void *) q->tasks[i]);
    //
    q->write = q->read = q->count = 0;

    return ret;
}

void
thread_q_push(struct thread_q *q, enum timeline_task_type elem)
{
    if (q->capacity == q->count) {
        // realloc (which can't be done using a normal realloc since we have the
        // problem of pivot elements
        mc_count_t const capacity = MC_MEM_NEXT_CAPACITY(q->count);
        enum timeline_task_type *const tasks =
            mc_malloc(sizeof(enum timeline_task_type) * capacity);

        // amount of elements at rear of deque
        mc_count_t const pivot = q->write + q->count < q->capacity
                                     ? q->count
                                     : q->capacity - q->write;
        if (q->tasks) {
            memcpy(tasks, q->tasks + q->write, pivot);
            memcpy(tasks + pivot, q->tasks, q->count - pivot);
        }

        mc_free((enum timeline_task_type *) q->tasks);

        q->read = 0;
        q->write = q->count;

        q->tasks = tasks;
        q->capacity = capacity;
    }

    if (q->unary) {
        q->tasks[q->write] = elem;
        q->count = 1;
    }
    else {
        q->tasks[q->write] = elem;
        q->write = (q->write + 1) % q->capacity;

        q->count++;
    }
}

void
thread_q_free(struct thread_q *q)
{
    mc_free(q->tasks);
    mc_free(q);
}
