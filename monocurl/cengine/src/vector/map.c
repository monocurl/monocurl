//
//  map.c
//  Monocurl
//
//  Created by Manu Bhat on 10/29/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>

#include "lvalue.h"
#include "map.h"
#include "primitives.h"

static struct vector_field_vtable const vtable = {
    .type = VECTOR_FIELD_TYPE_MAP,
    .type_name = "map",

    .copy = map_copy,
    .assign = NULL,

    .op_call = NULL,

    .op_add = NULL,
    .op_multiply = NULL,
    .op_subtract = NULL,
    .op_negative = NULL,
    .op_divide = NULL,
    .op_power = NULL,

    .op_bool = NULL,
    .op_comp = map_comp,
    .op_contains = map_contains,

    .op_index = map_index,
    .op_attribute = NULL,

    .hash = map_hash,

    .bytes = map_bytes,
    .free = map_free,

    .out_of_frame_like = 0,
};

struct vector_field
map_init(struct timeline_execution_context *executor)
{
    struct map *const map = mc_malloc(sizeof(struct map));

    *map = (struct map){
        .field_count = 0,
        .node_capacity = 1 << MC_MEM_INITIAL_COUNT_EXP,
        .nodes =
            mc_calloc(1 << MC_MEM_INITIAL_COUNT_EXP, sizeof(struct map_node *)),
        .head = { 0 },
        .tail = &map->head,
        .hash_cache = 0,
    };

    return (struct vector_field){
        .value = { .pointer = map },
        .vtable = &vtable,
    };
}

struct vector_field
map_copy(struct timeline_execution_context *executor, struct vector_field src)
{
    struct map *const src_map = src.value.pointer;

    // go through each node, create copy
    struct map *const map = mc_malloc(sizeof(struct map));
    *map = (struct map){
        .field_count = src_map->field_count,
        .node_capacity = src_map->node_capacity,
        .nodes = mc_calloc(src_map->node_capacity, sizeof(struct map_node *)),
        .head = { 0 },
        .tail = &map->head,
        .hash_cache = src_map->hash_cache,
    };

    for (struct map_node *node = src_map->head.next_ins; node;
         node = node->next_ins) {
        struct vector_field const c1 = VECTOR_FIELD_COPY(executor, node->field);
        struct vector_field const c2 = VECTOR_FIELD_COPY(executor, node->value);

        mc_ind_t const bucket =
            VECTOR_FIELD_HASH(executor, c1) % map->node_capacity;

        struct map_node *const n = mc_malloc(sizeof(struct map_node));
        n->field = c1;
        n->value = c2;
        n->next_ins = NULL;
        n->next_bucket = map->nodes[bucket];

        map->tail->next_ins = n;

        map->nodes[bucket] = n;
        map->tail = n;
        executor->byte_alloc += 2;
    }

    return (struct vector_field){
        .value = { .pointer = map },
        .vtable = &vtable,
    };
}

/* l value, creates if not present */
struct vector_field
map_index(
    struct timeline_execution_context *executor, struct vector_field map,
    struct vector_field *index
)
{
    if (!index->vtable) {
        return VECTOR_FIELD_NULL;
    }

    /* take ownership */
    struct vector_field rhs =
        vector_field_extract_type(executor, index, VECTOR_FIELD_PURE);
    *index = VECTOR_FIELD_NULL;

    struct map *const m = map.value.pointer;

    m->hash_cache = 0;

    mc_hash_t const hash = VECTOR_FIELD_HASH(executor, rhs);
    if (!hash) {
        VECTOR_FIELD_FREE(executor, rhs);
        return VECTOR_FIELD_NULL;
    }
    mc_ind_t bucket = hash % m->node_capacity;
    for (struct map_node *node = m->nodes[bucket]; node;
         node = node->next_bucket) {
        if (!VECTOR_FIELD_DBOOL(
                VECTOR_FIELD_BINARY(executor, node->field, op_comp, &rhs)
            )) {
            return lvalue_init(executor, &node->value);
        }
    }

    // need to add  it (and possibly resize everything...)
    if (m->field_count == m->node_capacity) {
        mc_free(m->nodes);
        m->node_capacity =
            (mc_count_t) (m->node_capacity * MC_MEM_RESIZE_SCALE);
        m->nodes = mc_calloc(m->node_capacity, sizeof(struct map_node *));

        for (struct map_node *node = m->head.next_ins; node;
             node = node->next_ins) {
            mc_ind_t const buck =
                VECTOR_FIELD_HASH(executor, node->field) % m->node_capacity;

            node->next_bucket = m->nodes[buck];
            m->nodes[buck] = node;
        }

        bucket = hash % m->node_capacity;
    }

    struct map_node *const n = mc_malloc(sizeof(struct map_node));
    n->field = rhs;
    n->value = *index = VECTOR_FIELD_NULL;
    n->next_ins = NULL;
    n->next_bucket = m->nodes[bucket];

    m->tail->next_ins = n;
    m->nodes[bucket] = n;
    m->tail = n;

    ++m->field_count;

    executor->byte_alloc += 2;

    return lvalue_init(executor, &n->value);
}

mc_hash_t
map_hash(struct timeline_execution_context *executor, struct vector_field map)
{
    struct map *const m = map.value.pointer;
    if (m->hash_cache) {
        return m->hash_cache;
    }

    mc_hash_t hash = m->field_count + 703;
    for (struct map_node *node = m->head.next_ins; node;
         node = node->next_ins) {
        struct vector_field comp;

        comp = node->field;
        mc_hash_t sub = VECTOR_FIELD_UNARY(executor, comp, hash);
        if (!sub) {
            return 0;
        }
        hash ^= 0x9e3779b9 + sub + (hash << 16) + (hash >> 12);

        comp = node->value;
        if (!node->value.vtable ||
            !(sub = VECTOR_FIELD_UNARY(executor, comp, hash))) {
            return 0;
        }
        hash ^= 0x9e3779b9 + sub + (hash << 16) + (hash >> 12);
    }

    return m->hash_cache = hash;
}

struct vector_field
map_comp(
    struct timeline_execution_context *executor, struct vector_field map,
    struct vector_field *rhs
)
{
    struct vector_field rhs_val =
        vector_field_safe_extract_type(executor, *rhs, VECTOR_FIELD_TYPE_MAP);

    int ret;
    if (!rhs_val.vtable) {
        return double_init(executor, 1);
    }
    else if ((ret = (int) rhs_val.vtable->type - (int) VECTOR_FIELD_TYPE_MAP) != 0) {
        return double_init(executor, ret);
    }

    struct map *const m = map.value.pointer;
    struct map *const rhs_v = rhs_val.value.pointer;

    struct vector_field vret;
    for (struct map_node *node = m->head.next_ins,
                         *cnode = rhs_v->head.next_ins;
         ; node = node->next_ins, cnode = cnode->next_ins) {
        if (!cnode && !node) {
            return double_init(executor, 0);
        }
        if (!cnode) {
            return double_init(executor, 1);
        }
        else if (!node) {
            return double_init(executor, -1);
        }
        else if (VECTOR_FIELD_DBOOL(
                     (vret = VECTOR_FIELD_BINARY(
                          executor, node->field, op_comp, &cnode->field
                      ))
                 )) {
            return vret;
        }
        else if (VECTOR_FIELD_DBOOL(
                     (vret = VECTOR_FIELD_BINARY(
                          executor, node->value, op_comp, &cnode->value
                      ))
                 )) {
            return vret;
        }
    }

    return VECTOR_FIELD_NULL;
}

struct vector_field
map_contains(
    struct timeline_execution_context *executor, struct vector_field map,
    struct vector_field *element
)
{
    if (!element->vtable) {
        VECTOR_FIELD_ERROR(
            executor, "Cannot see if uninitialized field is in a container"
        );
        return VECTOR_FIELD_NULL;
    }

    struct map *const m = map.value.pointer;

    struct vector_field const value =
        vector_field_safe_extract_type(executor, *element, VECTOR_FIELD_PURE);

    mc_hash_t const hash = VECTOR_FIELD_HASH(executor, value);
    if (!hash) {
        return VECTOR_FIELD_NULL;
    }
    mc_ind_t const bucket = hash % m->node_capacity;

    for (struct map_node *node = m->nodes[bucket]; node;
         node = node->next_bucket) {
        struct vector_field const comp = node->field;
        if (!VECTOR_FIELD_DBOOL(
                VECTOR_FIELD_BINARY(executor, comp, op_comp, element)
            )) {
            return double_init(executor, 1);
        }
    }

    return double_init(executor, 0);
}

void
map_free(struct timeline_execution_context *executor, struct vector_field field)
{
    struct map *const map = field.value.pointer;

    struct map_node *head = map->head.next_ins;
    while (head) {
        struct map_node *const next = head->next_ins;
        VECTOR_FIELD_FREE(executor, head->field);
        VECTOR_FIELD_FREE(executor, head->value);
        mc_free(head);
        head = next;

        executor->byte_alloc -= 2;
    }

    mc_free(map->nodes);
    mc_free(map);
}

mc_count_t
map_bytes(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    struct map *const v = field.value.pointer;

    mc_count_t ret = sizeof(struct map) + sizeof(field);
    for (struct map_node *node = &v->head; node; node = node->next_ins) {
        ret += sizeof(*node);
        ret += VECTOR_FIELD_BYTES(executor, node->field);
        ret += VECTOR_FIELD_BYTES(executor, node->value);
    }

    return ret;
}
