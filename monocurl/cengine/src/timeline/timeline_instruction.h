//
//  timeline_instrution.h
//  monocurl
//
//  Created by Manu Bhat on 12/5/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "entry.h"
#include "mc_env.h"
#include "vector_field.h"

struct timeline_execution_context;

/* Polymorphic base class*/
struct timeline_expression_node {
    struct vector_field (*execute)(
        struct timeline_expression_node *instruction,
        struct timeline_execution_context *context
    );
    void (*free)(
        struct timeline_expression_node *instruction,
        struct timeline_execution_context *context
    );
};

/* hmmmm what's best way to do this??? */
/* maybe the recursion goes on in the instruction class */
// from there, each type of node dictates what's next
// in_order_next is always the immediate next on the same level
// conditional next is generally immediate next
// but if previous is an if or else statement, then conditional next goes all
// the way to the front
struct timeline_instruction {
    struct raw_slide_model *slide;
    mc_count_t line_no;

    struct timeline_expression_node *root;

    struct timeline_instruction *next;
    struct timeline_instruction *conditional_next;

    struct timeline_instruction *in_order_next;
    struct timeline_instruction *in_order_prev;

#pragma message("TODO, ref count no longer needed?")
    unsigned int var_count, ref_count;

    enum {
        INSTRUCTION_CONDITIONAL_NONE = 0,
        INSTRUCTION_CONDITIONAL_IF = 1,
        INSTRUCTION_CONDITIONAL_ELSE = 2,
        INSTRUCTION_CONDITIONAL_ELSE_IF = 3
    } conditional; /* in relation to else and else if statements, so that the
                      next can be done properly*/
};

#if MC_INTERNAL
// runs the entire instruction list
inline struct vector_field
timeline_instruction_full_execute(
    struct timeline_instruction *instruction,
    struct timeline_execution_context *context
)
{
    return instruction->root->execute(instruction->root, context);
}

struct timeline_instruction *
timeline_instruction_parse(
    struct timeline_execution_context *executor,
    struct timeline_instruction *prev, struct aux_entry_model *node,
    mc_bool_t modify
);

struct timeline_instruction *
timeline_instruction_identity(struct timeline_execution_context *executor);

void
timeline_instruction_unref(
    struct timeline_execution_context *executor,
    struct timeline_instruction *instruction
);
#endif
