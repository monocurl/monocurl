//
//  unowned_map.h
//  Monocurl
//
//  Created by Manu Bhat on 4/15/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_types.h"
#include <stdint.h>
#include <stdio.h>

/* does not manage ownership of values, but does of keys */
/* uses linear probing, with no guarantees of traversal */
struct unowned_map {
    mc_count_t tomb_or_data_count;
    mc_count_t capacity;

    struct unowned_map_entry {
        char const *key;
        void *value;

        mc_bool_t data;
    } *fields;
};

union ptr_int64 {
    void *ptr_value;
    int64_t int_value;
};

struct integer_map {
    mc_count_t tomb_or_data_count;
    mc_count_t capacity;

    struct integer_map_entry {
        uint64_t key;
        union ptr_int64 value;

        enum integer_map_entry_state {
            INTEGER_MAP_ENTRY_TOMB = -1,
            INTEGER_MAP_ENTRY_EMPTY = 0,
            INTEGER_MAP_ENTRY_DATA = 1
        } state;
    } *fields;
};

#if MC_INTERNAL
// MARK: Unowned map
struct unowned_map
unowned_map_init(void);

void
unowned_map_clear(struct unowned_map *map);

void
unowned_map_set(struct unowned_map *map, char const *key, void *value);

void *
unowned_map_get(struct unowned_map const *map, char const *key);

void
unowned_map_del(struct unowned_map const *map, char const *key);

struct unowned_map
unowned_map_copy(struct unowned_map const *map);

void
unowned_map_free(struct unowned_map map);

// MARK: Integer map
struct integer_map
integer_map_init(void);

void
integer_map_clear(struct integer_map *map);

/* returns old value */
union ptr_int64
integer_map_set(struct integer_map *map, uint64_t key, union ptr_int64 value);

mc_bool_t
integer_map_has(struct integer_map const *map, uint64_t key);

union ptr_int64
integer_map_get(struct integer_map const *map, uint64_t key);

void
integer_map_del(struct integer_map const *map, uint64_t key);

struct integer_map
integer_map_copy(struct integer_map const *map);

void
integer_map_free(struct integer_map map);
#endif
