//
//  thread_queue.h
//  Monocurl
//
//  Created by Manu Bhat on 11/13/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_types.h"
#include "timeline.h"

// none need any data, since it's inferred from context
enum timeline_task_type {
    TIMELINE_NO_TASK,
    TIMELINE_SEEK,
    TIMELINE_PLAY,
    TIMELINE_EXPORT,
    TIMELINE_TERMINATE
};

struct thread_q {
    enum timeline_task_type *tasks;
    mc_count_t capacity, count;
    mc_ind_t read, write; // vpointers

    mc_bool_t unary;
};

#if MC_INTERNAL

// assumes trivial destructor
struct thread_q *
thread_q_init(mc_bool_t unary);

// garbage if non existent
enum timeline_task_type
thread_q_poll(struct thread_q *q);

inline enum timeline_task_type
thread_q_peek(struct thread_q *q)
{
    return q->tasks[q->read];
}

void
thread_q_push(struct thread_q *q, enum timeline_task_type elem);

enum timeline_task_type
thread_q_skip_old(struct thread_q *q);

void
thread_q_free(struct thread_q *q);
#endif
