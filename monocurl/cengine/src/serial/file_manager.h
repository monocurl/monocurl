//
//  file_manager.h
//  Monocurl
//
//  Created by Manu Bhat on 11/10/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_env.h"
#include "mc_types.h"
#include "scene_handle.h"
#include <stdio.h>

#if MC_INTERNAL
// null on error
char *
file_read_bytes(char const *path);

struct raw_slide_model *
file_read_std(void);

#endif

// done on calling thread
void
file_write_model(struct scene_handle *scene);

struct scene_handle *
file_read_sync(char const *path);

mc_status_t
file_write_default_scene(char const *path);

/* void file_write_default_image(char const *path); */
