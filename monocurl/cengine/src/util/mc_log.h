//
//  mc_log.h
//  monocurl
//
//  Created by Manu Bhat on 11/28/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once
#include <stdio.h>

#include "callback.h"
#include "config.h"
#include "mc_env.h"
#include "mc_macro_util.h"

#if MC_INTERNAL

#ifndef MC_LOG_TAG
#define MC_LOG_TAG "global"
#endif

#ifdef MC_LOGGING
#if MC_ENV_OS & MC_ENV_OS_WINDOWS && MC_DEBUG
#include <Windows.h>
static char __mc_log__msg_debug_[256];
/* assumes no multiple definitions per line, which is generally true */
#define _mc_log_printf(command, ...)                                           \
    sprintf(__mc_log__msg_debug_, command, __VA_ARGS__);                       \
    debug_write_log(__mc_log__msg_debug_)

#define mc_log(command, context, ...)                                          \
    _mc_log_printf(                                                            \
        "%25s :: %-15s [callee: %p" context "]", MC_LOG_TAG, command,          \
        (void *) __VA_ARGS__                                                   \
    )
#define mc_logn(command, context, ...)                                         \
    _mc_log_printf(                                                            \
        "%25s :: %-15s [callee: %p" context "]\n", MC_LOG_TAG, command,        \
        (void *) __VA_ARGS__                                                   \
    )
#define mc_logn_static(command, context, ...)                                  \
    _mc_log_printf(                                                            \
        "%25s :: %-15s [" context "]\n", MC_LOG_TAG, command, __VA_ARGS__      \
    )

#define mc_log_errorn(command, context, ...)                                   \
    _mc_log_printf(                                                            \
        "%25s :: %-15s [callee: %p" context "]\n", "err::" MC_LOG_TAG,         \
        command, (void *) __VA_ARGS__                                          \
    )
#define mc_log_errorn_static(command, context, ...)                            \
    _mc_log_printf(                                                            \
        "%25s :: %-15s [" context "]\n", "err::" MC_LOG_TAG, command,          \
        __VA_ARGS__                                                            \
    )
#else
#define mc_log(command, context, ...)                                          \
    printf(                                                                    \
        "%25s :: %-15s [callee: %p" context "]", MC_LOG_TAG, command,          \
        (void *) __VA_ARGS__                                                   \
    )
#define mc_logn(command, context, ...)                                         \
    printf(                                                                    \
        "%25s :: %-15s [callee: %p" context "]\n", MC_LOG_TAG, command,        \
        (void *) __VA_ARGS__                                                   \
    )
#define mc_logn_static(command, context, ...)                                  \
    printf("%25s :: %-15s [" context "]\n", MC_LOG_TAG, command, __VA_ARGS__)

#define mc_log_errorn(command, context, ...)                                   \
    fprintf(                                                                   \
        stderr, "%25s :: %-15s [callee: %p" context "]\n", "err::" MC_LOG_TAG, \
        command, (void *) __VA_ARGS__                                          \
    )
#define mc_log_errorn_static(command, context, ...)                            \
    fprintf(                                                                   \
        stderr, "%25s :: %-15s [" context "]\n", "err::" MC_LOG_TAG, command,  \
        __VA_ARGS__                                                            \
    )
#endif
#else
#define mc_log(command, context...)

#define mc_logn(command, context...)
#define mc_logn_static(command, context...)

#define mc_log_errorn(command, context...)
#define mc_log_errorn_static(command, context, ...)

#endif

#endif
