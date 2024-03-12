//
//  vector.c
//  Monocurl
//
//  Created by Manu Bhat on 10/29/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include "mc_memory.h"
#include <math.h>
#include <stdlib.h>

#include "lvalue.h"
#include "primitives.h"
#include "vector.h"

static struct vector_field_vtable const vtable = {
    .type = VECTOR_FIELD_TYPE_VECTOR,
    .type_name = "vector",

    .copy = vector_copy,
    .assign = vector_assign,
    .plus_assign = vector_plus,

    .op_call = NULL,

    .op_add = vector_plus,
    .op_multiply = NULL,
    .op_subtract = NULL,
    .op_negative = NULL,
    .op_divide = NULL,
    .op_power = NULL,

    .op_bool = NULL,
    .op_contains = vector_contains,
    .op_comp = vector_comp,

    .op_index = vector_index,
    .op_attribute = NULL,

    .hash = vector_hash,

    .bytes = vector_bytes,
    .free = vector_free,

    .out_of_frame_like = 0,
};

struct vector_field
vector_init(struct timeline_execution_context *executor)
{
    struct vector *vector = mc_malloc(sizeof(struct vector));

    *vector = (struct vector){
        .field_count = 0,
        .fields = NULL,
        .hash_cache = 0,
    };

    return (struct vector_field){
        .value = { .pointer = vector },
        .vtable = &vtable,
    };
}

struct vector_field
vector_copy(
    struct timeline_execution_context *executor, struct vector_field vector
)
{
    struct vector *const vec = vector.value.pointer;

    struct vector_field const dump = vector_init(executor);

    for (mc_ind_t i = 0; i < vec->field_count; i++) {
        struct vector_field copy = VECTOR_FIELD_COPY(executor, vec->fields[i]);

        if (vec->fields[i].vtable && !copy.vtable) {
            VECTOR_FIELD_FREE(executor, dump);
            return VECTOR_FIELD_NULL;
        }
        else if (!copy.vtable) {
            VECTOR_FIELD_FREE(executor, dump);
            VECTOR_FIELD_ERROR(
                executor, "Cannot copy from vector with uninitialized data"
            );
            return VECTOR_FIELD_NULL;
        }
        vector_plus(executor, dump, &copy);
    }

    struct vector *dump_v = dump.value.pointer;
    dump_v->hash_cache = vec->hash_cache;

    return dump;
}

struct vector_field
vector_assign(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *source
)
{
    struct vector_field const source_lu =
        vector_field_lvalue_unwrap(executor, source);
    struct vector_field source_v = vector_field_safe_extract_type(
        executor, source_lu, VECTOR_FIELD_TYPE_VECTOR
    );
    if (!(source_v.vtable->type & VECTOR_FIELD_TYPE_VECTOR)) {
        VECTOR_FIELD_ERROR(
            executor, "Cannot assign to vector from field that is not a vector"
        );
        VECTOR_FIELD_FREE(executor, source_lu);
        return *source = VECTOR_FIELD_NULL;
    }

    struct vector *const vec = vector.value.pointer;
    struct vector *const src = source_v.value.pointer;

    vec->hash_cache = 0;

    if (src->field_count != vec->field_count) {
        VECTOR_FIELD_ERROR(
            executor,
            "Cannot assign to vector with size %zu from source size %zu",
            vec->field_count, src->field_count
        );
        VECTOR_FIELD_FREE(executor, source_lu);
        return *source = VECTOR_FIELD_NULL;
    }
    else {
        for (mc_ind_t i = 0; i < vec->field_count; i++) {
            if (!vec->fields[i].vtable->assign) {
                VECTOR_FIELD_FREE(executor, source_lu);
                VECTOR_FIELD_ERROR(
                    executor, "Element at index %zu of vector not assignable", i
                );
                return *source = VECTOR_FIELD_NULL;
            }
            else if (!VECTOR_FIELD_BINARY(
                          executor, vec->fields[i], assign, src->fields + i
                     )
                          .vtable) {
                VECTOR_FIELD_FREE(executor, source_lu);
                return *source = VECTOR_FIELD_NULL;
            }
        }
    }

    VECTOR_FIELD_FREE(executor, source_lu);
    *source = VECTOR_FIELD_NULL;

    return vector;
}

struct vector_field
vector_contains(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *element
)
{
    if (!element->vtable) {
        VECTOR_FIELD_ERROR(
            executor, "Cannot perform contains on uninitialized variable"
        );
        return VECTOR_FIELD_NULL;
    }

    struct vector *const vec = vector.value.pointer;

    for (mc_ind_t i = 0; i < vec->field_count; i++) {
        if (VECTOR_FIELD_BINARY(executor, vec->fields[i], op_comp, element)
                .value.doub == 0) {
            return double_init(executor, 1);
        }
    }

    return double_init(executor, 0);
}

struct vector_field
vector_plus(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *field
)
{
    // only throw an error if it's not an element
    if (field->vtable) {
        struct vector *const vec = vector.value.pointer;
        vec->hash_cache = 0;

        MC_MEM_RESERVE(vec->fields, vec->field_count);
        vec->fields[vec->field_count] =
            vector_field_lvalue_unwrap(executor, field);
        ++vec->field_count;
        ++executor->byte_alloc;

        if (!vec->fields[vec->field_count - 1].vtable) {
            return VECTOR_FIELD_NULL;
        }

        *field = VECTOR_FIELD_NULL;

        return vector;
    }
    else {
        VECTOR_FIELD_ERROR(executor, "Cannot add uninitialized element");
        return VECTOR_FIELD_NULL;
    }
}

struct vector_field
vector_literal_plus(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *field
)
{
    // only throw an error if it's not an element
    if (field->vtable) {
        struct vector *const vec = vector.value.pointer;
        vec->hash_cache = 0;

        MC_MEM_RESERVE(vec->fields, vec->field_count);
        vec->fields[vec->field_count] = *field;
        ++vec->field_count;
        ++executor->byte_alloc;

        *field = VECTOR_FIELD_NULL;

        return vector;
    }
    else {
        VECTOR_FIELD_ERROR(executor, "Cannot add uninitialized element");
        return VECTOR_FIELD_NULL;
    }
}

struct vector_field
vector_index(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *index
)
{
    struct vector *const vec = vector.value.pointer;
    vec->hash_cache = 0;

    // always returns an lvalue
    struct vector_field index_v = vector_field_extract_type(
        executor, index, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_DOUBLE
    );
    if (!index_v.vtable) {
        return VECTOR_FIELD_NULL;
    }
    else if (index_v.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        // #pragma message("TODO vector indexing seems to have some lvalue
        // issues?")
        //         VECTOR_FIELD_ERROR(executor, "Vector indexing is currently
        //         buggy and disabled!"); return VECTOR_FIELD_NULL;

        struct vector *const index_vec = index_v.value.pointer;

        struct vector_field dump = vector_init(executor);

        for (mc_ind_t i = 0; i < index_vec->field_count; i++) {
            struct vector_field lvalue =
                vector_index(executor, vector, index_vec->fields + i);
            if (!lvalue.vtable) {
                VECTOR_FIELD_FREE(executor, dump);
                return VECTOR_FIELD_NULL;
            }

            dump.vtable->op_add(executor, dump, &lvalue);
        }

        return dump;
    }
    else if (index_v.vtable->type & VECTOR_FIELD_TYPE_DOUBLE) {
        // vector index by double
        double const ind = index_v.value.doub;
        long long const iint = (long long) ind;
        if (iint != ind || iint < 0 || iint >= (long long) vec->field_count) {
            VECTOR_FIELD_ERROR(
                executor,
                "Index %g invalid. Either out of bounds or not coercible to an "
                "integer",
                ind
            );
            return VECTOR_FIELD_NULL;
        }
        else {
            return lvalue_init(executor, vec->fields + iint);
        }
    }

    return VECTOR_FIELD_NULL;
}

struct vector_field
vector_comp(
    struct timeline_execution_context *executor, struct vector_field vector,
    struct vector_field *rhs
)
{
    struct vector_field rhs_val = vector_field_safe_extract_type(
        executor, *rhs, VECTOR_FIELD_TYPE_VECTOR
    );

    int ret;
    if (!rhs_val.vtable) {
        return double_init(executor, 1);
    }
    else if ((ret = (int) rhs_val.vtable->type - (int) VECTOR_FIELD_TYPE_VECTOR) != 0) {
        return double_init(executor, ret);
    }

    struct vector *const vec = vector.value.pointer;
    struct vector *const rhs_v = rhs_val.value.pointer;

    struct vector_field vret;
    for (mc_ind_t i = 0; i < vec->field_count; i++) {
        if (i >= rhs_v->field_count) {
            return double_init(executor, 1);
        }
        if (VECTOR_FIELD_DBOOL(
                (vret = VECTOR_FIELD_BINARY(
                     executor, vec->fields[i], op_comp, rhs_v->fields + i
                 ))
            ) ||
            !vret.vtable) {
            return vret;
        }
    }

    return double_init(
        executor, (vec->field_count == rhs_v->field_count) ? 0 : -1
    );
}

mc_hash_t
vector_hash(
    struct timeline_execution_context *executor, struct vector_field vector
)
{
    struct vector *const vec = vector.value.pointer;
    if (vec->hash_cache) {
        return vec->hash_cache;
    }

    mc_hash_t hash = vec->field_count + 505;
    for (mc_ind_t i = 0; i < vec->field_count; i++) {
        /* Golden Ratio * (1 << 32) */
        mc_hash_t const sub =
            VECTOR_FIELD_UNARY(executor, vec->fields[i], hash);
        if (!sub) {
            return 0;
        }
        hash ^= 0x9e3779b9 + i + sub + (hash << 16) + (hash >> 12);
    }

    return vec->hash_cache = hash;
}

void
vector_free(
    struct timeline_execution_context *executor, struct vector_field vector
)
{
    struct vector *const v = vector.value.pointer;

    executor->byte_alloc -= v->field_count;

    for (mc_ind_t i = 0; i < v->field_count; i++) {
        VECTOR_FIELD_FREE(executor, v->fields[i]);
    }
    mc_free(v->fields);

    mc_free(v);
}

mc_count_t
vector_bytes(
    struct timeline_execution_context *executor, struct vector_field field
)
{
    struct vector *const v = field.value.pointer;

    mc_count_t ret = sizeof(struct vector) + sizeof(field);
    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        ret += VECTOR_FIELD_BYTES(executor, v->fields[i]);
    }

    return ret;
}
