//
//  timeline_instrution.c
//  monocurl
//
//  Created by Manu Bhat on 12/5/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//
#include <errno.h>
#include <limits.h>
#include <math.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

#include "constructor.h"
#include "expression_tokenizer.h"
#include "function.h"
#include "functor.h"
#include "lvalue.h"
#include "map.h"
#include "mc_memory.h"
#include "mc_stdlib.h"
#include "primitives.h"
#include "strutil.h"
#include "timeline_instruction.h"

#define MAX_FUNCTION_ARG_COUNT 64

#define NULL_INDEX_STRING " "

static struct timeline_expression_node *
expression_b(struct expression_tokenizer *tokenizer, int min);

static void
expression_node_free(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    mc_free(node);
}

static struct vector_field
expresssion_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    return double_init(executor, 0);
}

static struct timeline_expression_node *
expression_null(void)
{
    struct timeline_expression_node *const base =
        mc_malloc(sizeof(struct timeline_expression_node));
    base->free = expression_node_free;
    base->execute = expresssion_execute;

    return base;
}

/* MARK: Literals */
struct expression_lvalue_literal {
    struct timeline_expression_node base;
    long long negated_offset;
};

static struct vector_field
expression_lvalue_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_lvalue_literal *literal = (void *) node;

    struct vector_field *const ptr =
        executor->stack +
        ((long long) executor->stack_frame - literal->negated_offset);

    return lvalue_init(executor, ptr);
}

static struct vector_field
expression_lvalue_const_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_lvalue_literal *literal = (void *) node;

    struct vector_field *const ptr =
        executor->stack +
        ((long long) executor->stack_frame - literal->negated_offset);

    return lvalue_const_init(executor, ptr);
}

static struct vector_field
expression_lvalue_reference_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_lvalue_literal *literal = (void *) node;

    struct vector_field *const ptr =
        executor->stack +
        ((long long) executor->stack_frame - literal->negated_offset);

    return lvalue_reference_init(executor, ptr);
}

static struct vector_field
expression_parameter_reference_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_lvalue_literal *literal = (void *) node;

    struct vector_field *const ptr =
        executor->stack +
        ((long long) executor->stack_frame - literal->negated_offset);

    return lvalue_parameter_init(executor, ptr);
}
static struct vector_field
expression_lvalue_persistent_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_lvalue_literal *literal = (void *) node;

    struct vector_field *const ptr =
        executor->stack +
        ((long long) executor->stack_frame - literal->negated_offset);

    return lvalue_persistent_init(executor, ptr);
}

static struct timeline_expression_node *
expression_lvalue_literal(
    long long neg_index, mc_bool_t constant, mc_bool_t reference,
    mc_bool_t parameter, mc_bool_t function_arg
)
{
    struct expression_lvalue_literal *const literal =
        mc_malloc(sizeof(struct expression_lvalue_literal));
    literal->base.free = expression_node_free;

    if (reference) {
        literal->base.execute = expression_lvalue_reference_execute;
    }
    else if (function_arg) {
        literal->base.execute = expression_lvalue_persistent_execute;
    }
    else if (constant) {
        literal->base.execute = expression_lvalue_const_execute;
    }
    else if (parameter) {
        literal->base.execute = expression_parameter_reference_execute;
    }
    else {
        literal->base.execute = expression_lvalue_execute;
    }

    literal->negated_offset = neg_index;
    return &literal->base;
}

struct expression_double_literal {
    struct timeline_expression_node base;
    double literal;
};

static struct vector_field
expression_double_execute(
    struct timeline_expression_node *expression,
    struct timeline_execution_context *executor
)
{
    struct expression_double_literal *doub = (void *) expression;
    return double_init(executor, doub->literal);
}

static struct timeline_expression_node *
expression_double_literal(double literal)
{
    struct expression_double_literal *const doub =
        mc_malloc(sizeof(struct expression_double_literal));
    doub->literal = literal;
    doub->base.execute = expression_double_execute;
    doub->base.free = expression_node_free;

    return &doub->base;
}

struct expression_vector_literal {
    struct timeline_expression_node base;
    mc_count_t count;
    struct timeline_expression_node **nodes;
};

static struct vector_field
expression_vector_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_vector_literal *literal = (void *) node;
    struct vector_field vector = vector_init(executor);

    for (mc_ind_t i = 0; i < literal->count; i++) {
        struct vector_field const result =
            literal->nodes[i]->execute(literal->nodes[i], executor);
        if (!result.vtable) {
            VECTOR_FIELD_FREE(executor, vector);
            return VECTOR_FIELD_NULL;
        }

        struct vector_field *const rhs_stack =
            timeline_executor_temporary_push(executor, result);
        if (!rhs_stack) {
            VECTOR_FIELD_FREE(executor, vector);
            return VECTOR_FIELD_NULL;
        }

        vector = vector_literal_plus(executor, vector, rhs_stack);
    }

    return vector;
}

static void
expression_vector_free(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_vector_literal *literal = (void *) node;

    for (mc_ind_t i = 0; i < literal->count; i++) {
        literal->nodes[i]->free(literal->nodes[i], executor);
    }

    mc_free(literal->nodes);
    mc_free(literal);
}

static struct timeline_expression_node *
expression_vector_literal(
    mc_count_t count, struct timeline_expression_node **nodes
)
{
    struct expression_vector_literal *const literal =
        mc_malloc(sizeof(struct expression_vector_literal));
    literal->base.free = expression_vector_free;
    literal->base.execute = expression_vector_execute;
    literal->count = count;
    literal->nodes = nodes;
    return &literal->base;
}

struct expression_map_literal {
    struct timeline_expression_node base;
    mc_count_t count;
    struct timeline_expression_node **keys;
    struct timeline_expression_node **values;
};

static struct vector_field
expression_map_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_map_literal *literal = (void *) node;
    struct vector_field map = map_init(executor);

    for (mc_ind_t i = 0; i < literal->count; i++) {
        struct vector_field key_l =
            literal->keys[i]->execute(literal->keys[i], executor);
        struct vector_field value_l =
            literal->values[i]->execute(literal->values[i], executor);
        struct vector_field const key =
            vector_field_lvalue_unwrap(executor, &key_l);
        struct vector_field const value =
            vector_field_lvalue_unwrap(executor, &value_l);
        struct vector_field *value_stack;

        if (!key.vtable || !value.vtable ||
            !timeline_executor_temporary_push(executor, key) ||
            !(value_stack =
                  timeline_executor_temporary_push(executor, value))) {
            VECTOR_FIELD_FREE(executor, key_l);
            VECTOR_FIELD_FREE(executor, value_l);
            VECTOR_FIELD_FREE(executor, map);
            return VECTOR_FIELD_NULL;
        }

        struct vector_field lvalue =
            map.vtable->op_index(executor, map, value_stack - 1);
        lvalue.vtable->assign(executor, lvalue, value_stack);
    }

    return map;
}

static void
expression_map_free(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_map_literal *literal = (void *) node;

    for (mc_ind_t i = 0; i < literal->count; i++) {
        literal->keys[i]->free(literal->keys[i], executor);
        literal->values[i]->free(literal->values[i], executor);
    }

    mc_free(literal->keys);
    mc_free(literal->values);
    mc_free(literal);
}

static struct timeline_expression_node *
expression_map_literal(
    mc_count_t count, struct timeline_expression_node **keys,
    struct timeline_expression_node **values
)
{
    struct expression_map_literal *const literal =
        mc_malloc(sizeof(struct expression_map_literal));
    literal->base.free = expression_map_free;
    literal->base.execute = expression_map_execute;
    literal->count = count;
    literal->keys = keys;
    literal->values = values;
    return &literal->base;
}

struct expression_char_literal {
    struct timeline_expression_node base;
    char literal;
};

static struct vector_field
expression_char_execute(
    struct timeline_expression_node *expression,
    struct timeline_execution_context *executor
)
{
    struct expression_char_literal *character = (void *) expression;
    return char_init(executor, character->literal);
}

static struct timeline_expression_node *
expression_char_literal(char literal)
{
    struct expression_char_literal *const character =
        mc_malloc(sizeof(struct expression_char_literal));
    character->literal = literal;
    character->base.execute = expression_char_execute;
    character->base.free = expression_node_free;

    return &character->base;
}

/* MARK: Function Declaration */

struct expression_function {
    struct timeline_expression_node base;
    mc_rind_t neg_index;
    mc_count_t capture_count;
    mc_rind_t const *captures_neg_index;
    struct timeline_instruction *head;
};

static struct vector_field
expression_function_dec_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_function *const function = (void *) node;

    struct vector_field *caches =
        mc_malloc(sizeof(struct vector_field) * function->capture_count);
    for (mc_ind_t i = 0; i < function->capture_count; ++i) {
        caches[i] = VECTOR_FIELD_COPY(
            executor, executor->stack
                          [(long long) executor->stack_frame -
                           function->captures_neg_index[i]]
        );
    }
    struct vector_field const vfunc = function_init(
        executor, function->head, function->capture_count, caches
    );
    executor->stack[(long long) executor->stack_frame - function->neg_index] =
        vfunc;

    return double_init(executor, 0);
}

static void
expression_function_dec_free(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_function *const function = (void *) node;

    timeline_instruction_unref(executor, function->head);

    mc_free((long long *) function->captures_neg_index);
    mc_free(node);
}

static struct timeline_expression_node *
function_dec_init(
    struct timeline_execution_context *executor, mc_rind_t neg_index,
    mc_count_t capture_count, mc_rind_t const *captures,
    struct timeline_instruction *head
)
{
    struct expression_function *const node =
        mc_malloc(sizeof(struct expression_function));
    node->base.free = expression_function_dec_free;
    node->base.execute = expression_function_dec_execute;
    node->capture_count = capture_count;
    node->captures_neg_index = captures;
    node->neg_index = neg_index;
    node->head = head;

    return &node->base;
}

/* MARK: Function Call */

struct expression_function_call {
    struct timeline_expression_node base;
    mc_count_t count;
    long long neg_index;
    struct timeline_expression_node **nodes;
    void (*func)(
        struct timeline_execution_context *executor, struct vector_field,
        mc_count_t count, struct vector_field *fields
    );
};

static struct vector_field
expression_native_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_function_call *const native = (void *) node;

    struct vector_field const caller =
        native->neg_index == -1
            ? VECTOR_FIELD_NULL
            : executor->stack
                  [(long long) executor->stack_frame - native->neg_index];

    /* to avoid lvalue offsets */
    struct vector_field buffer[MAX_FUNCTION_ARG_COUNT];
    for (mc_ind_t i = 0; i < native->count; ++i) {
        struct vector_field const field =
            native->nodes[i]->execute(native->nodes[i], executor);
        if (!field.vtable) {
            for (mc_ind_t j = 0; j < i; ++j) {
                VECTOR_FIELD_FREE(executor, buffer[j]);
            }
            return VECTOR_FIELD_NULL;
        }
        buffer[i] = field;
    }

    // important, if instructions invalidated mid call, native->count will be
    // deallocated memory thus we cache before
    mc_count_t const arg_count = native->count;
    native->func(executor, caller, arg_count, buffer);

    for (mc_ind_t i = 0; i < arg_count; ++i) {
        VECTOR_FIELD_FREE(executor, buffer[arg_count - 1 - i]);
    }

    struct vector_field const ret = executor->return_register;
    executor->return_register = VECTOR_FIELD_NULL;
    return ret;
}

static void
expression_native_free(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_function_call *const native = (void *) node;
    for (mc_ind_t i = 0; i < native->count; ++i) {
        native->nodes[i]->free(native->nodes[i], executor);
    }
    mc_free(native->nodes);
    mc_free(node);
}

static struct timeline_expression_node *
expression_function(
    struct expression_tokenizer *tokenizer, mc_rind_t neg_index,
    mc_count_t aux_group_count, struct timeline_arg_group *groups,
    void (*func)(
        struct timeline_execution_context *executor, struct vector_field,
        mc_count_t count, struct vector_field *fields
    )
)
{
    if (!tokenizer_equals(tokenizer, "(")) {
        VECTOR_FIELD_ERROR(tokenizer->executor, "Expected function call");
        return NULL;
    }
    tokenizer_read(tokenizer);

    struct expression_function_call *const expression =
        mc_calloc(1, sizeof(struct expression_function_call));
    expression->base.execute = expression_native_execute;
    expression->base.free = expression_native_free;
    expression->func = func;
    expression->nodes = NULL;
    expression->count = 0;
    expression->neg_index = neg_index;

    mc_ind_t aux_group_index = 0, aux_group_subindex = 0;

    /* scroll to appropriate argument, passing over zero width enums */
    while (aux_group_index < aux_group_count &&
           aux_group_subindex == groups[aux_group_index].modes[0].used_count) {
        if (!aux_group_subindex && groups[aux_group_index].index) {
            MC_MEM_RESERVE(expression->nodes, expression->count);
            expression->nodes[expression->count++] =
                expression_double_literal(0);
        }

        for (mc_ind_t i = groups[aux_group_index].modes[0].used_count;
             i < groups[aux_group_index].union_size; ++i) {
            MC_MEM_RESERVE(expression->nodes, expression->count);
            expression->nodes[expression->count++] =
                expression_double_literal(0);
        }
        ++aux_group_index;
        aux_group_subindex = 0;
    }

    while (!tokenizer_equals(tokenizer, ")")) {
        if (groups) {
            if (aux_group_index == aux_group_count) {
                VECTOR_FIELD_ERROR(tokenizer->executor, "Too many arguments");
                expression_native_free(&expression->base, tokenizer->executor);
                return NULL;
            }
            struct timeline_symbol_entry comp =
                groups[aux_group_index].modes[0].real_args[aux_group_subindex];
            if (comp.reference_var || comp.arg_groups) {
                VECTOR_FIELD_ERROR(
                    tokenizer->executor,
                    "Function that takes a reference or a function as "
                    "parameters (`%s`) must be called with functor syntax",
                    comp.name
                );
                expression_native_free(&expression->base, tokenizer->executor);
                return NULL;
            }
            ++aux_group_subindex;
            if (aux_group_subindex == 1 && groups[aux_group_index].index) {
                MC_MEM_RESERVE(expression->nodes, expression->count);
                expression->nodes[expression->count++] =
                    expression_double_literal(0);
            }
        }

        MC_MEM_RESERVE(expression->nodes, expression->count);
        expression->nodes[expression->count] = expression_b(tokenizer, -1);
        if (!expression->nodes[expression->count]) {
            expression_native_free(&expression->base, tokenizer->executor);
            return NULL;
        }
        ++expression->count;

        /* scroll to next position */
        while (aux_group_index < aux_group_count &&
               aux_group_subindex == groups[aux_group_index].modes[0].used_count
        ) {
            if (!aux_group_subindex && groups[aux_group_index].index) {
                MC_MEM_RESERVE(expression->nodes, expression->count);
                expression->nodes[expression->count++] =
                    expression_double_literal(0);
            }

            for (mc_ind_t i = groups[aux_group_index].modes[0].used_count;
                 i < groups[aux_group_index].union_size; ++i) {
                MC_MEM_RESERVE(expression->nodes, expression->count);
                expression->nodes[expression->count++] =
                    expression_double_literal(0);
            }
            ++aux_group_index;
            aux_group_subindex = 0;
        }

        if (tokenizer_equals(tokenizer, ")")) {
            break;
        }
        else if (!*tokenizer->start) {
            VECTOR_FIELD_ERROR(tokenizer->executor, "Expected )");
            expression_native_free(&expression->base, tokenizer->executor);
            return NULL;
        }
        else if (!tokenizer_equals(tokenizer, ",")) {
            VECTOR_FIELD_ERROR(tokenizer->executor, "Expected ,");
            expression_native_free(&expression->base, tokenizer->executor);
            return NULL;
        }
        tokenizer_read(tokenizer);
    }
    tokenizer_read(tokenizer);

    if (groups && aux_group_index != aux_group_count) {
        VECTOR_FIELD_ERROR(
            tokenizer->executor, "Too few arguments (received %zu)",
            aux_group_index
        );
        expression_native_free(&expression->base, tokenizer->executor);
        return NULL;
    }

    return &expression->base;
}

static struct timeline_expression_node *
expression_native(struct expression_tokenizer *tokenizer)
{
    /* find */
    void (*func)(
        struct timeline_execution_context *executor, struct vector_field,
        mc_count_t count, struct vector_field *fields
    ) = mc_find_stdlib(tokenizer);

    if (!func) {
        return NULL;
    }

    tokenizer_read(tokenizer);

    return expression_function(tokenizer, -1, 0, NULL, func);
}

/* MARK: Unary  */

struct expression_unary {
    struct timeline_expression_node base, *operand;
};

static void
expression_unary_free(
    struct timeline_expression_node *operand,
    struct timeline_execution_context *executor
)
{
    struct expression_unary *cast = (void *) operand;
    cast->operand->free(cast->operand, executor);
    mc_free(operand);
}

static struct vector_field
expression_negate_execute(
    struct timeline_expression_node *operand,
    struct timeline_execution_context *executor
)
{
    struct expression_unary *cast = (void *) operand;
    struct vector_field const x =
        cast->operand->execute(cast->operand, executor);
    if (!x.vtable) {
        return x;
    }
    if (!x.vtable->op_negative) {
        VECTOR_FIELD_ERROR(executor, "Operation negate not defined");
        VECTOR_FIELD_FREE(executor, x);
        return VECTOR_FIELD_NULL;
    }
    return x.vtable->op_negative(executor, x);
}

static struct vector_field
expression_not_execute(
    struct timeline_expression_node *operand,
    struct timeline_execution_context *executor
)
{
    struct expression_unary *cast = (void *) operand;
    struct vector_field const x =
        cast->operand->execute(cast->operand, executor);
    if (!x.vtable) {
        return x;
    }
    if (!x.vtable->op_bool) {
        VECTOR_FIELD_ERROR(
            executor, "Cannot coerce operand of logical not to a boolean"
        );
        VECTOR_FIELD_FREE(executor, x);
        return VECTOR_FIELD_NULL;
    }

    struct vector_field const boolean = x.vtable->op_bool(executor, x);
    if (!boolean.vtable) {
        return VECTOR_FIELD_NULL;
    }
    return double_init(executor, !VECTOR_FIELD_DBOOL(boolean));
}

static struct timeline_expression_node *
expression_negate(struct timeline_expression_node *operand)
{
    struct expression_unary *const unary =
        mc_malloc(sizeof(struct expression_unary));
    unary->operand = operand;
    unary->base.free = expression_unary_free;
    unary->base.execute = expression_negate_execute;
    return &unary->base;
}

static struct timeline_expression_node *
expression_not(struct timeline_expression_node *operand)
{
    struct expression_unary *const unary =
        mc_malloc(sizeof(struct expression_unary));
    unary->operand = operand;
    unary->base.free = expression_unary_free;
    unary->base.execute = expression_not_execute;
    return &unary->base;
}

static struct vector_field
expression_sticky_execute(
    struct timeline_expression_node *operand,
    struct timeline_execution_context *executor
)
{
    struct expression_unary *cast = (void *) operand;
    struct vector_field x = cast->operand->execute(cast->operand, executor);
    if (!x.vtable) {
        return x;
    }

    struct vector_field sticky_marker = animation_sticky_init(executor);
    struct vector_field build = vector_init(executor);
    vector_plus(executor, build, &sticky_marker);
    vector_plus(executor, build, &x);

    return build;
}

static struct timeline_expression_node *
expression_sticky(struct timeline_expression_node *operand)
{
    struct expression_unary *const unary =
        mc_malloc(sizeof(struct expression_unary));
    unary->operand = operand;
    unary->base.free = expression_unary_free;
    unary->base.execute = expression_sticky_execute;
    return &unary->base;
}

/* MARK: Binary  */
struct expression_op_pure_binary {
    struct timeline_expression_node base, *l, *r;
};

static void
expression_binary_free(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_op_pure_binary *const bin = (void *) node;

    bin->l->free(bin->l, executor);
    bin->r->free(bin->r, executor);

    mc_free(bin);
}

#define BINARY_CONSTRUCTOR_BASE(name)                                          \
    static struct timeline_expression_node *expression_##name##_init(          \
        struct timeline_expression_node *left,                                 \
        struct timeline_expression_node *right                                 \
    )                                                                          \
    {                                                                          \
        struct expression_op_pure_binary *const bin =                          \
            mc_malloc(sizeof(struct expression_op_pure_binary));               \
        bin->base.free = expression_binary_free;                               \
        bin->base.execute = expression_##name##_execute;                       \
        bin->l = left;                                                         \
        bin->r = right;                                                        \
        return &bin->base;                                                     \
    }

static struct vector_field
expression_or_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_op_pure_binary *const bin = (void *) node;

    struct vector_field const lhs = bin->l->execute(bin->l, executor);
    if (!lhs.vtable) {
        return VECTOR_FIELD_NULL;
    }
    else if (!lhs.vtable->op_bool) {
        VECTOR_FIELD_ERROR(
            executor, "Left hand side of or operator not coercible to boolean"
        );
        return VECTOR_FIELD_NULL;
    }
    else {
        struct vector_field boolean = lhs.vtable->op_bool(executor, lhs);
        VECTOR_FIELD_FREE(executor, lhs);
        if (!boolean.vtable) {
            return VECTOR_FIELD_NULL;
        }
        else if (VECTOR_FIELD_DBOOL(boolean)) {
            return double_init(executor, 1);
        }
    }

    struct vector_field const rhs = bin->r->execute(bin->r, executor);
    if (!rhs.vtable) {
        return VECTOR_FIELD_NULL;
    }
    else if (!rhs.vtable->op_bool) {
        VECTOR_FIELD_ERROR(
            executor, "Right hand side of or operator not coercible to boolean"
        );
        return VECTOR_FIELD_NULL;
    }
    else {
        struct vector_field boolean = rhs.vtable->op_bool(executor, rhs);
        VECTOR_FIELD_FREE(executor, rhs);
        if (!boolean.vtable) {
            return VECTOR_FIELD_NULL;
        }
        else if (VECTOR_FIELD_DBOOL(boolean)) {
            return double_init(executor, 1);
        }
    }
    return double_init(executor, 0);
}

BINARY_CONSTRUCTOR_BASE(or)

static struct vector_field
expression_and_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_op_pure_binary *const bin = (void *) node;

    struct vector_field const lhs = bin->l->execute(bin->l, executor);
    if (!lhs.vtable) {
        return VECTOR_FIELD_NULL;
    }
    else if (!lhs.vtable->op_bool) {
        VECTOR_FIELD_ERROR(
            executor, "Left hand side of and operator not coercible to boolean"
        );
        return VECTOR_FIELD_NULL;
    }
    else {
        struct vector_field boolean = lhs.vtable->op_bool(executor, lhs);
        VECTOR_FIELD_FREE(executor, lhs);
        if (!boolean.vtable) {
            return VECTOR_FIELD_NULL;
        }
        else if (!VECTOR_FIELD_DBOOL(boolean)) {
            return double_init(executor, 0);
        }
    }

    struct vector_field const rhs = bin->r->execute(bin->r, executor);
    if (!rhs.vtable) {
        return VECTOR_FIELD_NULL;
    }
    else if (!rhs.vtable->op_bool) {
        VECTOR_FIELD_ERROR(
            executor, "Right hand side of and operator not coercible to boolean"
        );
        return VECTOR_FIELD_NULL;
    }
    else {
        struct vector_field boolean = rhs.vtable->op_bool(executor, rhs);
        VECTOR_FIELD_FREE(executor, rhs);
        if (!boolean.vtable) {
            return VECTOR_FIELD_NULL;
        }
        else if (!VECTOR_FIELD_DBOOL(boolean)) {
            return double_init(executor, 0);
        }
    }

    return double_init(executor, 1);
}

BINARY_CONSTRUCTOR_BASE(and)

/* comparative */
#define COMPARATIVE_BASE(name, comp)                                           \
    static struct vector_field expression_##name##_execute(                    \
        struct timeline_expression_node *node,                                 \
        struct timeline_execution_context *executor                            \
    )                                                                          \
    {                                                                          \
        struct expression_op_pure_binary *const bin = (void *) node;           \
        struct vector_field const lhs = bin->l->execute(bin->l, executor);     \
        if (!lhs.vtable) {                                                     \
            return VECTOR_FIELD_NULL;                                          \
        }                                                                      \
        struct vector_field rhs = bin->r->execute(bin->r, executor);           \
        struct vector_field const ret =                                        \
            lhs.vtable->op_comp(executor, lhs, &rhs);                          \
        VECTOR_FIELD_FREE(executor, lhs);                                      \
        VECTOR_FIELD_FREE(executor, rhs);                                      \
        if (!ret.vtable) {                                                     \
            return VECTOR_FIELD_NULL;                                          \
        }                                                                      \
        return double_init(executor, ret.value.doub comp 0);                   \
    }                                                                          \
    BINARY_CONSTRUCTOR_BASE(name)

COMPARATIVE_BASE(gt, >)
COMPARATIVE_BASE(lt, <)
COMPARATIVE_BASE(gte, >=)
COMPARATIVE_BASE(lte, <=)
COMPARATIVE_BASE(eq, ==)
COMPARATIVE_BASE(ne, !=)

#undef COMPARATIVE_BASE
/* arithmetic */
#define ARITHMETIC_BASE(name, vtable_name, free_l)                             \
    static struct vector_field expression_##name##_execute(                    \
        struct timeline_expression_node *node,                                 \
        struct timeline_execution_context *executor                            \
    )                                                                          \
    {                                                                          \
        struct expression_op_pure_binary *const bin = (void *) node;           \
        struct vector_field const lhs = bin->l->execute(bin->l, executor);     \
        if (!lhs.vtable) {                                                     \
            return VECTOR_FIELD_NULL;                                          \
        }                                                                      \
        struct vector_field const rhs = bin->r->execute(bin->r, executor);     \
        struct vector_field *const rhs_stack =                                 \
            timeline_executor_temporary_push(executor, rhs);                   \
        if (!lhs.vtable->vtable_name || !rhs_stack) {                          \
            VECTOR_FIELD_FREE(executor, lhs);                                  \
            VECTOR_FIELD_ERROR(                                                \
                executor, "Operation " #vtable_name " not defined"             \
            );                                                                 \
            return VECTOR_FIELD_NULL;                                          \
        }                                                                      \
        struct vector_field const ret =                                        \
            lhs.vtable->vtable_name(executor, lhs, rhs_stack);                 \
        if (free_l && !timeline_executor_temporary_push(executor, lhs)) {      \
            return VECTOR_FIELD_NULL;                                          \
        }                                                                      \
        return ret;                                                            \
    }                                                                          \
    BINARY_CONSTRUCTOR_BASE(name)

ARITHMETIC_BASE(plus, op_add, 0)
ARITHMETIC_BASE(sub, op_subtract, 0)
ARITHMETIC_BASE(mul, op_multiply, 0)
ARITHMETIC_BASE(div, op_divide, 0)
ARITHMETIC_BASE(pow, op_power, 0)

ARITHMETIC_BASE(contains, op_contains, 0)

ARITHMETIC_BASE(index, op_index, 1)

#undef ASSIGNMENT_BASE

/* assignment */
#define ASSIGNMENT_BASE(name, vtable_name)                                     \
    static struct vector_field expression_##name##_execute(                    \
        struct timeline_expression_node *node,                                 \
        struct timeline_execution_context *executor                            \
    )                                                                          \
    {                                                                          \
        struct expression_op_pure_binary *const bin = (void *) node;           \
        struct vector_field const rhs = bin->r->execute(bin->r, executor);     \
        struct vector_field *const rhs_stack =                                 \
            timeline_executor_temporary_push(executor, rhs);                   \
        if (!rhs.vtable) {                                                     \
            return VECTOR_FIELD_NULL; /* important for thread checking... */   \
        }                                                                      \
        struct vector_field const lhs = bin->l->execute(bin->l, executor);     \
        if (!lhs.vtable) {                                                     \
            return VECTOR_FIELD_NULL;                                          \
        }                                                                      \
        if (!lhs.vtable->vtable_name || !rhs_stack) {                          \
            VECTOR_FIELD_FREE(executor, lhs);                                  \
            VECTOR_FIELD_ERROR(                                                \
                executor,                                                      \
                "Operation " #vtable_name " not defined (cannot assign to a "  \
                "constant reference or an rvalue)"                             \
            );                                                                 \
            return VECTOR_FIELD_NULL;                                          \
        }                                                                      \
        struct vector_field const ret =                                        \
            lhs.vtable->vtable_name(executor, lhs, rhs_stack);                 \
        return ret;                                                            \
    }                                                                          \
    BINARY_CONSTRUCTOR_BASE(name)

ASSIGNMENT_BASE(assign, assign)
ASSIGNMENT_BASE(plus_assign, plus_assign)

#undef ASSIGNMENT_BASE

/* range */
static struct vector_field
expression_range_execute(
    struct timeline_expression_node *root,
    struct timeline_execution_context *executor
)
{
    struct expression_op_pure_binary *const bin = (void *) root;

    struct vector_field lhs = bin->l->execute(bin->l, executor);
    struct vector_field const lhs_cast =
        vector_field_extract_type(executor, &lhs, VECTOR_FIELD_TYPE_DOUBLE);
    if (!lhs_cast.vtable) {
        VECTOR_FIELD_FREE(executor, lhs);
        return VECTOR_FIELD_NULL;
    }

    struct vector_field rhs = bin->r->execute(bin->r, executor);
    struct vector_field const rhs_cast =
        vector_field_extract_type(executor, &rhs, VECTOR_FIELD_TYPE_DOUBLE);
    if (!rhs_cast.vtable) {
        VECTOR_FIELD_FREE(executor, lhs);
        VECTOR_FIELD_FREE(executor, rhs);
        return VECTOR_FIELD_NULL;
    }

    struct vector_field const ret = vector_init(executor);

    for (long long i = (long long) lhs_cast.value.doub;
         i < (long long) rhs_cast.value.doub; i++) {
        if (timeline_executor_check_interrupt(executor, 0)) {
            VECTOR_FIELD_FREE(executor, ret);
            return VECTOR_FIELD_NULL;
        }

        struct vector_field tmp = double_init(executor, (double) i);
        vector_plus(executor, ret, &tmp);
    }

    VECTOR_FIELD_FREE(executor, lhs);
    VECTOR_FIELD_FREE(executor, rhs);

    return ret;
}

BINARY_CONSTRUCTOR_BASE(range)
#undef BINARY_CONSTRUCTOR_BASE

/* MARK: Attribute */

struct expression_op_attribute {
    struct timeline_expression_node base, *l;
    char const *attribute;
};

static struct vector_field
execute_attribute(
    struct timeline_expression_node *expression,
    struct timeline_execution_context *executor
)
{

    struct expression_op_attribute *const attribute = (void *) expression;

    struct vector_field const x = attribute->l->execute(attribute->l, executor);
    if (!x.vtable) {
        return VECTOR_FIELD_NULL;
    }

    if (!x.vtable->op_attribute ||
        !timeline_executor_temporary_push(executor, x)) {
        VECTOR_FIELD_FREE(executor, x);
        VECTOR_FIELD_ERROR(executor, "Field does not have attributes");
        return VECTOR_FIELD_NULL;
    }

    struct vector_field const ret =
        x.vtable->op_attribute(executor, x, attribute->attribute);

    return ret;
}

static void
free_attribute(
    struct timeline_expression_node *expression,
    struct timeline_execution_context *executor
)
{
    struct expression_op_attribute *const attribute = (void *) expression;
    attribute->l->free(attribute->l, executor);
    mc_free((char *) attribute->attribute);
    mc_free(attribute);
}

static struct timeline_expression_node *
expression_attribute(
    struct timeline_expression_node *left, char const *attribute
)
{
    struct expression_op_attribute *const ret =
        mc_calloc(1, sizeof(struct expression_op_attribute));
    ret->l = left;
    ret->attribute = attribute;
    ret->base.execute = execute_attribute;
    ret->base.free = free_attribute;
    return &ret->base;
}

/* MARK: Control */
struct expression_for {
    struct timeline_expression_node base, *container;
    struct timeline_instruction *head;
};

static struct vector_field
expression_iterate_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_for *const expression = (void *) node;
    if (!timeline_executor_var_push(executor, VECTOR_FIELD_NULL)) {
        return VECTOR_FIELD_NULL;
    }
    struct vector_field container =
        expression->container->execute(expression->container, executor);

    --executor->stack_frame; // too account for parsing the variable before the
                             // container
    /* ensure it'll be freed at some point*/

    struct vector_field const cast = vector_field_extract_type(
        executor, timeline_executor_temporary_push(executor, container),
        VECTOR_FIELD_TYPE_MAP | VECTOR_FIELD_TYPE_VECTOR
    );

    if (!cast.vtable) {
        return VECTOR_FIELD_NULL;
    }
    else if (cast.vtable->type & VECTOR_FIELD_TYPE_MAP) {
        struct map *const m = cast.value.pointer;
        mc_ind_t i = 0;
        for (struct map_node *n = m->head.next_ins; n; n = n->next_ins, ++i) {
            if (timeline_executor_check_interrupt(executor, 0)) {
                return VECTOR_FIELD_NULL;
            }
            if (!timeline_executor_var_push(executor, n->field)) {
                return VECTOR_FIELD_NULL;
            }
            /* will not be modified, so we can do a mini optimization of no copy
             */
            if (!timeline_executor_execute(executor, expression->head, 0)
                     .vtable) {
                --executor->stack_frame;
                return VECTOR_FIELD_NULL;
            }
            --executor->stack_frame;
        }
    }
    else {
        struct vector *const v = cast.value.pointer;
        for (mc_ind_t i = 0; i < v->field_count; ++i) {
            if (timeline_executor_check_interrupt(executor, 0)) {
                return VECTOR_FIELD_NULL;
            }
            if (!timeline_executor_var_push(executor, v->fields[i])) {
                return VECTOR_FIELD_NULL;
            }; //* will not be modified, so we can do a mini optimization of no
               // copy */
            if (!timeline_executor_execute(executor, expression->head, 0)
                     .vtable) {
                --executor->stack_frame;
                return VECTOR_FIELD_NULL;
            }
            --executor->stack_frame;
        }
    }

    return double_init(executor, 0);
}

static void
expression_iterator_free(
    struct timeline_expression_node *expression,
    struct timeline_execution_context *executor
)
{
    struct expression_for *const iterator = (void *) expression;
    iterator->container->free(iterator->container, executor);
    timeline_instruction_unref(executor, iterator->head);

    mc_free(iterator);
}

static struct timeline_expression_node *
expression_iterate(
    struct timeline_execution_context *executor,
    struct timeline_expression_node *container,
    struct timeline_instruction *instruction
)
{
    struct expression_for *const iterator =
        mc_malloc(sizeof(struct expression_for));
    iterator->base.execute = expression_iterate_execute;
    iterator->base.free = expression_iterator_free;
    iterator->container = container;
    iterator->head = instruction;
    return &iterator->base;
}

struct expression_while {
    struct timeline_expression_node base;
    struct timeline_expression_node *condition;
    struct timeline_instruction *inner; /* caller is unowned*/
};

static struct vector_field
expression_while_execute(
    struct timeline_expression_node *expression,
    struct timeline_execution_context *executor
)
{
    struct expression_while *const node = (void *) expression;

    for (;;) {
        if (timeline_executor_check_interrupt(executor, 0)) {
            return VECTOR_FIELD_NULL;
        }

        struct vector_field const cond =
            node->condition->execute(node->condition, executor);

        struct vector_field const cast = vector_field_extract_type(
            executor, timeline_executor_temporary_push(executor, cond),
            VECTOR_FIELD_TYPE_DOUBLE
        );

        if (!cast.vtable) {
            return VECTOR_FIELD_NULL;
        }

        if (VECTOR_FIELD_DBOOL(cast)) {
            if (!timeline_executor_execute(executor, node->inner, 0).vtable) {
                return VECTOR_FIELD_NULL;
            }
        }
        else {
            return double_init(executor, 0);
        }
    }

    return double_init(executor, 0);
}

static void
expression_while_free(
    struct timeline_expression_node *expression,
    struct timeline_execution_context *executor
)
{
    struct expression_while *const node = (void *) expression;
    node->condition->free(node->condition, executor);
    timeline_instruction_unref(executor, node->inner);
    mc_free(expression);
}

static struct timeline_expression_node *
expression_while(
    struct timeline_execution_context *executor,
    struct timeline_expression_node *condition,
    struct timeline_instruction *inner
)
{
    struct expression_while *const expression =
        mc_malloc(sizeof(struct expression_while));
    expression->inner = inner;
    expression->condition = condition;
    expression->base.free = expression_while_free;
    expression->base.execute = expression_while_execute;
    return &expression->base;
}

struct expression_if {
    struct timeline_expression_node base;
    struct timeline_expression_node *condition;
    struct timeline_instruction *caller, *inner; /* caller is unowned*/
};

static struct vector_field
expression_if_execute(
    struct timeline_expression_node *expression,
    struct timeline_execution_context *executor
)
{
    struct expression_if *const node = (void *) expression;

    struct vector_field container =
        node->condition->execute(node->condition, executor);

    struct vector_field const cast = vector_field_extract_type(
        executor, timeline_executor_temporary_push(executor, container),
        VECTOR_FIELD_TYPE_DOUBLE
    );

    if (!cast.vtable) {
        return VECTOR_FIELD_NULL;
    }

    if (VECTOR_FIELD_DBOOL(cast)) {
        if (!timeline_executor_execute(executor, node->inner, 0).vtable) {
            return VECTOR_FIELD_NULL;
        }
        node->caller->next = node->caller->conditional_next;
    }
    else {
        node->caller->next = node->caller->in_order_next;
    }

    return double_init(executor, 0);
}

static void
expression_if_free(
    struct timeline_expression_node *expression,
    struct timeline_execution_context *executor
)
{
    struct expression_if *const node = (void *) expression;
    node->condition->free(node->condition, executor);
    timeline_instruction_unref(executor, node->inner);
    mc_free(expression);
}

static struct timeline_expression_node *
expression_if(
    struct timeline_execution_context *executor,
    struct timeline_expression_node *condition,
    struct timeline_instruction *caller, struct timeline_instruction *inner
)
{
    struct expression_if *const expression =
        mc_malloc(sizeof(struct expression_if));
    expression->caller = caller;
    expression->inner = inner;
    expression->condition = condition;
    expression->base.free = expression_if_free;
    expression->base.execute = expression_if_execute;
    return &expression->base;
}

extern inline struct vector_field
timeline_instruction_full_execute(
    struct timeline_instruction *instruction,
    struct timeline_execution_context *context
);

#define ASSIGNMENT 0
#define DIRECT_ASSIGNMENT ASSIGNMENT
#define PLUS_ASSIGNMENT ASSIGNMENT

#define LOGICAL_OR 1
#define LOGICAL_AND 2

#define EQUALITY 3
#define EQUALS EQUALITY
#define NOT_EQUALS EQUALITY

#define INEQUALITY 4
#define LESS_THAN INEQUALITY
#define LESS_THAN_EQUALS INEQUALITY
#define GREATER_THAN INEQUALITY
#define GREATER_THAN_EQUALS INEQUALITY
#define CONTAINS INEQUALITY

#define RANGE_FUNCTOR 5

#define TRANSLATION 6
#define ADDITION TRANSLATION
#define SUBTRACTION TRANSLATION

#define SCALING 7
#define MULTIPLICATION SCALING
#define DIVISION SCALING

#define POWER 8

#define INFINITE_OPERATOR 10

/* MARK: Pure Reference Parameter */
/* hmm we have a LOT of code that has to do with parsing comma separated value
 * like object */
static struct timeline_expression_node *
expression_reference(struct expression_tokenizer *tokenizer)
{

    if (tokenizer_equals(tokenizer, "{")) {
        tokenizer_read(tokenizer);
        /* vector */
        mc_count_t count = 0;
        struct timeline_expression_node **nodes = NULL;

        if (tokenizer_equals(tokenizer, "}")) {
            tokenizer_read(tokenizer);
            return expression_vector_literal(count, nodes);
        }

        for (;; ++count) {
            /* if it's at capacity */
            MC_MEM_RESERVE(nodes, count);

            struct timeline_expression_node *const curr =
                expression_reference(tokenizer);

            if (!curr) {
                goto error;
            }

            nodes[count] = curr;

            if (tokenizer_equals(tokenizer, ",")) {
                tokenizer_read(tokenizer);
            }
            else if (tokenizer_equals(tokenizer, "}")) {
                tokenizer_read(tokenizer);
                ++count;
                break;
            }
            else {
                curr->free(curr, tokenizer->executor);
                VECTOR_FIELD_ERROR(tokenizer->executor, "Expected comma");
                goto error;
            }
        }

        return expression_vector_literal(count, nodes);
    error:
        for (mc_ind_t i = 0; i < count; ++i) {
            nodes[i]->free(nodes[i], tokenizer->executor);
        }
        mc_free(nodes);
        return NULL;
    }
    else {
        /* assume it's a pure reference */
        struct timeline_symbol_entry const entry =
            timeline_executor_symbol_search(tokenizer->executor, tokenizer, 1);
        tokenizer_read(tokenizer);

        if (!entry.name) {
            return NULL;
        }
        else if (entry.constant) {
            VECTOR_FIELD_ERROR(
                tokenizer->executor,
                "Input variable '%s' to reference argument expected to be "
                "mutable (i.e. use var instead of let)",
                entry.name
            );
            return NULL;
        }
        else if (entry.group_count) {
            VECTOR_FIELD_ERROR(
                tokenizer->executor,
                "Input variable '%s' to reference argument cannot be a "
                "function!",
                entry.name
            );
            return NULL;
        }

        return expression_lvalue_literal(
            (long long) tokenizer->executor->symbol_delta -
                (long long) entry.index,
            entry.constant, entry.reference_var, 0, entry.function_arg
        );
    }
}

/* MARK: Container Parsing */

/* vector or map (to be done) */
static struct timeline_expression_node *
expression_container(struct expression_tokenizer *tokenizer)
{
    mc_count_t count = 0;
    struct timeline_expression_node **nodes = NULL;
    struct timeline_expression_node **values = NULL;

    if (tokenizer_equals(tokenizer, "}")) {
        tokenizer_read(tokenizer);
        return expression_vector_literal(count, nodes);
    }
    else if (tokenizer_equals(tokenizer, ":")) {
        tokenizer_read(tokenizer);
        if (tokenizer_equals(tokenizer, "}")) {
            tokenizer_read(tokenizer);
            return expression_map_literal(count, nodes, values);
        }
        else {
            VECTOR_FIELD_ERROR(tokenizer->executor, "Invalid map literal");
            return NULL;
        }
    }

    for (;; ++count) {
        /* if it's at capacity */
        MC_MEM_RESERVE(nodes, count);

        struct timeline_expression_node *const curr =
            expression_b(tokenizer, -1);

        if (!curr) {
            goto error;
        }

        nodes[count] = curr;

        if (tokenizer_equals(tokenizer, ":")) {
            tokenizer_read(tokenizer);

            if (!values && count) {
                curr->free(curr, tokenizer->executor);
                VECTOR_FIELD_ERROR(
                    tokenizer->executor, "Ambiguous creation of vector or map"
                );
                goto error;
            }

            MC_MEM_RESERVE(values, count);

            struct timeline_expression_node *const val =
                expression_b(tokenizer, -1);

            if (!val) {
                curr->free(curr, tokenizer->executor);
                goto error;
            }

            values[count] = val;
        }
        else if (values) {
            curr->free(curr, tokenizer->executor);
            VECTOR_FIELD_ERROR(
                tokenizer->executor, "Ambiguous creation of vector or map"
            );
            goto error;
        }

        if (tokenizer_equals(tokenizer, ",")) {
            tokenizer_read(tokenizer);
        }
        else if (tokenizer_equals(tokenizer, "}")) {
            tokenizer_read(tokenizer);
            count++;
            break;
        }
        else {
            curr->free(curr, tokenizer->executor);
            if (values) {
                values[count]->free(values[count], tokenizer->executor);
            }
            VECTOR_FIELD_ERROR(tokenizer->executor, "Expected comma");
            goto error;
        }
    }

    if (values) {
        return expression_map_literal(count, nodes, values);
    }
    return expression_vector_literal(count, nodes);

error:
    for (mc_ind_t i = 0; i < count; ++i) {
        nodes[i]->free(nodes[i], tokenizer->executor);
        if (values) {
            values[i]->free(values[i], tokenizer->executor);
        }
    }
    mc_free(values);
    mc_free(nodes);
    return NULL;
}

/* expects tokenizer->end to be starting right after the first quote */
static struct timeline_expression_node *
expression_string(struct expression_tokenizer *tokenizer)
{
    mc_count_t count = 0;
    struct timeline_expression_node **nodes = NULL;

    for (;; ++count) {
        MC_MEM_RESERVE(nodes, count);

        char c;

        if (*tokenizer->end == '"') {
            ++tokenizer->end;
            tokenizer_read(tokenizer);
            break;
        }
        else if (*tokenizer->end == '%') {
            ++tokenizer->end;
            if (*tokenizer->end == 't') {
                c = '\t';
            }
            else if (*tokenizer->end == '%') {
                c = '%';
            }
            else if (*tokenizer->end == '"') {
                c = '"';
            }
            else if (*tokenizer->end == 'n') {
                c = '\n';
            }
            else if (*tokenizer->end == '{') {
                ++tokenizer->end;
                tokenizer_read(tokenizer);
                struct timeline_expression_node *const node =
                    expression_b(tokenizer, -1);
                if (!node) {
                    goto error;
                }
                nodes[count] = node;

                if (*tokenizer->start != '}') {
                    VECTOR_FIELD_ERROR(tokenizer->executor, "expected }");
                    goto error;
                }
                continue;
            }
            else {
                VECTOR_FIELD_ERROR(
                    tokenizer->executor, "Invalid escape code `%c`",
                    *tokenizer->end
                );
                goto error;
            }
        }
        else if (*tokenizer->end) {
            c = *tokenizer->end;
        }
        else {
            VECTOR_FIELD_ERROR(
                tokenizer->executor, "Unterminated string literal"
            );
            goto error;
        }

        struct timeline_expression_node *const node =
            expression_char_literal(c);
        nodes[count] = node;

        ++tokenizer->end;
    }

    return expression_vector_literal(count, nodes);

error:
    for (mc_ind_t i = 0; i < count; ++i) {
        nodes[i]->free(nodes[i], tokenizer->executor);
    }
    mc_free(nodes);
    return NULL;
}

/* MARK: Func Declaration Parsing */

static struct timeline_arg_group
expression_func_group(struct expression_tokenizer *tokenizer);
static struct timeline_symbol_entry
expression_func_func(char const *name, struct expression_tokenizer *tokenizer)
{
    struct timeline_symbol_entry entry = { 0 };
    entry.name = name;
    entry.constant = 1;
    entry.delta = 1;
    if (!tokenizer_equals(tokenizer, "(")) {
        VECTOR_FIELD_ERROR(
            tokenizer->executor, "Invalid function declaration, expected ("
        );
        goto error;
    }
    tokenizer_read(tokenizer);

    mc_count_t total_args = 0;

    if (!tokenizer_equals(tokenizer, ")")) {
        for (;;) {
            struct timeline_arg_group const e =
                expression_func_group(tokenizer);

            if (!e.modes) {
                goto error;
            }
            else {
                MC_MEM_RESERVE(entry.arg_groups, entry.group_count);
                entry.arg_groups[entry.group_count++] = e;
                total_args += e.union_size + (e.index ? 1 : 0);
            }

            if (tokenizer_equals(tokenizer, ")")) {
                break;
            }
            else if (tokenizer_equals(tokenizer, ",")) {
                tokenizer_read(tokenizer);
            }
            else {
                VECTOR_FIELD_ERROR(
                    tokenizer->executor, "Invalid separation character `%c`",
                    *tokenizer->end
                );
                goto error;
            }
        }
        tokenizer_read(tokenizer);
    }

    if (entry.group_count == 0) {
        VECTOR_FIELD_ERROR(
            tokenizer->executor, "Function cannot take zero arguments"
        );
        goto error;
    }
    else if (total_args > MAX_FUNCTION_ARG_COUNT) {
        VECTOR_FIELD_ERROR(
            tokenizer->executor, "Max function argument count of %d exceeded",
            MAX_FUNCTION_ARG_COUNT
        );
        goto error;
    }

    return entry;

error:
    timeline_executor_symbol_free(entry);
    return (struct timeline_symbol_entry){ 0 };
}

/* either a regular, a functoin, or a reference */
static struct timeline_symbol_entry
expression_func_single(struct expression_tokenizer *tokenizer)
{
    char const *const name = tokenizer_dup(tokenizer);
    tokenizer_read(tokenizer);

    if (tokenizer_equals(tokenizer, "(")) {
        struct timeline_symbol_entry entry =
            expression_func_func(name, tokenizer);
        entry.function_arg = 1;
        return entry;
    }
    else if (tokenizer_equals(tokenizer, "&")) {
        tokenizer_read(tokenizer);

        struct timeline_symbol_entry entry = { 0 };
        entry.name = name;
        entry.reference_var = 1;
        entry.function_arg = 1;
        entry.delta = 1;

        return entry;
    }
    else {
        struct timeline_symbol_entry entry = { 0 };
        entry.name = name;
        entry.delta = 1;
        entry.function_arg = 1;
        entry.constant = 1;

        return entry;
    }
}

static struct timeline_arg_mode
expression_func_mode(struct expression_tokenizer *tokenizer)
{
    struct timeline_arg_mode mode = { 0 };

    if (tokenizer_equals(tokenizer, "[")) {
        tokenizer_read(tokenizer);
        mode.label = tokenizer_dup(tokenizer);

        tokenizer_read(tokenizer);

        if (!tokenizer_equals(tokenizer, "]")) {
            VECTOR_FIELD_ERROR(
                tokenizer->executor, "Invalid mode label, expected "
                                     "[mode_label] {[path 1] {a,b,c...}, ...}"
            );
            goto error;
        }
        tokenizer_read(tokenizer);
        if (!tokenizer_equals(tokenizer, "{")) {
            VECTOR_FIELD_ERROR(
                tokenizer->executor, "Invalid mode value, expected "
                                     "[mode_label] {[path 1] {a,b,c...}, ...}"
            );
            goto error;
        }
        tokenizer_read(tokenizer);

        /* keep writing parameters until we hit } */
        if (!tokenizer_equals(tokenizer, "}")) {
            for (;;) {
                struct timeline_symbol_entry const e =
                    expression_func_single(tokenizer);
                if (!e.name) {
                    goto error;
                }

                MC_MEM_RESERVE(mode.real_args, mode.used_count);
                mode.real_args[mode.used_count++] = e;

                if (tokenizer_equals(tokenizer, "}")) {
                    break;
                }
                else if (tokenizer_equals(tokenizer, ",")) {
                    tokenizer_read(tokenizer);
                }
                else {
                    VECTOR_FIELD_ERROR(tokenizer->executor, "Expected , or }");
                    goto error;
                }
            }
        }
        tokenizer_read(tokenizer);
    }
    else {
        VECTOR_FIELD_ERROR(
            tokenizer->executor, "Invalid mode declaration, expected ["
        );
    }

    return mode;

error:
    timeline_executor_symbol_mode_free(mode);
    return (struct timeline_arg_mode){ 0 };
}

/* either a regular/function/reference OR a group of aforemetioned*/
static struct timeline_arg_group
expression_func_group(struct expression_tokenizer *tokenizer)
{
    struct timeline_arg_group entry = { 0 };

    /* see if we have an arg group index */
    if (tokenizer_equals(tokenizer, "[")) {
        if (!tokenizer->entry->group->slide->is_std) {
            VECTOR_FIELD_ERROR(
                tokenizer->executor,
                "Due to security issues, union types are not allowed outside "
                "of libmc setup code. "
                "This behavior may be changed in the future"
            );
            goto error;
        }

        tokenizer_read(tokenizer);
        entry.index = tokenizer_dup(tokenizer);
        tokenizer_read(tokenizer);

        if (!tokenizer_equals(tokenizer, "]")) {
            VECTOR_FIELD_ERROR(
                tokenizer->executor, "Invalid group label, expected ]"
            );
            goto error;
        }
        tokenizer_read(tokenizer);
        if (!tokenizer_equals(tokenizer, "{")) {
            VECTOR_FIELD_ERROR(
                tokenizer->executor, "Invalid group value, expected {"
            );
            goto error;
        }
        tokenizer_read(tokenizer);

        /* keep writing parameters until we hit } */
        for (;;) {
            struct timeline_arg_mode const m = expression_func_mode(tokenizer);
            if (!m.label) {
                goto error;
            }

            MC_MEM_RESERVE(entry.modes, entry.mode_count);
            entry.modes[entry.mode_count++] = m;

            if (tokenizer_equals(tokenizer, "}")) {
                break;
            }
            else if (tokenizer_equals(tokenizer, ",")) {
                tokenizer_read(tokenizer);
            }
            else {
                VECTOR_FIELD_ERROR(tokenizer->executor, "Expected , or }");
                goto error;
            }
        }
        tokenizer_read(tokenizer);

        for (mc_ind_t i = 0; i < entry.mode_count; ++i) {
            if (entry.modes[i].used_count > entry.union_size) {
                entry.union_size = entry.modes[i].used_count;
            }
        }

        for (mc_ind_t j = 0; j < entry.union_size; ++j) {
            mc_count_t reference_count = 0, nonreference_count = 0;
            for (mc_ind_t i = 0; i < entry.mode_count; ++i) {
                if (j >= entry.modes[i].used_count) {
                    continue;
                }
                else if (entry.modes[i].real_args[j].reference_var) {
                    ++reference_count;
                }
                else {
                    ++nonreference_count;
                }
            }

            if (reference_count && nonreference_count) {
                VECTOR_FIELD_ERROR(
                    tokenizer->executor,
                    "In a single slot, all variables must either be reference "
                    "or all variables must be non reference"
                );
                goto error;
            }
        }
    }
    else {
        struct timeline_symbol_entry const main =
            expression_func_single(tokenizer);
        if (!main.name) {
            goto error;
        }

        struct timeline_arg_mode const mode = {
            .label = mc_strdup("main"),
            .used_count = 1,
            .real_args = mc_malloc(sizeof(struct timeline_symbol_entry))
        };
        *mode.real_args = main;

        entry.mode_count = 1;
        entry.union_size = 1;
        entry.modes = mc_malloc(sizeof(struct timeline_arg_mode));
        *entry.modes = mode;
    }

    return entry;

error:
    timeline_executor_symbol_aux_group_free(entry);
    return (struct timeline_arg_group){ 0 };
}

static mc_status_t
push_arg_group(
    struct timeline_execution_context *executor, struct timeline_arg_group group
)
{
    if (group.index) {
        struct timeline_symbol_entry e = { 0 };
        e.name = group.index;
        e.delta = 1;
        e.constant = 1;
        if (timeline_executor_symbol_push(executor, e) != MC_STATUS_SUCCESS) {
            return MC_STATUS_FAIL;
        }
    }

    mc_ind_t last_nonzero_index;
    for (last_nonzero_index = group.mode_count - 1;
         !group.modes[last_nonzero_index].used_count; --last_nonzero_index) {
        if (!last_nonzero_index) {
            break;
        }
    }

    for (mc_ind_t i = 0; i < group.mode_count; ++i) {
        struct timeline_arg_mode const mode = group.modes[i];

        for (mc_ind_t j = 0; j + 1 < mode.used_count; ++j) {
            mode.real_args[j].delta = 1;
            if (timeline_executor_symbol_push(executor, mode.real_args[j]) !=
                0) {
                return 1;
            }
        }

        if (mode.used_count) {
            mode.real_args[mode.used_count - 1].delta =
                i == last_nonzero_index
                    ? (int) (group.union_size - mode.used_count) + 1
                    : -(int) mode.used_count + 1;
            if (timeline_executor_symbol_push(
                    executor, mode.real_args[mode.used_count - 1]
                ) != 0) {
                return 1;
            }
        }
    }

    return MC_STATUS_SUCCESS;
}

static struct timeline_expression_node *
expression_func_righthand(
    struct expression_tokenizer *tokenizer,
    struct timeline_execution_context *executor,
    struct timeline_symbol_entry *aux_entry_ref
)
{

    MC_MEM_RESERVE(executor->func_stack, executor->func_count);
    executor->func_stack[executor->func_count] =
        (mc_ind_t) (aux_entry_ref - executor->symbol_stack);

    MC_MEM_RESERVE(executor->func_cut_stack, executor->func_count);
    executor->func_cut_stack[executor->func_count++] =
        executor->symbol_count - 1;

    struct timeline_symbol_entry const entry = *aux_entry_ref;

    ++executor->symbol_depth;
    mc_count_t const org = executor->symbol_count;

    for (mc_ind_t i = 0; i < entry.group_count; ++i) {
        if (push_arg_group(executor, entry.arg_groups[i]) != 0) {
            while (executor->symbol_count > org) {
                timeline_executor_symbol_pop(executor, 0);
            }

            --executor->func_count;
            --executor->symbol_depth;
            return NULL;
        }
    }

    /* parse equation (which isn't even that hard anymore...) */
    struct timeline_expression_node *node = expression_b(tokenizer, 0);

    while (executor->symbol_count > org) {
        timeline_executor_symbol_pop(executor, 0);
    }

    --executor->func_count;
    --executor->symbol_depth;

    if (node) {
        aux_entry_ref = timeline_executor_symbol_pointer(executor, entry.name);
        struct timeline_instruction *instruction =
            mc_calloc(1, sizeof(struct timeline_instruction));
        instruction->ref_count = 1;
        instruction->slide = executor->execution_slide;
        instruction->line_no = executor->execution_line;
        instruction->root = node;
        node = function_dec_init(
            executor,
            (mc_rind_t) executor->symbol_delta -
                (mc_rind_t) aux_entry_ref->index,
            aux_entry_ref->capture_count, aux_entry_ref->captures, instruction
        );
        aux_entry_ref->captures = NULL; /* move */
        aux_entry_ref->capture_count = 0;
    }

    return node;
}

static struct timeline_expression_node *
expression_func(
    struct expression_tokenizer *tokenizer,
    struct timeline_execution_context *executor
)
{
    char const *const name = tokenizer_dup(tokenizer);
    tokenizer_read(tokenizer);
    struct timeline_symbol_entry entry = expression_func_func(name, tokenizer);
    entry.delta = 1;

    if (!entry.name) {
        return NULL;
    }

    if (timeline_executor_symbol_push(executor, entry) != 0) {
        timeline_executor_symbol_free(entry);
        return NULL;
    }

    if (!tokenizer_equals(tokenizer, "=")) {
        VECTOR_FIELD_ERROR(
            executor, "Expected equals after function declaration"
        );
        return NULL;
    }
    tokenizer_read(tokenizer);

    return expression_func_righthand(
        tokenizer, executor, &executor->symbol_stack[executor->symbol_count - 1]
    );
}

/* MARK: Functor Call */
struct expression_functor {
    struct timeline_expression_node base;
    long long neg_index;
    mc_bool_t force_const;
    mc_count_t mode_count;
    mc_ind_t *indices;
    mc_ind_t *widths;
    mc_count_t *used;
    mc_count_t field_count;
    char const **field_names;
    struct timeline_instruction *head;
};

static struct vector_field
expression_functor_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_functor *const functor = (void *) node;

    struct vector_field const caller =
        executor->stack[(long long) executor->stack_frame - functor->neg_index];

    mc_count_t const org = executor->stack_frame;

    /* push appropriate variables */
    mc_count_t arg_count = 0;
    mc_ind_t non_null_pos = 0;
    for (mc_ind_t i = 0; i < functor->mode_count; ++i) {
        if (functor->indices[i] != SIZE_MAX) {
            if (!timeline_executor_var_push(
                    executor,
                    double_init(executor, (double) functor->indices[i])
                )) {
                return VECTOR_FIELD_NULL;
            }
            ++arg_count;
        }

        for (mc_ind_t j = 0; j < functor->widths[i]; ++j) {
            if (!timeline_executor_var_push(executor, VECTOR_FIELD_NULL)) {
                return VECTOR_FIELD_NULL;
            }
        }

        arg_count += functor->widths[i];
    }
    /* 'hack' to make sure the stuff is not prematurely freed, (in essence if we
       have swap: var p = 0 var q = 0 elem&: {p,q}

        which has no practical purpose, but we don't want a faulty access
     */
    mc_count_t org_frame = executor->stack_frame;
    mc_count_t const org_depth = executor->stack_depth;
    executor->stack_depth = 0;
    struct vector_field const ret =
        timeline_executor_execute(executor, functor->head, 0);
    executor->stack_depth = org_depth;
    if (!ret.vtable) {
        return VECTOR_FIELD_NULL;
    }

    struct vector_named_field *const arg_copy =
        mc_malloc(sizeof(struct vector_named_field) * arg_count);
    arg_count = 0;
    for (mc_ind_t i = 0; i < functor->mode_count; ++i) {
        if (functor->indices[i] != SIZE_MAX) {
            struct vector_field const field = executor->stack[org + arg_count];
            arg_copy[arg_count] = (struct vector_named_field){
                .field = field,
                .name = NULL,
                .last_hash = 0, /* hashes not needed for indices */
                .dirty = 0,
            };
            ++arg_count;
            ++non_null_pos;
        }

        for (mc_ind_t j = 0; j < functor->widths[i]; ++j) {
            if (j < functor->used[i]) {
                struct vector_field const field =
                    executor->stack[org + arg_count];
                char const *const name = functor->field_names[non_null_pos];
                mc_hash_t const sub =
                    name ? VECTOR_FIELD_HASH(executor, field) : 0;
                if (name && !sub) {
                    while (executor->stack_frame > org_frame) {
                        --executor->stack_frame;
                        VECTOR_FIELD_FREE(
                            executor, executor->stack[executor->stack_frame]
                        );
                        VECTOR_FIELD_FREE(
                            executor,
                            executor
                                ->creation_follower_stack[executor->stack_frame]
                        );
                    }
                    mc_free(arg_copy);
                    return VECTOR_FIELD_NULL;
                }
                arg_copy[arg_count] = (struct vector_named_field){
                    .field = field,
                    .name = name,
                    .last_hash = name ? sub : 0,
                    .dirty = 0,
                };
                ++non_null_pos;
            }
            else {
                arg_copy[arg_count] = (struct vector_named_field){
                    .field = VECTOR_FIELD_NULL,
                    .name = NULL,
                    .last_hash = 0,
                    .dirty = 0,
                };
            }
            ++arg_count;
        }
    }

    /* call (which handles cleanup for us) */
    function_call(executor, caller, arg_count, &executor->stack[org]);

    while (executor->stack_frame > org_frame) {
        --executor->stack_frame;
        VECTOR_FIELD_FREE(executor, executor->stack[executor->stack_frame]);
        VECTOR_FIELD_FREE(
            executor, executor->creation_follower_stack[executor->stack_frame]
        );
    }

    if (!executor->return_register.vtable) {
        mc_free(arg_copy); /* the actual fields will be freed by the stack... */
        return VECTOR_FIELD_NULL;
    }

    executor->stack_frame -=
        arg_count; /* makes sure we can succesfully transfer to the functor
                      attribute list without double free */

    /* create functor */
    struct vector_field const res = vector_field_extract_type(
        executor, &executor->return_register, VECTOR_FIELD_PURE
    );
    struct vector_field const ret2 = functor_init(
        executor, arg_count, arg_copy, VECTOR_FIELD_COPY(executor, caller), res,
        functor->force_const
    );
    executor->return_register = VECTOR_FIELD_NULL;
    return ret2;
}

static void
expression_functor_free(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_functor *const functor = (void *) node;

    timeline_instruction_unref(executor, functor->head);

    for (mc_ind_t i = 0; i < functor->field_count; ++i) {
        mc_free((char *) functor->field_names[i]);
    }
    mc_free(functor->field_names);
    mc_free(functor->indices);
    mc_free(functor->used);
    mc_free(functor->widths);
    mc_free(functor);
}

static mc_bool_t
mode_matches(struct aux_group_model_mode mode, struct timeline_arg_mode comp)
{
    if (mode.aux_entry_count != comp.used_count) {
        return 0;
    }
    if (strcmp(mode.key, comp.label)) {
        return 0;
    }

    for (mc_ind_t i = 0; i < mode.aux_entry_count; ++i) {
        /* title should match expected */
        if (!comp.real_args[i].reference_var &&
            !comp.real_args[i].group_count) {
            if (strcmp(mode.entries[i].title, comp.real_args[i].name)) {
                return 0;
            }
        }
        else {
#pragma message("OPTIMIZATION don't create auxillary string?")
            struct str_dynamic dynamic = str_dynamic_init();
            timeline_symbol_aux_entry_string(&dynamic, comp.real_args[i]);
            dynamic.pointer[dynamic.offset] = 0;
            if (strcmp(dynamic.pointer, mode.entries[i].title)) {
                mc_free(dynamic.pointer);
                return 0;
            }
            mc_free(dynamic.pointer);
        }
    }

    return 1;
}

static mc_bool_t
aux_group_matches(struct aux_group_model *group, struct timeline_arg_group comp)
{
    if (group->mode_count != comp.mode_count) {
        return 0;
    }
    if ((!comp.index && strcmp(group->group_class, NULL_INDEX_STRING)) ||
        (!strcmp(group->group_class, NULL_INDEX_STRING) && comp.index) ||
        (strcmp(group->group_class, NULL_INDEX_STRING) && comp.index &&
         strcmp(group->group_class, comp.index))) {
        return 0;
    }

    for (mc_ind_t i = 0; i < group->mode_count; ++i) {
        if (!mode_matches(group->modes[i], comp.modes[i])) {
            return 0;
        }
    }

    return 1;
}

static struct aux_group_model *
aux_group_from(
    struct timeline_arg_group source, struct aux_group_model *replacing
)
{
    struct aux_group_model *const ret =
        replacing ? replacing : mc_calloc(1, sizeof(struct aux_group_model));
    struct aux_group_model_mode *const modes =
        mc_malloc(source.mode_count * sizeof(struct aux_group_model_mode));
    struct aux_group_model_mode *const mode_src =
        replacing ? aux_group_mode(replacing) : NULL;

    char const *best_key = NULL;
    int most_matches = -1;

    for (mc_ind_t i = 0; i < source.mode_count; ++i) {
        modes[i].key =
            source.modes[i].label ? mc_strdup(source.modes[i].label) : NULL;
        modes[i].aux_entry_count = source.modes[i].used_count;
        modes[i].entries =
            mc_calloc(modes[i].aux_entry_count, sizeof(struct aux_entry_model));

        int matches = 0;

        for (mc_ind_t j = 0; j < modes[i].aux_entry_count; ++j) {
            struct str_dynamic dynamic = str_dynamic_init();
            timeline_symbol_aux_entry_string(
                &dynamic, source.modes[i].real_args[j]
            );
            dynamic.pointer[dynamic.offset] = 0;
            modes[i].entries[j].title = dynamic.pointer;

            if (mode_src && j < mode_src->aux_entry_count) {
                if (mode_src->entries[j].title &&
                    !strcmp(
                        modes[i].entries[j].title, mode_src->entries[j].title
                    )) {
                    ++matches;
                }
                modes[i].entries[j].data = mc_strdup(mode_src->entries[j].data);
            }
            else {
                modes[i].entries[j].data = mc_calloc(1, sizeof(char));
            }

            modes[i].entries[j].group = ret;
        }

        /* enum entry */
        if (!modes[i].aux_entry_count && mode_src &&
            mode_src->aux_entry_count && source.index &&
            !strcmp(mode_src->entries[0].title, source.index) && modes[i].key &&
            !strcmp(mode_src->entries[0].data, modes[i].key)) {
            best_key = modes[i].key;
            most_matches = 0;
        }
        else if (mode_src && modes[i].aux_entry_count <= mode_src->aux_entry_count && matches > most_matches) {
            most_matches = matches;
            best_key = modes[i].key;
        }
    }

    if (replacing) {
        aux_group_partial_swap_mode(ret, source.mode_count, modes);

        if (best_key) {
            aux_group_partial_switch_mode(ret, best_key);
        }
        else {
            aux_group_partial_switch_mode(ret, ret->modes[0].key);
        }

        aux_group_swap_class(
            ret, source.index ? mc_strdup(source.index)
                              : mc_strdup(NULL_INDEX_STRING)
        );
    }
    else {
        ret->group_class = source.index ? mc_strdup(source.index)
                                        : mc_strdup(NULL_INDEX_STRING);
        ret->mode_count = source.mode_count;
        ret->modes = modes;
        ret->mode_key = ret->modes[0].key;
    }

    return ret;
}

static void
merge_into(struct aux_group_model *src, struct aux_group_model *dst)
{
    if (mc_po2_ceil(dst->modes[0].aux_entry_count) ==
        dst->modes[0].aux_entry_count) {
        dst->modes[0].entries = mc_reallocf(
            dst->modes[0].entries,
            mc_po2_ceil(dst->modes[0].aux_entry_count + 1) *
                sizeof(struct aux_entry_model)
        );
    }

    dst->modes[0].entries[dst->modes[0].aux_entry_count++] =
        src->modes[0].entries[0];

    for (mc_ind_t i = 0; i < src->child_count; ++i) {
        aux_group_insert_custom_child(dst, src->children[i], dst->child_count);
    }

    /* free, without double freeing */
    src->child_count = 0;
    src->modes[0].aux_entry_count = 0;
    aux_group_delete(src);
}

static void
match_group_children_to(
    struct timeline_execution_context *executor, struct aux_group_model *group,
    struct timeline_symbol_entry entry
)
{
    mc_ind_t current_group = 0;
    struct aux_group_model *root = NULL;

    // scroll past all the groups, and see if we find one that matches the
    // current arg if so, take it (and the value), and keep taking while we can
    for (mc_ind_t i = 0; i < group->child_count; ++i) {
        if (group->children[i]->group_class) {
            char const *title = group->children[i]->modes[0].entries->title;
            struct expression_tokenizer sub_tokenizer = {
                .start = title,
                .end = title,
                .remove_children = 0,
                .block_functors = 0,
                .entry = group->children[i]->modes[0].entries,
                .executor = executor,
            };
            tokenizer_read(&sub_tokenizer);

            /* no match, delete */
            for (int q = 0; current_group < entry.group_count && q < 2; ++q) {
                struct timeline_arg_group comp =
                    entry.arg_groups[current_group];

                for (mc_ind_t j = 0; j < comp.mode_count; ++j) {
                    if (!comp.modes[j].used_count) {
                        if (tokenizer_equals(&sub_tokenizer, comp.index)) {
                            goto after_adding;
                        }
                    }

                    for (mc_ind_t k = 0; k < comp.modes[j].used_count; ++k) {
                        if (tokenizer_equals(
                                &sub_tokenizer, comp.modes[j].real_args[k].name
                            )) {
                            goto after_adding;
                        }
                    }
                }

                if (root) {
                    current_group++;
                    root = NULL;
                }
            }

            // by definition, must not have been added
            aux_group_delete(group->children[i]);
            --i;
            continue;

        after_adding:
            if (!root) {
                root = group->children[i];
            }
            else {
                merge_into(group->children[i], root);
                --i;
            }
        }
    }

    if (root) {
        current_group++;
    }

    mc_ind_t insertion = group->child_count;
    while (insertion > 0 && !group->children[insertion - 1]->group_class &&
           group->children[insertion - 1]->modes[0].entries[0].is_empty) {
        --insertion;
    }

    while (current_group < entry.group_count) {
        struct aux_group_model *insert =
            aux_group_from(entry.arg_groups[current_group], NULL);
        insert->tabs = group->tabs + 1;

        aux_group_insert_custom_child(group, insert, insertion++);

        ++current_group;
    }

    /* normalize all groups */
    current_group = 0;
    for (mc_ind_t i = 0; i < group->child_count; ++i) {
        if (group->children[i]->group_class) {
            if (!aux_group_matches(
                    group->children[i], entry.arg_groups[current_group]
                )) {
                aux_group_from(
                    entry.arg_groups[current_group], group->children[i]
                );
            }
            current_group++;
        }
    }
}

static struct timeline_expression_node *
expression_functor(
    struct expression_tokenizer *tokenizer, long long neg_index,
    struct timeline_symbol_entry entry,
    struct timeline_execution_context *executor
)
{
    struct expression_functor *node =
        mc_calloc(1, sizeof(struct expression_functor));
    node->base.execute = expression_functor_execute;
    node->base.free = expression_functor_free;

    /* create node */
    node->neg_index = neg_index;
    node->mode_count = entry.group_count;
    node->indices = mc_malloc(sizeof(mc_ind_t) * entry.group_count);
    node->widths = mc_malloc(sizeof(mc_count_t) * entry.group_count);
    node->used = mc_malloc(sizeof(mc_count_t) * entry.group_count);
    /* maintain invariants (remove bad ones... and replace with good) */
    struct aux_group_model *const group = tokenizer->entry->group;

    ++executor->symbol_depth;

    mc_ind_t aux_group_pointer = 0;
    mc_count_t symbols = 0;
    match_group_children_to(executor, group, entry);
    for (mc_ind_t i = 0; i < group->child_count; ++i) {
        struct aux_group_model *child = group->children[i];
        if (child->group_class) {
            mc_ind_t const mode_index = aux_group_mode_index(child);
            node->widths[aux_group_pointer] =
                entry.arg_groups[aux_group_pointer].union_size;
            node->used[aux_group_pointer] = entry.arg_groups[aux_group_pointer]
                                                .modes[mode_index]
                                                .used_count;
            node->indices[aux_group_pointer] = mode_index;

            // can index be elided?
            if (mode_index || strcmp(child->group_class, NULL_INDEX_STRING)) {
                struct timeline_symbol_entry symbol = { 0 };
                symbol.delta = 1;
                if (child->modes[mode_index].aux_entry_count == 0) {
                    symbol.delta +=
                        entry.arg_groups[aux_group_pointer].union_size;
                }
                timeline_executor_symbol_push(executor, symbol);
                ++symbols;
            }
            else {
                node->indices[aux_group_pointer] = SIZE_MAX;
            }

            for (mc_ind_t j = 0; j < child->modes[mode_index].aux_entry_count;
                 ++j) {
                struct aux_entry_model *en =
                    child->modes[mode_index].entries + j;
                struct expression_tokenizer sub_tokenizer = {
                    .start = en->title,
                    .end = en->title,
                    .remove_children = 0,
                    .block_functors = 0,
                    .entry = en,
                    .executor = executor,
                };
                tokenizer_read(&sub_tokenizer);

                struct timeline_symbol_entry symbol =
                    expression_func_single(&sub_tokenizer);
                symbol.functor_arg = 1;
                if (j == child->modes[mode_index].aux_entry_count - 1) {
                    symbol.delta =
                        (int) (entry.arg_groups[aux_group_pointer].union_size -
                               child->modes[mode_index].aux_entry_count + 1);
                }
                timeline_executor_symbol_push(executor, symbol);
                ++symbols;
            }

            ++aux_group_pointer;
        }
    }

    node->field_count = symbols;
    node->field_names = mc_malloc(sizeof(char const *) * symbols);
    for (mc_ind_t i = 0; i < symbols; ++i) {
        struct timeline_symbol_entry const e =
            executor->symbol_stack[executor->symbol_count - 1 - i];
        node->field_names[symbols - 1 - i] =
            e.reference_var || e.group_count || !e.name ? NULL
                                                        : mc_strdup(e.name);
        if (e.reference_var) {
            node->force_const = 1;
        }
    }

    /* really parse frame, and put it on the same level as the declared
     * variables */
    /* then, the inner parse frame is responsible for popping variables */
    --executor->symbol_depth;
    if (!(node->head = timeline_executor_parse_frame(
              executor, group->child_count, group->children,
              tokenizer->mod_allowed
          ))) {
        expression_functor_free(&node->base, executor);
        return NULL;
    }

    return &node->base;
}

/* MARK: Trees */
struct expression_tree {
    struct timeline_expression_node base;
    long long neg_symbol_index;
};

static struct vector_field
expression_tree_execute(
    struct timeline_expression_node *node,
    struct timeline_execution_context *executor
)
{
    struct expression_tree *tree = (void *) node;

    mc_ind_t const index =
        executor->stack_frame - (mc_ind_t) tree->neg_symbol_index;
    if (timeline_executor_ref_capture(executor, index) != MC_STATUS_SUCCESS) {
        return VECTOR_FIELD_NULL;
    }

    return lvalue_init(executor, &executor->stack[index]);
}

static struct timeline_expression_node *
expression_tree_init(
    long long neg_index, struct timeline_execution_context *executor
)
{
    struct expression_tree *tree = mc_malloc(sizeof(struct expression_tree));
    tree->base.execute = &expression_tree_execute;
    tree->base.free = &expression_node_free;
    tree->neg_symbol_index = neg_index;
    return &tree->base;
}

/* MARK: Unary Parsing */
static struct timeline_expression_node *
expression_u(struct expression_tokenizer *tokenizer)
{
    struct timeline_expression_node *left = NULL;

    // get prefix unary list (applied using head recursion
    if (tokenizer_equals(tokenizer, "-")) {
        /* unary negative */
        tokenizer_read(tokenizer);
        struct timeline_expression_node *const x = expression_u(tokenizer);
        if (!x) {
            return NULL;
        }
        return expression_negate(x);
    }
    else if (tokenizer_equals(tokenizer, "!")) {
        /* logical not */
        tokenizer_read(tokenizer);
        struct timeline_expression_node *const x = expression_u(tokenizer);
        if (!x) {
            return NULL;
        }
        return expression_not(x);
    }

    if (*tokenizer->start == '\'') {
        /* char, always just a single character */
        tokenizer_read(tokenizer);
        if (tokenizer->end != tokenizer->start + 1) {
            VECTOR_FIELD_ERROR(
                tokenizer->executor,
                "Expected char literal to be a single character, received %d",
                tokenizer->end - tokenizer->start
            );
            return NULL;
        }

        char const c = *tokenizer->start;

        tokenizer_read(tokenizer);
        if (*tokenizer->start != '\'') {
            VECTOR_FIELD_ERROR(tokenizer->executor, "Invalid char literal");
            return NULL;
        }
        tokenizer_read(tokenizer);
        left = expression_char_literal(c);
    }
    else if (*tokenizer->start == '\"') {
        if (!(left = expression_string(tokenizer))) {
            return NULL;
        }
    }
    else if (*tokenizer->start == '{') {
        tokenizer_read(tokenizer);
        /* map or vector */
        if (!(left = expression_container(tokenizer))) {
            return NULL;
        }
    }
    else if (*tokenizer->start >= '0' && *tokenizer->start <= '9') {
        char const *const start = tokenizer->start;
        char const *t_end = tokenizer->end;
        tokenizer_read(tokenizer);
        if (tokenizer_equals(tokenizer, ".")) {
            /* try another read */
            tokenizer_read(tokenizer);
            if (*tokenizer->start < '0' || *tokenizer->start > '9') {
                VECTOR_FIELD_ERROR(
                    tokenizer->executor, "Invalid double literal"
                );
                return NULL;
            }
            t_end = tokenizer->end;

            tokenizer_read(tokenizer);
        }
        char *end;
        errno = 0;
        double const ret = strtod(start, &end);
        if (((ret == 0 || ret == INFINITY) && errno == ERANGE) ||
            end != t_end) {
            VECTOR_FIELD_ERROR(tokenizer->executor, "Invalid double literal");
            errno = 0;
            return NULL;
        }
        left = expression_double_literal(ret);
    }
    else if (*tokenizer->start == '(') {
        /* parenthesis group */
        tokenizer_read(tokenizer);

        if (!(left = expression_b(tokenizer, -1))) {
            return NULL;
        }

        if (!tokenizer_equals(tokenizer, ")")) {
            left->free(left, tokenizer->executor);
            VECTOR_FIELD_ERROR(
                tokenizer->executor, "Expected right parenthesis"
            );
            return NULL;
        }
        /* skip parenthesis*/
        tokenizer_read(tokenizer);
    }
    else {
        if (tokenizer_equals(tokenizer, "native")) {
            tokenizer_read(tokenizer);
            left = expression_native(tokenizer);
        }
        else if (tokenizer_equals(tokenizer, "sticky")) {
            tokenizer_read(tokenizer);
            struct timeline_expression_node *const x = expression_u(tokenizer);
            if (!x) {
                return NULL;
            }
            return expression_sticky(x);
        }
        else {
            /* lvalue (possibly being a function in which) */
            struct timeline_symbol_entry const entry =
                timeline_executor_symbol_search(
                    tokenizer->executor, tokenizer, 1
                );
            if (!entry.name) {
                return NULL;
            }
            tokenizer_read(tokenizer);

            if (entry.arg_groups) {
                if (tokenizer_equals(tokenizer, "(")) {
#pragma message("TODO, not greatest solution")
                    if ((!strcmp(entry.name, "random") ||
                         !strcmp(entry.name, "randint")) &&
                        tokenizer->executor->func_count) {
                        VECTOR_FIELD_ERROR(
                            tokenizer->executor,
                            "To ensure pureness, random functions must only "
                            "be called at the root level."
                        );
                        return NULL;
                    }

                    return expression_function(
                        tokenizer,
                        (long long) tokenizer->executor->symbol_delta -
                            (long long) entry.index,
                        entry.group_count, entry.arg_groups, function_call
                    );
                }
                else if (tokenizer_equals(tokenizer, ":")) {
                    /* make sure next is */
                    tokenizer_read(tokenizer);
                    if (*tokenizer->start) {
                        VECTOR_FIELD_ERROR(
                            tokenizer->executor,
                            "Functors must appear at the tail of a statement"
                        );
                        return NULL;
                    }

                    struct aux_group_model *const group =
                        tokenizer->entry->group;
                    struct aux_group_model_mode *const mode =
                        aux_group_mode(group);
                    if (mode->entries + mode->aux_entry_count - 1 !=
                        tokenizer->entry) {
                        VECTOR_FIELD_ERROR(
                            tokenizer->executor,
                            "Functors must appear at the tail of a group (in "
                            "this case `%s`)",
                            (mode->entries + mode->aux_entry_count - 1)->title
                        );
                        return NULL;
                    }
                    else if (tokenizer->block_functors) {
                        VECTOR_FIELD_ERROR(
                            tokenizer->executor, "Illegal functor placement"
                        );
                        return NULL;
                    }

                    tokenizer->remove_children = 0;

                    return expression_functor(
                        tokenizer,
                        (long long) tokenizer->executor->symbol_delta -
                            (long long) entry.index,
                        entry, tokenizer->executor
                    );
                }
                else if (tokenizer_equals(tokenizer, "!")) {
                    tokenizer_read(tokenizer);
                    return expression_lvalue_literal(
                        (long long) tokenizer->executor->symbol_delta -
                            (long long) entry.index,
                        1, 0, 0, 0
                    );
                }
                else {
                    VECTOR_FIELD_ERROR(
                        tokenizer->executor, "Functions cannot appear naked"
                    );
                    return NULL;
                }
            }

            left = expression_lvalue_literal(
                (long long) tokenizer->executor->symbol_delta -
                    (long long) entry.index,
                entry.constant, entry.reference_var, 0, entry.function_arg
            );
        }
    }

    if (left) {
        // get postfix unary list (function call is handled along with lvalue
        // read, attribute, index)
        for (;;) {
            if (tokenizer_equals(tokenizer, ".")) {
                /* attribute */
                tokenizer_read(tokenizer);

                mc_count_t const len =
                    (mc_count_t) (tokenizer->end - tokenizer->start);
                char *const copy = mc_malloc(len + 1);
                for (mc_ind_t i = 0; i < len; i++) {
                    copy[i] = tokenizer->start[i];
                }
                copy[len] = 0;

                left = expression_attribute(left, copy);

                tokenizer_read(tokenizer);
            }
            else if (tokenizer_equals(tokenizer, "[")) {
                /* index */
                tokenizer_read(tokenizer);
                struct timeline_expression_node *const index =
                    expression_b(tokenizer, -1);
                if (!index) {
                    left->free(left, tokenizer->executor);
                    return NULL;
                }
                if (!tokenizer_equals(tokenizer, "]")) {
                    left->free(left, tokenizer->executor);
                    index->free(index, tokenizer->executor);
                    VECTOR_FIELD_ERROR(
                        tokenizer->executor,
                        "Expected right square bracket in indexing operation"
                    );
                    return NULL;
                }

                left = expression_index_init(left, index);
                tokenizer_read(tokenizer);
            }
            else {
                break;
            }
        }
    }

    return left;
}

/* MARK: Binary Parsing */
static struct timeline_expression_node *
expression_b(struct expression_tokenizer *tokenizer, int min)
{
    struct timeline_expression_node *left = expression_u(tokenizer);
    if (!left) {
        return NULL;
    }

    while (*tokenizer->start) {
        struct timeline_expression_node *right;
        /* maybe use macros here? it is slightly hard though because there are
         * slight tweaks */
        if (tokenizer_equals(tokenizer, "=")) {
            /* right associative */
            if (min > ASSIGNMENT) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, ASSIGNMENT))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_assign_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "+=")) {
            /* right associative */
            if (min > PLUS_ASSIGNMENT) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, PLUS_ASSIGNMENT))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_plus_assign_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "||")) {
            if (min >= LOGICAL_OR) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, LOGICAL_OR))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_or_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "&&")) {
            if (min >= LOGICAL_AND) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, LOGICAL_AND))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_and_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "==")) {
            if (min >= EQUALS) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, EQUALS))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_eq_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "!=")) {
            if (min >= NOT_EQUALS) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, NOT_EQUALS))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_ne_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "<")) {
            if (min >= LESS_THAN) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, LESS_THAN))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_lt_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, ">")) {
            if (min >= GREATER_THAN) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, GREATER_THAN))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_gt_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "<=")) {
            if (min >= LESS_THAN_EQUALS) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, LESS_THAN_EQUALS))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_lte_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, ">=")) {
            if (min >= GREATER_THAN_EQUALS) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, GREATER_THAN_EQUALS))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_gte_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "in")) {
            if (min >= CONTAINS) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, CONTAINS))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            /* flipped on purpose since it's contains */
            left = expression_contains_init(right, left);
        }
        else if (tokenizer_equals(tokenizer, "+")) {
            if (min >= ADDITION) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, ADDITION))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_plus_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "-")) {
            if (min >= SUBTRACTION) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, SUBTRACTION))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_sub_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "*")) {
            if (min >= MULTIPLICATION) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, MULTIPLICATION))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_mul_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "/")) {
            if (min >= DIVISION) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, DIVISION))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_div_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, "**")) {
            if (min >= POWER) {
                return left;
            }
            tokenizer_read(tokenizer);
            if (!(right = expression_b(tokenizer, POWER))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_pow_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, ":<")) {
            if (min >= RANGE_FUNCTOR) {
                return left;
            }
            tokenizer_read(tokenizer);

            if (!(right = expression_b(tokenizer, RANGE_FUNCTOR))) {
                left->free(left, tokenizer->executor);
                return NULL;
            }
            left = expression_range_init(left, right);
        }
        else if (tokenizer_equals(tokenizer, ")") || tokenizer_equals(tokenizer, "}") || tokenizer_equals(tokenizer, "]") || tokenizer_equals(tokenizer, ":") || tokenizer_equals(tokenizer, ",")) {
            break;
        }
        else {
            left->free(left, tokenizer->executor);
            VECTOR_FIELD_ERROR(tokenizer->executor, "Expected valid operator");
            return NULL;
        }
    }

    return left;
}

/* MARK: Root Parsing */
static void
remove_class_groups(struct aux_entry_model const *model)
{
    struct aux_group_model *const group = model->group;

    for (mc_ind_t i = 0; i < group->child_count; ++i) {
        if (group->children[i]->group_class) {
            aux_group_delete(group->children[i]);
            --i;
        }
    }

    if (!group->child_count) {
        aux_group_insert_custom_child(group, aux_group_custom_blank(NULL), 0);
    }
    else if (group->children[0]->tabs != group->tabs + 1) {
        group->children[0]->tabs = group->tabs + 1;
        group->children[0]->modes[0].entries[0].is_empty = 0;
    }
}

/* im fairly certain we can ignore the modify entirely, since we assume each
 * state is consistent, and we only remove when theres no errors, undos and
 * redos *should *be respected. To be tested to see if this breaks down*/
static struct timeline_expression_node *
expression_parse(
    struct timeline_execution_context *executor,
    struct timeline_instruction *prev, struct aux_entry_model *model,
    struct timeline_instruction *instruction, mc_bool_t modify
)
{
    struct expression_tokenizer tokenizer = {
        .start = model->data,
        .end = model->data,
        .mod_allowed = modify,
        .remove_children = 1,
        .block_functors = 0,
        .entry = model,
        .executor = executor,
    };
    tokenizer_read(&tokenizer);

    /*
      special starts:
        - var
        - let
        - tree
        - func
        - for
        - while
        - if
        - else
     */
    struct timeline_expression_node *root;
    if (model->title) {
        char const *sep = str_first_non_var_name(model->title);
        if (!*tokenizer.start) {
            VECTOR_FIELD_ERROR(executor, "Expected expression");
            root = NULL;
        }
        else if (!*sep) {
            long long const neg_index =
                timeline_executor_symbol_negindex(executor, model->title);

            if ((root = expression_b(&tokenizer, 0))) {
                root = expression_assign_init(
                    expression_lvalue_literal(neg_index, 0, 0, 0, 0), root
                );
            }
        }
        else {
            char *const str = mc_malloc((mc_count_t) (sep - model->title) + 1);
            strncpy(str, model->title, (mc_count_t) (sep - model->title));
            str[sep - model->title] = 0;
            struct timeline_symbol_entry *const ref =
                timeline_executor_symbol_pointer(executor, str);
            long long const neg_index =
                (long long) executor->symbol_delta - (long long) ref->index;
            mc_free(str);

            if (*sep == '&') {
                /* reference */
                /* allowing any lvalue? now... */
                if ((root = expression_reference(&tokenizer))) {
                    root = expression_assign_init(
                        expression_lvalue_literal(neg_index, 0, 0, 1, 0), root
                    );
                }
                //                if ((root = expression_b(&tokenizer, 0))) root
                //                =
                //                expression_assign_init(expression_lvalue_literal(neg_index,
                //                0, 0, 1, 0), root);
            }
            else if (*sep == '(') {
                /* function */
                root = expression_func_righthand(&tokenizer, executor, ref);
            }
            else {
                VECTOR_FIELD_ERROR(executor, "Invalid functor argument syntax");
                root = NULL;
            }
        }
    }
    else if (tokenizer_equals(&tokenizer, "tree")) {
        tokenizer_read(&tokenizer);
        struct timeline_symbol_entry entry = { 0 };
        entry.tree = 1;
        entry.name = tokenizer_dup(&tokenizer);
        entry.delta = 1;
        tokenizer_read(&tokenizer);
        if (timeline_executor_symbol_push(executor, entry) != 0) {
            timeline_executor_symbol_free(entry);
            root = NULL;
        }
        else {
            if (tokenizer_equals(&tokenizer, "=")) {
                struct timeline_expression_node *left, *right;
                left = expression_tree_init(1, executor);

                tokenizer_read(&tokenizer);
                if (!(right = expression_b(&tokenizer, 0))) {
                    left->free(left, tokenizer.executor);
                    root = NULL;
                }
                else {
                    root = expression_assign_init(left, right);
                    ++instruction->var_count;
                }
            }
            else {
                VECTOR_FIELD_ERROR(
                    executor,
                    "Expected initialization with declaration for tree variable"
                );
                root = NULL;
            }
        }
    }
    else if (tokenizer_equals(&tokenizer, "let")) {
        tokenizer_read(&tokenizer);
        struct timeline_symbol_entry entry = { 0 };
        entry.constant = 1;
        entry.name = tokenizer_dup(&tokenizer);
        entry.delta = 1;

        if (timeline_executor_symbol_push(executor, entry) != 0) {
            timeline_executor_symbol_free(entry);
            root = NULL;
        }
        else {
            tokenizer_read(&tokenizer);
            if (tokenizer_equals(&tokenizer, "=")) {
                struct timeline_expression_node *left, *right;
                left = expression_lvalue_literal(1, 0, 0, 0, 0);

                tokenizer_read(&tokenizer);
                if (!(right = expression_b(&tokenizer, 0))) {
                    left->free(left, tokenizer.executor);
                    root = NULL;
                }
                else {
                    root = expression_assign_init(left, right);
                    ++instruction->var_count;
                }
            }
            else {
                VECTOR_FIELD_ERROR(
                    executor,
                    "Expected initialization with declaration for let variable"
                );
                root = NULL;
            }
        }
    }
    else if (tokenizer_equals(&tokenizer, "var")) {
        tokenizer_read(&tokenizer);
        struct timeline_symbol_entry entry = { 0 };
        entry.name = tokenizer_dup(&tokenizer);
        entry.delta = 1;

        if (timeline_executor_symbol_push(executor, entry) != 0) {
            timeline_executor_symbol_free(entry);
            root = NULL;
        }
        else {
            tokenizer_read(&tokenizer);
            if (tokenizer_equals(&tokenizer, "=")) {
                struct timeline_expression_node *left, *right;
                left = expression_lvalue_literal(1, 0, 0, 0, 0);

                tokenizer_read(&tokenizer);
                if (!(right = expression_b(&tokenizer, 0))) {
                    left->free(left, tokenizer.executor);
                    root = NULL;
                }
                else {
                    root = expression_assign_init(left, right);
                    ++instruction->var_count;
                }
            }
            else {
                VECTOR_FIELD_ERROR(
                    executor,
                    "Expected initialization with declaration for let variable"
                );
                root = NULL;
            }
        }
    }
    else if (tokenizer_equals(&tokenizer, "func")) {
        tokenizer_read(&tokenizer);

        if ((root = expression_func(&tokenizer, executor))) {
            ++instruction->var_count;
        }
    }
    else if (tokenizer_equals(&tokenizer, "for")) {
        /* get var name */
        tokenizer_read(&tokenizer);
        tokenizer.block_functors = 1;

        struct timeline_symbol_entry entry = { 0 };
        entry.name = tokenizer_dup(&tokenizer);
        entry.constant = 1;
        entry.delta = 1;

        if (timeline_executor_symbol_push(executor, entry) != 0) {
            timeline_executor_symbol_free(entry);
            root = NULL;
        }
        else {
            tokenizer_read(&tokenizer);
            if (!tokenizer_equals(&tokenizer, "in")) {
                root = NULL;
                VECTOR_FIELD_ERROR(
                    executor, "Expected for <var> in <collection>"
                );
            }
            else {
                tokenizer_read(&tokenizer);
                root = expression_b(&tokenizer, 0);
                if (root) {
                    /* ensure that children is proper */
                    if (modify) {
                        remove_class_groups(model);
                    }
                    struct timeline_instruction *const head =
                        timeline_executor_parse_frame(
                            executor, model->group->child_count,
                            model->group->children, modify
                        );
                    if (!head) {
                        root->free(root, executor);
                        root = NULL;
                    }
                    else {
                        root = expression_iterate(executor, root, head);
                    }
                }
            }
            timeline_executor_symbol_pop(
                executor, 1
            ); /* make it invisible for future statements */
        }

        tokenizer.remove_children = 0;
    }
    else if (tokenizer_equals(&tokenizer, "while")) {
        tokenizer_read(&tokenizer);
        tokenizer.block_functors = 1;

        root = expression_b(&tokenizer, -1);

        if (root) {
            remove_class_groups(model);
            struct timeline_instruction *const head =
                timeline_executor_parse_frame(
                    executor, model->group->child_count, model->group->children,
                    modify
                );
            if (!head) {
                root->free(root, executor);
                root = NULL;
            }
            else {
                root = expression_while(executor, root, head);
            }
        }
        tokenizer.remove_children = 0;
    }
    else if (tokenizer_equals(&tokenizer, "if")) {
        tokenizer_read(&tokenizer);
        tokenizer.block_functors = 1;

        root = expression_b(&tokenizer, -1);
        if (root) {
            tokenizer.block_functors = 1;
            remove_class_groups(model);
            struct timeline_instruction *const head =
                timeline_executor_parse_frame(
                    executor, model->group->child_count, model->group->children,
                    modify
                );
            if (!head) {
                root->free(root, executor);
                root = NULL;
            }
            else {
                root = expression_if(executor, root, instruction, head);
            }
            instruction->conditional = INSTRUCTION_CONDITIONAL_IF;
        }

        tokenizer.remove_children = 0;
    }
    else if (tokenizer_equals(&tokenizer, "else")) {
        tokenizer_read(&tokenizer);
        tokenizer.block_functors = 1;

        if (!prev || (prev->conditional != INSTRUCTION_CONDITIONAL_ELSE_IF &&
                      prev->conditional != INSTRUCTION_CONDITIONAL_IF)) {
            root = NULL;
            VECTOR_FIELD_ERROR(
                executor, "Invalid placement of else; can only appear directly "
                          "after else if or if statement"
            );
        }
        else if (tokenizer_equals(&tokenizer, "if")) {
            tokenizer_read(&tokenizer);
            tokenizer.block_functors = 1;

            root = expression_b(&tokenizer, -1);
            instruction->conditional = INSTRUCTION_CONDITIONAL_ELSE_IF;
        }
        else {
            tokenizer.block_functors = 1;

            if (*tokenizer.start) {
                VECTOR_FIELD_ERROR(executor, "Unexpected token after else");
                root = NULL;
            }
            else {
                root = expression_double_literal(1);
            }
            instruction->conditional = INSTRUCTION_CONDITIONAL_ELSE;
        }

        if (root) {
            remove_class_groups(model);
            struct timeline_instruction *const head =
                timeline_executor_parse_frame(
                    executor, model->group->child_count, model->group->children,
                    modify
                );
            if (!head) {
                root->free(root, executor);
                root = NULL;
            }
            else {
                root = expression_if(executor, root, instruction, head);
            }
        }

        tokenizer.remove_children = 0;
    }
    else if (!*tokenizer.start) {
        root = expression_null();
    }
    else {
        root = expression_b(&tokenizer, 0);
    }

    /* if it didn't make it to the end, that indicates that we had too many
     * right parenthesis or something of the sorts */
    if (root && *tokenizer.end) {
        root->free(root, executor);
        VECTOR_FIELD_ERROR(
            executor, "Did not reach end of expression, likely due to "
                      "unbalanced parenthesis or similar breaking character"
        );
        return NULL;
    }

    struct aux_group_model *const group = model->group;
    struct aux_group_model_mode *const mode = aux_group_mode(group);
    if (mode->entries + (mode->aux_entry_count - 1) == model &&
        tokenizer.remove_children && root) {
        for (mc_ind_t i = 0; i < group->child_count; ++i) {
            executor->execution_line++;
            if (!group->children[i]->modes[0].entries[0].is_empty) {
                VECTOR_FIELD_ERROR(executor, "Illegal indentation");
                root->free(root, executor);
                return NULL;
            }
        }
    }

    return root;
}

struct timeline_instruction *
timeline_instruction_parse(
    struct timeline_execution_context *executor,
    struct timeline_instruction *prev, struct aux_entry_model *node,
    mc_bool_t modify
)
{
    /* Look at executor for past variables...*/
    struct timeline_instruction *ret =
        mc_calloc(1, sizeof(struct timeline_instruction));
    ret->ref_count = 1;
    ret->slide = executor->execution_slide;
    ret->line_no = executor->execution_line;

    struct timeline_expression_node *const pair =
        expression_parse(executor, prev, node, ret, modify);
    ret->root = pair;

    if (!pair) {
        mc_free(ret);
        return NULL;
    }

    return ret;
}

struct timeline_instruction *
timeline_instruction_identity(struct timeline_execution_context *executor)
{
    struct timeline_instruction *ret =
        mc_calloc(1, sizeof(struct timeline_instruction));
    ret->ref_count = 1;
    ret->root = expression_null();

    /* this might not be ideal, but identity operations can never throw so... */
    ret->slide = executor->execution_slide;
    ret->line_no = 0;

    return ret;
}

static void
timeline_instruction_free(
    struct timeline_execution_context *executor,
    struct timeline_instruction *instruction
)
{
    instruction->root->free(instruction->root, executor);
    mc_free(instruction);
}

void
timeline_instruction_unref(
    struct timeline_execution_context *executor,
    struct timeline_instruction *instruction
)
{
    while (instruction) {

        struct timeline_instruction *const n = instruction->in_order_next;
        if (!--instruction->ref_count) {
            timeline_instruction_free(executor, instruction);
        }
        instruction = n;
    }
}
