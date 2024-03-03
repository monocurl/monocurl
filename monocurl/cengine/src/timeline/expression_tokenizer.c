//
//  expression_tokenizer.c
//  Monocurl
//
//  Created by Manu Bhat on 1/4/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <stdlib.h>

#include "expression_tokenizer.h"
#include "mc_memory.h"

char *
tokenizer_dup(struct expression_tokenizer const *tokenizer)
{
    mc_count_t const len = (mc_count_t) (tokenizer->end - tokenizer->start);
    char *const ret = mc_malloc(len + 1);

    for (mc_ind_t i = 0; i < len; ++i) {
        ret[i] = tokenizer->start[i];
    }
    ret[len] = 0;

    return ret;
}

void
tokenizer_read(struct expression_tokenizer *tokenizer)
{
    char const *comp = tokenizer->end;
    /* skip whitespace */
    while (*comp == '\t' || *comp == ' ' || *comp == '\n') {
        comp++;
    }
    tokenizer->start = comp;
    if (!*tokenizer->start) {
        tokenizer->end = tokenizer->start;
        return;
    }
    else {
        tokenizer->end = comp + 1;
    }

    char const c = *tokenizer->start;
    switch (c) {
    case '(':
    case '[':
    case '{':
    case ',':
    case '}':
    case ']':
    case ')':

    case '\'':
    case '"':

    case '-':
    case '.':
        return;
    case ':':
        if (*tokenizer->end == '<') {
            ++tokenizer->end;
        }
        return;
    case '/':
        /* comment */
        if (*tokenizer->end == '*') {
            do {
                ++tokenizer->end;
            } while (*tokenizer->end &&
                     (tokenizer->end[-1] != '*' || *tokenizer->end != '/'));

            if (*tokenizer->end == '/') {
                ++tokenizer->end;
            }
            tokenizer_read(tokenizer);
        }
        return;
    case '+':
    case '<':
    case '>':
    case '=':
    case '!':
        if (*tokenizer->end == '=') {
            ++tokenizer->end;
        }
        return;
    case '*':
        /* or * **/
        if (*tokenizer->end == '*') {
            ++tokenizer->end;
        }
        return;
    case '|':
        if (*tokenizer->end == '|') {
            ++tokenizer->end;
        }
        return;
    case '&':
        if (*tokenizer->end == '&') {
            ++tokenizer->end;
        }
        return;
    }
    /* reached a string like object... */
    for (;;) {
        switch (*tokenizer->end) {
        case 0:
        case ' ':
        case '\t':
        case '\n':

        case '(':
        case '[':
        case '{':
        case ',':
        case '}':
        case ']':
        case ')':

        case '.':

        case '\'':
        case '"':

        case '-':
        case '/':
        case ':':
        case '+':
        case '<':
        case '>':
        case '=':
        case '!':
        case '*':
        case '|':
        case '&':
            return;
        }

        ++tokenizer->end;
    }
}

mc_bool_t
tokenizer_equals(struct expression_tokenizer const *tokenizer, char const *cmp)
{
    for (char const *x = tokenizer->start, *y = cmp;; ++x, ++y) {
        if (x == tokenizer->end) {
            return !(*y);
        }
        else if (!*y) {
            return 0;
        }

        if (*x != *y) {
            return 0;
        }
    }
}
