//
//  config.h
//  Monocurl
//
//  Created by Manu Bhat on 11/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>
#include <stdlib.h>

#include "mc_macro_util.h"

#define MAX_SLIDE_COUNT 1024

#define APP_NAME "monocurl"
#define SCENE_FILE_TYPE "scene"

#define APP_MAJOR 0
#define APP_MINOR 1
#define APP_PATCH 0

#define APP_VERSION                                                            \
    MC_STRINGIFY(APP_MAJOR)                                                    \
    "." MC_STRINGIFY(APP_MINOR) "." MC_STRINGIFY(APP_PATCH)
