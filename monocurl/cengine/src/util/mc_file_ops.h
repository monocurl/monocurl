//
//  mc_file_ops.h
//  Monocurl
//
//  Created by Manu Bhat on 12/18/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_env.h"
#include "mc_types.h"

#if MC_INTERNAL
mc_bool_t
mc_file_exists(char const *path);
#endif
