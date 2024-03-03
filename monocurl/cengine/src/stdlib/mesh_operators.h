//
//  mesh_operators.h
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#pragma once

#include "mc_env.h"
#include "mc_meshes.h"

/*
 func Project(x, y) = x
 func Uprank(x) = x
 func Downrank(x) = x
 func Masked(x,y) = 0
 func Joined(x,y) = 0
 func SetDifference(x,y) = 0
 func SymmetricDifference(vec) = 0
 func MinkowkskiSum(vec) = 0
 func Extended(x) = 0
 func Resampled(x) = x
 func Recolored(x) = x
 func Retextured(x) = x
 func Shifted(x) = x
 func Scaled(x) = x
 func MatchedEdge(x, y) = x
 func NextTo(x,y) = x
 func XStack(x,y) = x
 func YStack(x,y) = y
 func ZStack(x,y) = y
 func Stack(x,y,dir) = 0
 func ToEdge(x) = x
 func ToCorner(x) = x
 func Grid(x,y) = 0
 func Table(x,y) = 0
 func Map(x,pTransform(x)) = 0
 func ColorMap(x,pTransform(x)) = 0
 func UVMap(x, pTransform(x)) = 0
 func Field(domain, x) = 0
 func Extrude(x) = x
 func TransferToSpace(x) = x
 func WireframeSet(x) = x
 func VertexSet(x) = x
 func FaceSet(x) = x
 func TetrahedronSet(x) = x
 func GaussianSurfaces(x) = x
 func Loops(x) = x
 */

LIBMC_DEC_FUNC(mesh_scale);
LIBMC_DEC_FUNC(mesh_shift);
LIBMC_DEC_FUNC(mesh_rotate);
LIBMC_DEC_FUNC(mesh_project);
LIBMC_DEC_FUNC(mesh_faded);
LIBMC_DEC_FUNC(mesh_zindex);

LIBMC_DEC_FUNC(mesh_point_map);
LIBMC_DEC_FUNC(mesh_uv_map);
LIBMC_DEC_FUNC(mesh_color_map);
LIBMC_DEC_FUNC(mesh_retagged);
LIBMC_DEC_FUNC(mesh_tag_map);

LIBMC_DEC_FUNC(mesh_recolored);
LIBMC_DEC_FUNC(mesh_retextured);
LIBMC_DEC_FUNC(mesh_embed_in_space);
LIBMC_DEC_FUNC(mesh_subdivided);
LIBMC_DEC_FUNC(mesh_line_subdivided);
LIBMC_DEC_FUNC(mesh_extruded);
LIBMC_DEC_FUNC(mesh_revolved);
LIBMC_DEC_FUNC(mesh_glossy);

LIBMC_DEC_FUNC(mesh_uprank);
LIBMC_DEC_FUNC(mesh_downrank);

LIBMC_DEC_FUNC(mesh_downrank);

LIBMC_DEC_FUNC(mesh_centered);
LIBMC_DEC_FUNC(mesh_stack);
LIBMC_DEC_FUNC(mesh_to_side);
LIBMC_DEC_FUNC(mesh_matched_edge);
LIBMC_DEC_FUNC(mesh_next_to);

LIBMC_DEC_FUNC(mesh_grid);
LIBMC_DEC_FUNC(mesh_table);
