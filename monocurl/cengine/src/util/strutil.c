//
//  strutil.c
//  Monocurl
//
//  Created by Manu Bhat on 11/16/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "mc_memory.h"
#include "strutil.h"

#define START_CAPACITY 16

static void
grow(struct str_dynamic *string, mc_count_t const to)
{
    char *const tmp = string->pointer;
    string->pointer = mc_malloc(string->capacity = to);
    strncpy(string->pointer, tmp, string->offset);
    mc_free(tmp);
}

struct str_dynamic
str_dynamic_init(void)
{
    return (struct str_dynamic){ mc_malloc(START_CAPACITY), 0, START_CAPACITY };
}

void
str_dynamic_append(struct str_dynamic *string, char const *src)
{
    mc_count_t const src_len = strlen(src);
    mc_count_t const dst_len = string->offset;

    //>= to allow space for null
    if (src_len + dst_len >= string->capacity) {
        grow(
            string, (mc_count_t) ((1 + src_len + dst_len) * MC_MEM_RESIZE_SCALE)
        );
    }

    strcpy(string->pointer + dst_len, src);
    string->offset += src_len;
}

static inline void
str_dynamic_append_chr(struct str_dynamic *dst, char x)
{
    if (dst->offset + 1 == dst->capacity) {
        grow(dst, (mc_count_t) ((1 + dst->capacity) * MC_MEM_RESIZE_SCALE));
    }

    dst->pointer[dst->offset++] = x;
}

void
str_dynamic_append_esc(struct str_dynamic *dst, char const *src)
{
    for (char const *x = src; *x; ++x) {
        if (*x == '\\' || *x == '"') {
            str_dynamic_append_chr(dst, '\\');
        }

        str_dynamic_append_chr(dst, *x);
    }
}

char const *
str_first_non_var_name(char const *str)
{
    for (char const *ret = str;; ++ret) {
        char c = *ret;
        if (('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') ||
            ('0' <= c && c <= '9') || c == '_') {
            continue;
        }
        return ret;
    }
}

mc_hash_t
str_hash(unsigned char const *bytes, mc_count_t count)
{
    mc_hash_t hash = 5381;

    for (mc_ind_t i = 0; i < count; ++i) {
        hash = ((hash << 5) + hash) + (mc_hash_t) bytes[i]; /* hash * 33 + c */
    }

    return hash;
}

mc_hash_t
str_null_terminated_hash(unsigned char const *bytes)
{
    mc_hash_t hash = 5381;

    for (unsigned char const *x = bytes; *x; ++x) {
        hash = ((hash << 5) + hash) + (mc_hash_t) *x; /* hash * 33 + c */
    }

    return hash % MC_HASHING_PRIME;
}

char *
mc_strdup(char const *src)
{
    char *const ret = mc_malloc(strlen(src) + 1);
    if (!ret) {
        return NULL;
    }
    strcpy(ret, src);
    return ret;
}
