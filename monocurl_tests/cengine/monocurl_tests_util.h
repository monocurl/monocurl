//
//  monocurl_tests_util.h
//  Monocurl
//
//  Created by Manu Bhat on 1/4/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#ifndef monocurl_tests_util_h
#define monocurl_tests_util_h

#include <stdio.h>
#include "cengine_tests.h"

const char* monocurl_format(const char* format, ...);

#define monocurl_assert_desc(cond, desc) cond ? report_success(desc, __FILE__, __LINE__) : report_failure(desc, __FILE__, __LINE__)
#define monocurl_assert_eq_desc(a, b, desc) monocurl_assert_desc(a == b, desc)

#define monocurl_assert(cond) monocurl_assert_desc(cond, #cond)
#define monocurl_assert_eq(a, b) monocurl_assert_eq_desc(a, b, #a" == "#b)
#define monocurl_assert_doubles(a, b) monocurl_assert_eq_desc(a, b, monocurl_format("%s [recorded: %f] != %s [recorded: %f]",#a,(double) (a),#b,(double)(b)))

#endif /* monocurl_tests_util_h */
