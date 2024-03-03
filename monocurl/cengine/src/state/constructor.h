//
//  constructor.h
//  Monocurl
//
//  Created by Manu Bhat on 9/25/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "tree.h"

#if MC_INTERNAL
struct aux_group_model *
aux_group_custom_blank(char const *title);

struct raw_slide_model *
slide_standard(char const *name);
#endif
