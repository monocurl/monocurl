//
//  callback.h
//  Monocurl
//
//  Created by Manu Bhat on 10/17/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <stdio.h>

#include "mc_env.h"
#include "mc_types.h"
#include "timeline.h"
#include "tree.h"
#include "viewport.h"

#if MC_INTERNAL
void
pre_modify(struct raw_scene_model *scene);

void
post_slide_modify(struct raw_slide_model *reference, mc_bool_t is_global);

void
post_scene_modify(struct raw_scene_model *reference, mc_bool_t is_global);

void
post_history_modify(void);

void
post_filewriter_modify(void);
#endif

// on windows, we use the linker
// on macos, we use function pointers since
// we can't call swift code directly
#if MC_ENV_OS & MC_ENV_OS_WINDOWS
#define _PTR_SYMBOL
#else
#define _PTR_SYMBOL *
#endif

extern void(_PTR_SYMBOL slide_flush)(
    struct raw_slide_model *reference, mc_bool_t is_global
);
extern void(_PTR_SYMBOL scene_flush)(
    struct raw_scene_model *reference, mc_bool_t is_global
);

extern void(_PTR_SYMBOL viewport_flush)(struct viewport *reference);
extern void(_PTR_SYMBOL timeline_flush)(struct timeline *reference);

// buffers are written on native code, so no need for writing that here
extern mc_handle_t(_PTR_SYMBOL poll_texture)(char const *path);
extern void(_PTR_SYMBOL free_buffer)(mc_handle_t buffer);

// exporting
extern void(_PTR_SYMBOL export_frame)(struct timeline const *timeline);
extern void(_PTR_SYMBOL export_finish)(
    struct timeline const *timeline, char const *error
);

// expected to return allocated string
extern char const *(_PTR_SYMBOL path_translation)(char const *handle);

// expected to return string literals (should not be freed)
extern char const *(_PTR_SYMBOL std_lib_path)(void);
extern char const *(_PTR_SYMBOL default_scene_path)(void);
extern char const *(_PTR_SYMBOL tex_binary_path)(void);
extern char const *(_PTR_SYMBOL tex_intermediate_path)(void);

/* debugging */
#if MC_ENV_OS & MC_ENV_OS_WINDOWS && MC_DEBUG
void
debug_write_log(char const *str);
#endif

#undef _PTR_SYMBOL
