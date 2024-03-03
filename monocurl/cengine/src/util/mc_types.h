#pragma once

#include <stddef.h>
#include <stdint.h>

typedef enum _mc_status {
    MC_STATUS_SUCCESS = 0,
    MC_STATUS_FAIL = -1
} mc_status_t;

typedef enum _mc_ternary_status {
    MC_TERNARY_STATUS_FAIL = -1,
    MC_TERNARY_STATUS_CONTINUE = 0,
    MC_TERNARY_STATUS_FINISH = 1,
} mc_ternary_status_t;

typedef char mc_bool_t;

typedef int mc_graph_color_t;
typedef long long mc_diff_t;
typedef size_t mc_count_t;
typedef size_t mc_ind_t;
typedef long long mc_rind_t;
typedef size_t mc_hash_t;
typedef size_t mc_bitmask_t; /* for bit operations */
typedef long long mc_long_t; /* for libmc calculations like factorial */

typedef double mc_tag_t;

typedef unsigned int mc_handle_t;
