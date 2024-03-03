//
//  cengine_tests.c
//  Monocurl
//
//  Created by Manu Bhat on 1/3/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include "cengine_tests.h"
#include "mc_stdlib.h"
#include "expression_tests.h"

void (*report_failure)(char const* description, char const* file, size_t line);
void (*report_success)(char const* description, char const* file, size_t line);

void cengine_tests_run(void) {
    limbc_stdlib_init();
    
    expression_tests_run();
}
