//
//  main.h
//  Monocurl
//
//  Created by Manu Bhat on 9/10/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "actions.h"
#include "callback.h"
#include "file_manager.h"
#include "scene_handle.h"
#include "tetramesh.h"
#include "timeline.h"
#include "viewport.h"

void
monocurl_init(void);

char const *
monocurl_version_str(void);

void
monocurl_free(void);
