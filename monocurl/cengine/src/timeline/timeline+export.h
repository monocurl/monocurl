//
//  timeline+export.h
//  monocurl
//
//  Created by Manu Bhat on 11/30/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_types.h"
#include "timeline.h"

#if MC_INTERNAL
// called only once configuration is set into the variable
void
timeline_export(struct timeline *timeline);
#endif
