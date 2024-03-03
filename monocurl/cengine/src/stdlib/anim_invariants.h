//
//  anim_invariants.h
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once
#include <stdio.h>

#include "mc_anims.h"
#include "mc_env.h"

/*
 func Maintain([mesh_tree] {[root] {dst&}, [subfield] {dst&, subfield(ref)},
 [subtags] {dst_root&, subtags, leaf_subfield(ref, tag)}, [predicate]
 {dst_root&, tag_predicate(tag), leaf_subfield(ref, tag)}}, [source] {[main]
 {src&}, [stateful] {src_root&, src_func(src_root, t)}}, [extraneous]
 {[passive] {min_time, sticky}, [main] {time, sticky}}) = Anim: func
 MaintainState(dst&, src&) = 0 func MaintainCamera(x) = 0 func KeyFrame(x) = 0
 func KeyFrameState(x) = 0
 */

#if MC_INTERNAL
LIBMC_DEC_FUNC(set);

LIBMC_DEC_FUNC(maintain);
LIBMC_DEC_FUNC(lerp_anim);
LIBMC_DEC_FUNC(transfer);
LIBMC_DEC_FUNC(transfer_runtime);

struct vector_field
parse_src_keyframe(
    struct timeline_execution_context *executor, double t,
    struct vector_field *fields, mc_bool_t *finished
);

#endif
