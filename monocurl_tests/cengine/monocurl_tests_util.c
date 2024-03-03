//
//  monocurl_tests_util.c
//  Monocurl
//
//  Created by Manu Bhat on 1/4/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <stdio.h>
#include <stdarg.h>
#include <stdlib.h>

#include "monocurl_tests_util.h"


const char* monocurl_format(const char* format, ...) {
    char* ret;
    va_list args;
    va_start(args, format);
    vasprintf(&ret, format, args);
    va_end (args);
    
    return ret;
}
