//
//  expression_tests.c
//  Monocurl
//
//  Created by Manu Bhat on 1/3/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <string.h>
#include <stdlib.h>
#include <math.h>

#include "monocurl_tests_util.h"
#include "expression_tests.h"
#include "vector_field.h"
#include "entry.h"
#include "functor.h"
#include "timeline_instruction.h"
#include "constructor.h"
#include "monocurl.h"

#define MAX_DEPTH 10

static struct timeline_execution_context* executor = NULL;
struct raw_scene_model* scene;

void dummy_handler(void* pointer, char is_global) {
}
void dummy_etc_handler(void* pointer) {
    
}

struct raw_slide_model* slide_for(const char* str) {
    if (!executor) {
        struct scene_handle* scene_handle = calloc(1, sizeof(struct scene_handle));
        scene_handle->model = init_scene_session(scene_handle);
        scene_handle->timeline = timeline_init(scene_handle);
        executor = scene_handle->timeline->executor;
        scene = init_scene_session(NULL);
        
        entry_flush = (void*) dummy_handler;
        group_flush = (void*) dummy_handler;
        slide_flush = (void*) dummy_handler;
        scene_flush = (void*) dummy_handler;
        
        timeline_flush = (void*) dummy_etc_handler;
        viewport_flush = (void*) dummy_etc_handler;
    }
 
    char* st = strdup(str);
       
    struct raw_slide_model* const s = slide_standard("Example");
    scene->slide_count = 0; //quick hack for testing
    slide_append_to_scene(s, scene, 0, 0);

    struct {
        struct raw_slide_model* slide;
        struct raw_group_model* group;
        size_t count;
    } stack[MAX_DEPTH];
    
    stack[0].group = NULL;
    stack[0].slide = s;
    stack[0].count = 1;
    
    char* token;
    while ((token = strsep(&st, "\n"))) {
        size_t tabs = 0;
        while (*token == '\t') {
            ++token;
            ++tabs;
        }
        char* title = NULL;
        /* not general...*/
        for (char* repeat = token;*repeat;++repeat) {
            if (*repeat == ':' && repeat[1] != '<' && tabs) {
                title = token;
                *repeat = 0;
                token = repeat + 1;
                break;
            }
        }
        
        struct raw_group_model* const group = group_custom_blank(title);
        group->modes[0].entries[0].data = strdup(token);
        
        stack[tabs + 1].count = 0;
        stack[tabs + 1].group = group;
        stack[tabs + 1].slide = s;
        
        tree_general_insert(s, stack[tabs].group, group, stack[tabs].count++, 0);
    }
    
    return s;
}

/* get the expected and actual results */
static struct vector_field run_expression(const char* str) {
    struct raw_slide_model* s = slide_for(str);
   
    timeline_executor_resize(executor, scene, 1);
    slide_delete(s, 0);
  
    
    if (!executor->slides[1].instructions) {
        struct vector_field special = VECTOR_FIELD_NULL;
        special.value.doub = -1.5;
        return special;
    }
    
    struct timeline_instruction* instruction = executor->slides[1].instructions;

    struct timeline_slide* const slide = executor->slides;

    for (size_t i = 0; i < executor->stack_frame; ++i) VECTOR_FIELD_FREE(executor, executor->stack[i]);
    executor->stack_frame = slide->stack_frame;
    for (size_t i = 0; i < executor->stack_frame; ++i) executor->stack[i] = VECTOR_FIELD_COPY(executor, slide->stack[i]);
    
    /* capture_frame does not need to be reallocated since it*/
    executor->capture_count = slide->capture_count;
    /* capture_frame does not need to be reallocated since it*/
    for (size_t i = 0; i < executor->capture_count; ++i) executor->capture_frame[i] = VECTOR_FIELD_COPY(executor, slide->capture_frame[i]);
    
    for (size_t i = 0; i < executor->mesh_count; ++i) VECTOR_FIELD_FREE(executor, executor->meshes[i]);
    executor->mesh_count = slide->mesh_count;
    executor->meshes = reallocf(executor->meshes, sizeof(struct vector_field) * executor->mesh_count);
    for (size_t i = 0; i < executor->mesh_count; ++i) executor->meshes[i] = VECTOR_FIELD_COPY(executor, slide->meshes[i]);
    
    struct vector_field ret = timeline_executor_execute(executor, instruction, 1, 1);
    slide_free(s);
    
    return vector_field_safe_extract_type(executor, ret, VECTOR_FIELD_PURE);
}

static char error(const char* str) {
    struct vector_field ret = run_expression(str);
    return ret.value.doub == -1.5 && !ret.vtable;
}

/* may be differences due to integer division and c not having ** */
#define assert_expr(expr) monocurl_assert_doubles(run_expression(#expr).value.doub, (expr))

void expression_tests_run(void) {
    monocurl_assert_doubles(run_expression("func swap(y&) = identity:\n"
                                         "\ty = {y[1], y[0]}\n"
                                         "\telement: 0\n"
                                         "var p = -1\n"
                                         "var q = 2\n"
                                         "let ret = swap:\n"
                                         "\ty&: {p, q}\n"
                                         "q"
                                         ).value.doub, -1);
    
    /* passing in function*/
    assert_expr(1 + 2 + 3);
    assert_expr(1 * 2 + 3);
    assert_expr(1 - 2 * 3);
    assert_expr((1 - 2) * 3);
    assert_expr((((((1 - 2))))) + 3);
    assert_expr(!(1 + 2));
    assert_expr(0 && 1 > 1 && 2 < 0 + 3 - 3 * 4 || 0);
    assert_expr(0 && 1 || 1);
    assert_expr(0 == 2);
    assert_expr(0 == 0);
    assert_expr(0 != 2);
    assert_expr(0 != 3);
    assert_expr(0.002 <= 2);
    assert_expr(0.2342 >= 2);
    assert_expr(-1 + 2);
    assert_expr(-1 + -2);
    
    monocurl_assert_doubles(run_expression("var y = 0\n"
                                           "for x in 1:<100\n"
                                           "\ty=y+x\n"
                                           "\tlet g = y\n"
                                           "\t/* comment */\n"
                                           "\tfor j in 1:<x\n"
                                           "\t\tvar q = g\n"
                                           "\t\tq=q-g\n"
                                           "\t\ty+=q\n"
                                           "y").value.doub, 50 * (99));
    
    monocurl_assert_doubles(run_expression("var y = 0\n"
                                           "var mods = {1:2,3:4,5:6}\n"
                                           "for x in mods\n"
                                           "\ty+=x\n"
                                           "\tmods[x+1]=3\n"
                                           "y"
                                           ).value.doub, 1 + 3 + 5);
    
    monocurl_assert_doubles(run_expression("var y = 0\n"
                                           "if 1\n"
                                           "\ty=3\n"
                                           "if 0\n"
                                           "\ty=4\n"
                                           "else if 0\n"
                                           "\ty=5\n"
                                           "else if y\n"
                                           "\ty+=6\n"
                                           "else if y\n"
                                           "\ty=-1\n"
                                           "else \n"
                                           "\ty=-2\n"
                                           "if y == 9\n"
                                           "\ty=11\n"
                                           "y"
                                           ).value.doub, 11);
    
    monocurl_assert_doubles(run_expression("var y = 0\n"
                                           "while y < 10\n"
                                           "\ty=y+1\n"
                                           "y"
                                           ).value.doub, 10);
    monocurl_assert_doubles(run_expression("func f(x)=x+1\n"
                                           "func g(x)=2*x+1\n"
                                           "f(1+f(1)+g(2))"
                                           ).value.doub, 9);
    monocurl_assert_doubles(run_expression(
                                   "func swap(y&) = identity:\n"
                                   "\ty = {y[1], y[0]}\n"
                                   "\telement: 0\n"
                                   "var q = -1\n"
                                   "let ret = swap:\n"
                                   "\tvar p = 2\n"
                                   "\ty&: {p, q}\n"
                                   "q"
                                   ).value.doub, 2);
    
  
    monocurl_assert(!run_expression(
                                    "var x = 4\n"
                                    "identity(x+1)=3\n"
                                    "x"
                                   ).vtable);
    monocurl_assert(!run_expression("func identity(x)=x\n"
                                    "let x = 4\n"
                                    "identity(x)=3\n" // can't change an immutable variable
                                    "x"
                                   ).vtable);
    monocurl_assert(!run_expression("func identity_not(x)=x+1\n"
                                    "var x = 4\n"
                                    "identity_not(x)=3\n"
                                    "x"
                                    ).vtable);
    monocurl_assert(!run_expression("var y = 0\n"
                                    "func identity_not(x)=x+y\n"
                                    "var x = 4\n"
                                    "identity_not(x)=3\n"
                                    "x"
                                    ).vtable);
    monocurl_assert_doubles(run_expression("let y = 1\n"
                                    "func identity_not(x)=x+y\n"
                                    "var z = 0\n"
                                    "z=identity_not(3)\n"
                                    "z"
                                    ).value.doub,4);
   
    monocurl_assert_doubles(run_expression("let y = 1\n"
                                           "func g(x)=y\n"
                                           "func identity_not(x)=x+g(x)\n"
                                           "var z = 0\n"
                                           "z=identity_not(3)\n"
                                           "z"
                                            ).value.doub,4);
    monocurl_assert_doubles(run_expression("func parity(x) = x==1 || !parity(x-1)\n"
                                           "parity(25)"
                                           ).value.doub, 1);
    
    monocurl_assert_doubles(run_expression("func multi(a,b,c) = a * a + b * a + a * c\n"
                                           "multi(multi(2, 3, -1) - 6, 3, -1)"
                                           ).value.doub, 8);
    
   

//    monocurl_assert_doubles(run_expression("/* comment test*/{1,2,3,4}[2:<3][0]").value.doub, 3);
//    monocurl_assert_doubles(run_expression("{1,2,3,4}[2:<4][1]").value.doub, 4);
    monocurl_assert_doubles(run_expression("3 in {1,2,3,4}").value.doub, 1);
    monocurl_assert_doubles(run_expression("3 in {1:0,2:0,3:0,4:0}").value.doub, 1);
    monocurl_assert_doubles(run_expression("10 in {1,2,3,4}").value.doub, 0);
    monocurl_assert_doubles(run_expression("10 in {1:10,2:10,3:10,4:10}").value.doub, 0);

    monocurl_assert_doubles(run_expression("4 * -2 ** -3 * 4 + 4 - 3").value.doub, 4 * pow(-2, -3) * 4 + 4 - 3);
    
    monocurl_assert_doubles(run_expression("var x = {1,2,4}\nx[0]").value.doub, 1);
    monocurl_assert_doubles(run_expression("let x = 2*3+3+3\nx+2").value.doub, 14);
    monocurl_assert_doubles(run_expression("var x = 2*3+3+3\nx=3").value.doub, 3);
    monocurl_assert_doubles(run_expression("var x = {1,2,4}\n{x}={3}\nx").value.doub, 3);
    monocurl_assert_doubles(run_expression("var x = {1,2,4}\nx[0]=3\nx[0]").value.doub, 3);
    monocurl_assert_doubles(run_expression("var x = {1,2,4}\nlet y = x\nx[0]=3\nx[0]-y[0]").value.doub, 2);
    monocurl_assert_doubles(run_expression("var x = {1,2,4}\nlet y = x+x\nx[2]=3\ny[3][2]").value.doub, 4);
    monocurl_assert_doubles(run_expression("var x = {1,2,4}\nlet y = x\nx[0]=3\nx[0]").value.doub, 3);
   
    monocurl_assert_doubles(run_expression("var x = 0\nvar y = 1\n{x,y}={1,3}\nx").value.doub, 1);
    monocurl_assert_doubles(run_expression("var x = {1,2}\nx += 3\nx+=4\nx[2]").value.doub, 3);
    monocurl_assert_doubles(run_expression("var x = {:}\nx[2]=10\nvar y = x\n{x[5],x[2]}={10,4}\nx[2]+x[5]+y[2]").value.doub, 24);
    monocurl_assert_doubles(run_expression("var x = {1,2,4}\n"
                                           "var y = 1\n"
                                           "{x,y}={y,x}\n"
                                           "x").value.doub, 1);

    monocurl_assert_doubles(run_expression("var x = {1,2,4}\n"
                                           "var y = 1\n"
                                           "{x,y}={y,x}\n"
                                           "y[1]").value.doub, 2);
    
   
    monocurl_assert_doubles(run_expression("func multi(a,b,c) = a * a + b * a + a * c\n"
                                           "var y = multi:\n"
                                           "\ta: 2\n"
                                           "\tb: 3\n"
                                           "\tc: 1\n"
                                           "y.c = -1\n"
                                           "y"
                                           ).value.doub, 8);
    monocurl_assert_doubles(run_expression("func multi(a,b,c) = a * a + b * a + a * c\n"
                                           "var y = multi:\n"
                                           "\ta: 2\n"
                                           "\tb: 3\n"
                                           "\tc: 1\n"
                                           "y.c = -1\n"
                                           "y.c + y.a + y.b"
                                           ).value.doub, 4);
    monocurl_assert_doubles(run_expression("var a = 0\n"
                                           "func multi(a,b,c) = a * a + b * a + a * c\n"
                                         
                                           "var y = multi:\n"
                                           "\ta: 2\n"
                                           "\tb: 3\n"
                                           "\tc: 1\n"
                                           "y.c = -1\n"
                                           "y.c + y.a + y.b"
                                           ).value.doub, 4);
    monocurl_assert_doubles(run_expression("func identity_(x) = x\n"
                                           "var y = identity_:\n"
                                           "\tx: {1,4,5}\n"
                                           "let copy_y = y\n"
                                           "let copy_x = y.x\n"
                                           "y.x = {0, 1, 3}\n"
                                           "y.x[1] - copy_x[1] - copy_x[2]"
                                           ).value.doub, -8);
//    monocurl_assert_doubles(run_expression("var x = 4\n"
//                                           "native unconst(identity(x))=3\n"
//                                           "x"
//                                           ).value.doub, 3);
//    monocurl_assert_doubles(run_expression("var x = {4}\n"
//                                           "native unconst(identity(x[0]))=3\n"
//                                           "x[0]"
//                                           ).value.doub, 3);
    
    
    
    
    monocurl_assert_doubles(run_expression("let helper = 1\n"
                                           "func identity_(x) = helper * x * helper * helper\n"
                                           "func matches(vec, pred(x)) = identity_:\n"
                                           "\tvar ret = 0 * helper\n"
                                           "\tfor y in vec\n"
                                           "\t\tif pred(y)\n"
                                           "\t\t\tret = ret + helper\n"
                                           "\tx: ret\n"
                                           "func base(x) = x * helper * 9 / x\n"
                                           "let cnt = matches:\n"
                                           "\tfunc comp(x) = x * x <= base(x) * helper\n"
                                           "\tvec: {4,3,2,helper}\n"
                                           "\tpred(x): comp(x)\n"
                                           "cnt + 0"
                                           ).value.doub, 3);
    monocurl_assert_doubles(run_expression("func multi(a,b,c) = a * a + b * a + a * c\n"
                                           "let y = multi:\n"
                                           "\ta: 2\n"
                                           "\tb: 3\n"
                                           "\tc: -1\n"
                                           "y"
                                           ).value.doub, 8);

    monocurl_assert_doubles(run_expression("let double_capture = 2\n"
                                            "func identity_(x) = x * double_capture\n"
                                            "func multi(a,b,c) = 1/double_capture * identity_:\n"
                                            "\tlet curr = a * a + b * a + a * c\n"
                                            "\tx: curr - double_capture\n"
                                            "let tmp = 1\n"
                                            "let y = multi:\n"
                                            "\ta: 2\n"
                                            "\tlet interupt = 0\n"
                                            "\tb: tmp * tmp + tmp + tmp\n"
                                            "\tc: -tmp + interupt\n"
                                            "y"
                                            ).value.doub, 6);
    
    
    monocurl_assert_doubles(run_expression("{1,2,4}[0]").value.doub, 1);
    
    monocurl_assert_doubles(run_expression("{1,2,3,4}[0]").value.doub, 1);
    monocurl_assert_doubles(run_expression("{1,2,3,4}[1]").value.doub, 2);
    monocurl_assert_doubles(run_expression("{1,2,3,4}[2]").value.doub, 3);
    monocurl_assert_doubles(run_expression("{1,2,3,4}[3]").value.doub, 4);
    
    monocurl_assert_doubles(run_expression("{1:5,\"2\":10,3:4,5:0}[1]").value.doub, 5);
    monocurl_assert_doubles(run_expression("{1:5,\"2\":10,3:4,5:0}[3]").value.doub, 4);
    monocurl_assert_doubles(run_expression("{1:5,\"2\":10,3:4,50234:-1}[50234]").value.doub, -1);
    monocurl_assert_doubles(run_expression("\"01234%{2**3}blank\"[5]").value.doub, 8);
    monocurl_assert_doubles(run_expression("{1:5,\"2\":10,3:4,5:8}[\"2\"]").value.doub, 10);
    monocurl_assert_doubles(run_expression("{1:5,\"2\":10,2:4,'2':'2',3:4,5:8}['2']").value.c, '2');
    
    monocurl_assert(!run_expression("1 = 3").vtable);
    monocurl_assert(!run_expression("{1} = {3}").vtable);
    monocurl_assert(!run_expression("let x = 2*3+3+3\nx=3").vtable);
    monocurl_assert(!run_expression("let x = {0,1,2,3}\nx[0]=3").vtable);
    monocurl_assert(!run_expression("var x = 0\nlet y = 1\n{x,y}={1,2}").vtable);
    monocurl_assert(!run_expression("var x = 0\var y = 1\n{x,y}={1}").vtable);
    monocurl_assert(!run_expression("let x = {1,2}\nx += 3\nx+=4\nx[2]").vtable);
    monocurl_assert(!run_expression("var x = {0}\nlet y = x + y").vtable);
    

    
    monocurl_assert(run_expression("{}={}").vtable);
    
    monocurl_assert(error("native bad_call()"));
    monocurl_assert(error("native test_animation"));
    
    monocurl_assert(error("var a = 0\nlet a = 0"));
   

    monocurl_assert(!error("\"string example %t %{1+2+3}\""));
    monocurl_assert(error("\"01234%{2*(*3})\""));
    monocurl_assert(error("\"string example %t %a\"[0]"));
    monocurl_assert(error("\"string example %{1+2+3\""));
    monocurl_assert(error("\"string example nicee"));
    
    monocurl_assert(error("{1:3,2,3,4}[0]"));
    monocurl_assert(error("{1"));
    monocurl_assert(error("{"));
    
    monocurl_assert(error("1."));
    monocurl_assert(error("1.aav.v"));
    monocurl_assert(error(".aav.v"));
    monocurl_assert(error("0.2()"));
    
    monocurl_assert(error("}"));
    
    monocurl_assert(error("{}[{a}]"));
    
    monocurl_assert(error("{a}[4]"));
    monocurl_assert(error("3[{a}]"));
    
    /* overflow */
    monocurl_assert(error("123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123123"));
}
