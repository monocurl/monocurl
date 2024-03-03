//
//  unowned_map.c
//  Monocurl
//
//  Created by Manu Bhat on 4/15/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "mc_memory.h"
#include "strutil.h"
#include "unowned_map.h"

#define INITIAL_CAPACITY 8
#define MAX_LOAD_FACTOR 0.5

struct unowned_map
unowned_map_init(void)
{
    return (struct unowned_map){
        0,
        1ull << MC_MEM_INITIAL_COUNT_EXP,
        mc_calloc(
            1ull << MC_MEM_INITIAL_COUNT_EXP, sizeof(struct unowned_map_entry)
        ),
    };
}

void
unowned_map_clear(struct unowned_map *map)
{
    map->tomb_or_data_count = 0;
    mc_free(map->fields);
    map->capacity = 1ull << MC_MEM_INITIAL_COUNT_EXP;
    map->fields = mc_calloc(
        1ull << MC_MEM_INITIAL_COUNT_EXP, sizeof(struct unowned_map_entry)
    );
}

static mc_ind_t
unowned_map_index(struct unowned_map const *map, char const *key)
{
    mc_hash_t const hash =
        str_null_terminated_hash((unsigned char const *) key);
    mc_ind_t index = hash % map->capacity;

    while (map->fields[index].data &&
           (!map->fields[index].key || strcmp(map->fields[index].key, key))) {
        ++index;
        if (index == map->capacity) {
            index = 0;
        }
    }

    return index;
}

void
unowned_map_set(struct unowned_map *map, char const *key, void *value)
{
    mc_ind_t const index = unowned_map_index(map, key);

    mc_bool_t const increase = !map->fields[index].key;

    if (!map->fields[index].key) {
        map->fields[index].key = mc_strdup(key);
    }
    map->fields[index].value = value;
    map->fields[index].data = 1;

    if (increase &&
        ++map->tomb_or_data_count >= map->capacity * MAX_LOAD_FACTOR) {
        mc_count_t const old_capacity = map->capacity;
        struct unowned_map_entry *const entries = map->fields;

        map->capacity = MC_MEM_NEXT_CAPACITY(map->capacity);
        map->fields =
            mc_calloc(map->capacity, sizeof(struct unowned_map_entry));
        for (mc_ind_t i = 0; i < old_capacity; ++i) {
            if (entries[i].key) {
                mc_ind_t const sub_index =
                    unowned_map_index(map, entries[i].key);

                map->fields[sub_index].key = entries[i].key;
                map->fields[sub_index].value = entries[i].value;
                map->fields[sub_index].data = 1;
            }
        }

        mc_free(entries);
    }
}

void *
unowned_map_get(struct unowned_map const *map, char const *key)
{
    mc_ind_t const index = unowned_map_index(map, key);

    return map->fields[index].value;
}

void
unowned_map_del(struct unowned_map const *map, char const *key)
{
    mc_ind_t const index = unowned_map_index(map, key);

    /* either hit a does not exist, or the target node */
    if (map->fields[index].key) {
        free((char *) map->fields[index].key);
    }
    map->fields[index].key = NULL;
}

struct unowned_map
unowned_map_copy(struct unowned_map const *map)
{
    struct unowned_map copy = { 0 };
    copy.capacity = (mc_count_t) (map->tomb_or_data_count / MAX_LOAD_FACTOR);
    copy.fields = mc_calloc(copy.capacity, sizeof(struct unowned_map_entry));
    for (mc_ind_t i = 0; i < map->capacity; ++i) {
        if (map->fields[i].key) {
            unowned_map_set(&copy, map->fields[i].key, map->fields[i].value);
        }
    }

    return copy;
}

void
unowned_map_free(struct unowned_map map)
{
    for (mc_ind_t i = 0; i < map.capacity; ++i) {
        mc_free((char *) map.fields[i].key);
    }
    mc_free(map.fields);
}

struct integer_map
integer_map_init(void)
{
    return (struct integer_map){
        0,
        1ull << MC_MEM_INITIAL_COUNT_EXP,
        mc_calloc(
            1ull << MC_MEM_INITIAL_COUNT_EXP, sizeof(struct integer_map_entry)
        ),
    };
}

void
integer_map_clear(struct integer_map *map)
{
    map->tomb_or_data_count = 0;
    mc_free(map->fields);
    map->capacity = 1ull << MC_MEM_INITIAL_COUNT_EXP;
    map->fields = mc_calloc(
        1ull << MC_MEM_INITIAL_COUNT_EXP, sizeof(struct integer_map_entry)
    );
}

static mc_ind_t
integer_map_index(struct integer_map const *map, uint64_t key)
{
    mc_ind_t index = key % map->capacity;

    while (map->fields[index].state == INTEGER_MAP_ENTRY_TOMB ||
           (map->fields[index].state == INTEGER_MAP_ENTRY_DATA &&
            map->fields[index].key != key)) {
        if (++index == map->capacity) {
            index = 0;
        }
    }

    return index;
}

union ptr_int64
integer_map_set(struct integer_map *map, uint64_t key, union ptr_int64 value)
{
    mc_ind_t const index = integer_map_index(map, key);

    mc_bool_t const increase =
        map->fields[index].state == INTEGER_MAP_ENTRY_EMPTY;

    union ptr_int64 const ret = map->fields[index].value;

    map->fields[index].key = key;
    map->fields[index].value = value;
    map->fields[index].state = INTEGER_MAP_ENTRY_DATA;

    if (increase &&
        ++map->tomb_or_data_count >= map->capacity * MAX_LOAD_FACTOR) {
        mc_count_t const old_capacity = map->capacity;
        struct integer_map_entry *const entries = map->fields;

        map->capacity = MC_MEM_NEXT_CAPACITY(map->capacity);
        map->fields =
            mc_calloc(map->capacity, sizeof(struct unowned_map_entry));
        for (mc_ind_t i = 0; i < old_capacity; ++i) {
            if (entries[i].key) {
                mc_ind_t const sub_index =
                    integer_map_index(map, entries[i].key);

                map->fields[sub_index].key = entries[i].key;
                map->fields[sub_index].value = entries[i].value;
                map->fields[sub_index].state = INTEGER_MAP_ENTRY_DATA;
            }
        }

        mc_free(entries);
    }

    return ret;
}

mc_bool_t
integer_map_has(struct integer_map const *map, uint64_t key)
{
    mc_ind_t const index = integer_map_index(map, key);

    return map->fields[index].state == INTEGER_MAP_ENTRY_DATA;
}

union ptr_int64
integer_map_get(struct integer_map const *map, uint64_t key)
{
    mc_ind_t const index = integer_map_index(map, key);

    return map->fields[index].value;
}

void
integer_map_del(struct integer_map const *map, uint64_t key)
{
    mc_ind_t const index = integer_map_index(map, key);

    if (map->fields[index].state == INTEGER_MAP_ENTRY_DATA) {
        map->fields[index].state = INTEGER_MAP_ENTRY_TOMB;
    }
}

struct integer_map
integer_map_copy(struct integer_map const *map)
{
    struct integer_map copy = { 0 };
    copy.capacity = (mc_count_t) (map->tomb_or_data_count / MAX_LOAD_FACTOR);
    copy.fields = mc_calloc(copy.capacity, sizeof(struct integer_map_entry));
    for (mc_ind_t i = 0; i < map->capacity; ++i) {
        if (map->fields[i].state == INTEGER_MAP_ENTRY_DATA) {
            integer_map_set(&copy, map->fields[i].key, map->fields[i].value);
        }
    }

    return copy;
}

void
integer_map_free(struct integer_map map)
{
    mc_free(map.fields);
}
