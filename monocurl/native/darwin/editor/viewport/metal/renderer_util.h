//
//  util.h
//  Monocurl
//
//  Created by Manu Bhat on 11/6/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#ifndef renderer_util_h
#define renderer_util_h

#include <stdio.h>

#include "shader.h"
#include "tetramesh.h"

struct tri_vert_in const* tri_buffer_pointer_for(struct tetramesh const* mesh, size_t* count);
struct lin_vert_in const* lin_buffer_pointer_for(struct tetramesh const* mesh, size_t* count);
struct dot_vert_in const* dot_buffer_pointer_for(struct tetramesh const* mesh, size_t* count);

#endif /* renderer_util_h */
