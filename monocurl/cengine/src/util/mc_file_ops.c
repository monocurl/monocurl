//
//  mc_file_ops.c
//  Monocurl
//
//  Created by Manu Bhat on 12/18/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//
#include <plibsys.h>

#include "mc_file_ops.h"

mc_bool_t
mc_file_exists(char const *path)
{
    return p_file_is_exists(path) != 0;
}
