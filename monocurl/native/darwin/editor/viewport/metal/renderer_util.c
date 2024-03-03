//
//  util.c
//  Monocurl
//
//  Created by Manu Bhat on 11/6/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <assert.h>
#include <simd/simd.h>

#include "renderer_util.h"

struct tri_vert_in const* tri_buffer_pointer_for(struct tetramesh const* mesh, size_t* count) {
    assert(mesh->tris);
    
    struct tri_vert_in* const ret = malloc(3 * sizeof(struct tri_vert_in) * mesh->tri_count);
    struct tri_vert_in* current = ret;
    
    for (size_t i = 0; i < mesh->tri_count; ++i) {
        struct tetra_tri const tri = mesh->tris[i];
        
        struct tetra_tri_vertex const vert_a = tri.a;
        struct tetra_tri_vertex const vert_b = tri.b;
        struct tetra_tri_vertex const vert_c = tri.c;
        
        if (vert_a.col.w < FLT_EPSILON && vert_b.col.w < FLT_EPSILON && vert_c.col.w < FLT_EPSILON) continue;;
        
        struct tri_vert_in const a = {
            .pos = simd_make_float3(vert_a.pos.x, vert_a.pos.y, vert_a.pos.z),
            .norm = simd_make_float3(vert_a.norm.x, vert_a.norm.y, vert_a.norm.z),
            .uv = simd_make_float2(vert_a.uv.x, vert_a.uv.y),
            .col = simd_make_float4(vert_a.col.x, vert_a.col.y, vert_a.col.z, vert_a.col.w)
        };
        
        struct tri_vert_in const b = {
            .pos = simd_make_float3(vert_b.pos.x, vert_b.pos.y, vert_b.pos.z),
            .norm = simd_make_float3(vert_b.norm.x, vert_b.norm.y, vert_b.norm.z),
            .uv = simd_make_float2(vert_b.uv.x, vert_b.uv.y),
            .col = simd_make_float4(vert_b.col.x, vert_b.col.y, vert_b.col.z, vert_b.col.w)
        };
        
        struct tri_vert_in const c = {
            .pos = simd_make_float3(vert_c.pos.x, vert_c.pos.y, vert_c.pos.z),
            .norm = simd_make_float3(vert_c.norm.x, vert_c.norm.y, vert_c.norm.z),
            .uv = simd_make_float2(vert_c.uv.x, vert_c.uv.y),
            .col = simd_make_float4(vert_c.col.x, vert_c.col.y, vert_c.col.z, vert_c.col.w)
        };
        
        *current++ = a;
        *current++ = b;
        *current++ = c;
        
        ++*count;
    }
    
    return ret;
}

struct lin_vert_in const* lin_buffer_pointer_for(struct tetramesh const* mesh, size_t* count) {
    assert(mesh->lins);
    
    struct lin_vert_in* const ret = malloc(6 * sizeof(struct lin_vert_in) * mesh->lin_count);
    struct lin_vert_in* current = ret;
    
    //might be able to micro optimize this by using same variable as lin_vert_in
    //and then only changing what's necessary..
    for (size_t i = 0; i < mesh->lin_count; ++i) {
        struct tetra_lin const line = mesh->lins[i];
        
        if (!line.is_dominant_sibling) continue;
        else if (line.a.col.w < FLT_EPSILON && line.b.col.w < FLT_EPSILON) continue;
        
        struct tetra_lin const prev = line.prev >= 0 ? mesh->lins[line.prev] : line;
        struct tetra_lin const next = line.next >= 0 ? mesh->lins[line.next] : line;

        struct tetra_lin_vertex const vert_p = prev.a;
        struct tetra_lin_vertex const vert_a = line.a;
        struct tetra_lin_vertex const vert_b = line.b;
        struct tetra_lin_vertex const vert_n = next.b;
        
        simd_float3 const pos_p = simd_make_float3(vert_p.pos.x, vert_p.pos.y, vert_p.pos.z);
        simd_float3 const pos_a = simd_make_float3(vert_a.pos.x, vert_a.pos.y, vert_a.pos.z);
        simd_float3 const pos_b = simd_make_float3(vert_b.pos.x, vert_b.pos.y, vert_b.pos.z);
        simd_float3 const pos_n = simd_make_float3(vert_n.pos.x, vert_n.pos.y, vert_n.pos.z);
        
        simd_float3 const tan = pos_b - pos_a;
        simd_float3 const prev_tan = pos_a - pos_p;
        simd_float3 const next_tan = pos_n - pos_b;
        
        simd_float4 const col_a = simd_make_float4(vert_a.col.x, vert_a.col.y, vert_a.col.z, vert_a.col.w);
        simd_float4 const col_b = simd_make_float4(vert_b.col.x, vert_b.col.y, vert_b.col.z, vert_b.col.w);
        
        simd_float3 const norm = simd_make_float3(line.norm.x, line.norm.y, line.norm.z);
        simd_float3 const prev_norm = simd_make_float3(prev.norm.x, prev.norm.y, prev.norm.z);
        simd_float3 const next_norm = simd_make_float3(next.norm.x, next.norm.y, next.norm.z);

        struct lin_vert_in const prev_join = {
            .pos = pos_a,
            .col = col_a,
            .tangent = tan,
            .norm = norm,
            .prev_tan = prev_tan,
            .prev_norm = prev_norm,
            .extrude = 1
        };
        struct lin_vert_in const prev_base = {
            .pos = pos_a,
            .col = col_a,
            .tangent = tan,
            .norm = norm,
            .prev_tan = tan,
            .prev_norm = norm,
            .extrude = 0
        };
        struct lin_vert_in const prev_extrude = {
            .pos = pos_a,
            .col = col_a,
            .tangent = tan,
            .norm = norm,
            .prev_tan = tan,
            .prev_norm = norm,
            .extrude = 1
        };
        
        struct lin_vert_in const next_base = {
            .pos = pos_b,
            .col = col_b,
            .tangent = tan,
            .norm = norm,
            .prev_tan = tan,
            .prev_norm = norm,
            .extrude = 0
        };
        struct lin_vert_in const next_extrude = {
            .pos = pos_b,
            .col = col_b,
            .tangent = tan,
            .norm = norm,
            .prev_tan = tan,
            .prev_norm = norm,
            .extrude = 1
        };
        struct lin_vert_in const next_join = {
            .pos = pos_b,
            .col = col_b,
            .tangent = tan,
            .norm = norm,
            .prev_tan = next_tan,
            .prev_norm = next_norm,
            .extrude = 1
        };
        
        
        *current++ = prev_join;
        *current++ = prev_base;
        *current++ = prev_extrude;
        
        *current++ = next_base;
        *current++ = next_extrude;
        *current++ = next_join;
        
        ++*count;
    }
    
    return ret;
}

struct dot_vert_in const* dot_buffer_pointer_for(struct tetramesh const* mesh, size_t* count) {
    assert(mesh->dots);
    
    struct dot_vert_in* const ret = malloc(sizeof(struct dot_vert_in) * mesh->dot_count);
    struct dot_vert_in* current = ret;
    
    for (size_t i = 0; i < mesh->dot_count; ++i) {
        struct tetra_dot const dot = mesh->dots[i];
        
        if (dot.col.w < FLT_EPSILON) continue;;
        
        struct dot_vert_in const a = {
            .col = simd_make_float4(dot.col.x, dot.col.y, dot.col.z, dot.col.w),
            .pos = simd_make_float3(dot.pos.x, dot.pos.y, dot.pos.z),
            .norm = simd_make_float3(dot.norm.x, dot.norm.y, dot.norm.z)
        };
        
        *current++ = a;
        ++*count;
    }
    
    return ret;
}
