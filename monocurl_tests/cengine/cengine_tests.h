//
//  cengine_tests.h
//  Monocurl
//
//  Created by Manu Bhat on 1/3/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#ifndef cengine_tests_h
#define cengine_tests_h

#include <stdio.h>

extern void (*report_failure)(char const* description, char const* file, size_t line);
extern void (*report_success)(char const* description, char const* file, size_t line);

void cengine_tests_run(void);

#endif /* cengine_tests_h */
