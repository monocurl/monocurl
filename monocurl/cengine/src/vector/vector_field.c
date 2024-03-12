//
//  vector_field.c
//  Monocurl
//
//  Created by Manu Bhat on 12/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "functor.h"
#include "lvalue.h"
#include "vector_field.h"

static char const *type_names[] = {
    "double", "char", "function",  "functor", "vector",
    "map",    "mesh", "animation", "lvalue",
};

void
vector_field_type_to_a(enum vector_field_type type, char *out)
{
    *out = '`';
    int pos = 1;

    for (mc_ind_t i = 0; (1 << i) <= VECTOR_FIELD_TYPE_LVALUE; ++i) {
        if (type & (1 << i)) {
            pos += snprintf(
                &out[pos], VECTOR_FIELD_TYPE_STR_BUFFER - 1 - pos, "%s|",
                type_names[i]
            );
        }
    }

    if (!type) {
        pos += snprintf(
            &out[pos], VECTOR_FIELD_TYPE_STR_BUFFER - 1 - pos, "%s|",
            "uninitialized"
        );
    }

    /* change last character to a ` */
    out[pos - 1] = '`';
}

// essentially a no copy extract type
static struct vector_field
extract_type_recurse(
    struct timeline_execution_context *executor, struct vector_field *raw,
    enum vector_field_type target, char const *message
)
{
    // if the type matches, then return that
    if (raw && raw->vtable) {
        enum vector_field_type src = raw->vtable->type;

        if (src & target) {
            return *raw;
        }
        else if (src & VECTOR_FIELD_TYPE_LVALUE) {
            struct vector_field copy =
                *((struct vector_field *) raw->value.pointer);
            return *raw =
                       extract_type_recurse(executor, &copy, target, message);
        }
        else if (src & VECTOR_FIELD_TYPE_FUNCTOR) {
            struct vector_field copy = functor_get_res(executor, *raw);
            return extract_type_recurse(executor, &copy, target, message);
        }
    }

    char rx[VECTOR_FIELD_TYPE_STR_BUFFER];
    char tx[VECTOR_FIELD_TYPE_STR_BUFFER];
    vector_field_type_to_a(raw && raw->vtable ? raw->vtable->type : 0, rx);
    vector_field_type_to_a(target, tx);
    VECTOR_FIELD_ERROR(executor, message, rx, tx);

    return VECTOR_FIELD_NULL;
}

struct vector_field
vector_field_extract_type_message(
    struct timeline_execution_context *executor, struct vector_field *raw,
    enum vector_field_type target, char const *message
)
{
    // if the type matches, then return that
    if (raw && raw->vtable) {
        enum vector_field_type src = raw->vtable->type;

        if (src & target) {
            return *raw;
        }
        else if (src & VECTOR_FIELD_TYPE_LVALUE) {
            struct vector_field copy =
                *((struct vector_field *) raw->value.pointer);
            struct vector_field const to_copy =
                extract_type_recurse(executor, &copy, target, message);
            struct vector_field const ret =
                VECTOR_FIELD_COPY(executor, to_copy);
            return *raw = ret;
        }
        else if (src & VECTOR_FIELD_TYPE_FUNCTOR) {
            struct vector_field copy = functor_steal_res(executor, *raw);
            struct vector_field const to_copy =
                vector_field_extract_type_message(
                    executor, &copy, target, message
                );
            VECTOR_FIELD_FREE(executor, *raw);
            return *raw = to_copy;
        }
    }

    char rx[VECTOR_FIELD_TYPE_STR_BUFFER];
    char tx[VECTOR_FIELD_TYPE_STR_BUFFER];
    vector_field_type_to_a(raw && raw->vtable ? raw->vtable->type : 0, rx);
    vector_field_type_to_a(target, tx);
    VECTOR_FIELD_ERROR(executor, message, rx, tx);

    if (raw) {
        VECTOR_FIELD_FREE(executor, *raw);
        *raw = VECTOR_FIELD_NULL;
    }

    return VECTOR_FIELD_NULL;
}

struct vector_field
vector_field_extract_type(
    struct timeline_execution_context *executor, struct vector_field *raw,
    enum vector_field_type target
)
{
    return vector_field_extract_type_message(
        executor, raw, target,
        "Could not cast to expected type. Received %s expected %s"
    );
}

struct vector_field
vector_field_nocopy_extract_type_message(
    struct timeline_execution_context *executor, struct vector_field raw,
    enum vector_field_type target, char const *message
)
{
    // if the type matches, then return that
    if (raw.vtable) {
        enum vector_field_type src = raw.vtable->type;

        if (src & target) {
            return raw;
        }
        else if (src & VECTOR_FIELD_TYPE_LVALUE) {
            return vector_field_nocopy_extract_type_message(
                executor, *(struct vector_field *) raw.value.pointer, target,
                message
            );
        }
        else if (src & VECTOR_FIELD_TYPE_FUNCTOR) {
            return vector_field_nocopy_extract_type_message(
                executor, functor_get_res(executor, raw), target, message
            );
        }
    }

    char rx[VECTOR_FIELD_TYPE_STR_BUFFER];
    char tx[VECTOR_FIELD_TYPE_STR_BUFFER];
    vector_field_type_to_a(raw.vtable ? raw.vtable->type : 0, rx);
    vector_field_type_to_a(target, tx);
    VECTOR_FIELD_ERROR(executor, message, rx, tx);
    return VECTOR_FIELD_NULL;
}

struct vector_field
vector_field_nocopy_extract_type(
    struct timeline_execution_context *executor, struct vector_field raw,
    enum vector_field_type target
)
{
    return vector_field_nocopy_extract_type_message(
        executor, raw, target,
        "Could not cast to expected type. Received %s expected %s"
    );
}

struct vector_field
vector_field_safe_extract_type(
    struct timeline_execution_context *executor, struct vector_field raw,
    enum vector_field_type target
)
{
    // if the type matches, then return that
    if (raw.vtable) {
        enum vector_field_type src = raw.vtable->type;

        if (src & target) {
            return raw;
        }
        else if (src & VECTOR_FIELD_TYPE_LVALUE) {
            return vector_field_safe_extract_type(
                executor, *((struct vector_field *) raw.value.pointer), target
            );
        }
        else if (src & VECTOR_FIELD_TYPE_FUNCTOR) {
            return vector_field_safe_extract_type(
                executor, functor_get_res(executor, raw), target
            );
        }

        return raw;
    }

    VECTOR_FIELD_ERROR(executor, "Uninitialized data");
    return VECTOR_FIELD_NULL;
}

struct vector_field
vector_field_lvalue_unwrap(
    struct timeline_execution_context *executor, struct vector_field *raw
)
{
    if (raw->vtable) {
        enum vector_field_type const src = raw->vtable->type;

#pragma message(                                                                                                                        \
    "OPTIMIZATION: can be optimized to only copy the vector if it has lvalues within it... but that's still O(N) so may be not worth??" \
)
        if (src & (VECTOR_FIELD_TYPE_LVALUE | VECTOR_FIELD_TYPE_VECTOR)) {
            struct vector_field const copy = VECTOR_FIELD_COPY(executor, *raw);
            VECTOR_FIELD_FREE(executor, *raw);
            *raw = VECTOR_FIELD_NULL;
            return copy;
        }
        else {
            return *raw;
        }
    }

    VECTOR_FIELD_ERROR(executor, "Uninitialized data");
    return VECTOR_FIELD_NULL;
}

static struct vector_field
functor_elide(
    struct timeline_execution_context *executor, struct vector_field raw
)
{
    if (raw.vtable) {
        enum vector_field_type src = raw.vtable->type;

        if (src & VECTOR_FIELD_TYPE_FUNCTOR) {
            return VECTOR_FIELD_COPY(executor, functor_get_res(executor, raw));
        }
        else if (src & VECTOR_FIELD_TYPE_LVALUE) {
            struct vector_field const ret = functor_elide(
                executor, *(struct vector_field *) raw.value.pointer
            );
            if (ret.vtable) {
                return ret;
            }
            return raw;
        }
    }

    VECTOR_FIELD_ERROR(executor, "Uninitialized data");

    return VECTOR_FIELD_NULL;
}

/* for making sure function calls are pure */
struct vector_field
vector_field_functor_elide(
    struct timeline_execution_context *executor, struct vector_field *raw
)
{
    if (raw->vtable) {
        enum vector_field_type src = raw->vtable->type;

        if (src & VECTOR_FIELD_TYPE_FUNCTOR) {
            struct vector_field const ret = functor_steal_res(executor, *raw);
            VECTOR_FIELD_FREE(executor, *raw);
            return *raw = ret;
        }
        else if (src & VECTOR_FIELD_TYPE_LVALUE) {
            struct vector_field const ret = functor_elide(
                executor, *(struct vector_field *) raw->value.pointer
            );
            return ret;
        }

        return *raw;
    }

    VECTOR_FIELD_ERROR(executor, "Uninitialized data");
    return VECTOR_FIELD_NULL;
}

struct vector_field
vector_field_lvalue_copy(
    struct timeline_execution_context *executor, struct vector_field raw
)
{
    if (!raw.vtable) {
        return raw;
    }

    if (raw.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        return raw;
    }
    else if (raw.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *vec = raw.value.pointer;
        struct vector_field const dump = vector_init(executor);

        for (mc_ind_t i = 0; i < vec->field_count; i++) {
            struct vector_field copy =
                vector_field_lvalue_copy(executor, vec->fields[i]);

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
            vector_literal_plus(executor, dump, &copy);
        }

        return dump;
    }
    else {
        return VECTOR_FIELD_COPY(executor, raw);
    }
}

static void
str_dump(
    struct timeline_execution_context *executor, struct vector_field curr,
    struct str_dynamic *str
)
{
    curr = vector_field_nocopy_extract_type(executor, curr, VECTOR_FIELD_PURE);

    if (!curr.vtable) {
        goto free;
    }

    enum vector_field_type const type = curr.vtable->type;

    if (type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vector = curr.value.pointer;
        for (mc_ind_t i = 0; i < vector->field_count; ++i) {
            str_dump(executor, vector->fields[i], str);
            if (!str->pointer) {
                return;
            }
        }
    }
    else if (type & VECTOR_FIELD_TYPE_CHAR) {
        char source[2] = { curr.value.c, 0 };
        str_dynamic_append(str, source);
    }
    else if (type & VECTOR_FIELD_TYPE_DOUBLE) {
        char buffer[1];
        int const len = snprintf(buffer, 1, "%g", curr.value.doub);
        char *const dump = mc_malloc(sizeof(char) * (mc_count_t) (len + 1));
        snprintf(dump, len + 1, "%g", curr.value.doub);
        str_dynamic_append(str, dump);
        mc_free(dump);
    }
    else {
        goto free;
    }

    return;

free:
    VECTOR_FIELD_ERROR(
        executor,
        "Cannot coerce data to a string (expected `vector|char|double`"
    );
    mc_free(str->pointer);
    str->pointer = NULL;
}

char const *
vector_field_str(
    struct timeline_execution_context *executor, struct vector_field str
)
{
    struct str_dynamic curr = str_dynamic_init();
    str_dump(executor, str, &curr);

    if (curr.pointer) {
        curr.pointer[curr.offset] = 0;
    }
    return curr.pointer;
}
