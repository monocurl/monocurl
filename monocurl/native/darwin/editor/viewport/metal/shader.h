//
//  shader.h
//  Monocurl
//
//  Created by Manu Bhat on 10/31/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#ifndef shader_h
#define shader_h

#include <simd/simd.h>

//should strokes be subject to lighting?
//I say yes, in worst case you can just select strokes yourself and we'll go with that

//uniforms
struct tri_vert_uniform {
    matrix_float4x4 mv;
    matrix_float4x4 p;
    matrix_float4x4 normal;
    float z_offset;
};

struct tri_frag_uniform {
    //again pretty easy
    //lighting at some point
    float opacity, gloss;
};

struct lin_vert_uniform {
    //screen size
    //anyways just extend out and assume ccw?, hmm idk, actually lets just make all surfaces doubly oriented?
    //hm maybe for degenerate line we set normal to 0 (which can be done manualy anyways)
    //in which case it's just radius
    //ok i think that works then?
    vector_float2 inlet_size;
    
    float radius; //pixels
    float max_miter_scale;
    
    matrix_float4x4 mv;
    matrix_float4x4 p;
    matrix_float4x4 normal;
    
    float z_offset;
};
struct lin_frag_uniform {
    //shouldn't be bad
    float opacity, gloss;
};

struct dot_vert_uniform {
    //hmmm, i say we just compute indices dynamically cpu side, depending on size
    //again we can specify normal and have it flat, or make it a sphere
    uint16_t vertex_count;
    vector_float2 viewport_size;
    float radius; //pixels
    
    matrix_float4x4 mv;
    matrix_float4x4 p;
    matrix_float4x4 normal;
    
    float z_offset;
};
struct dot_frag_uniform {
    //is lighting per pixel or the same for a dot
    //either way, empty for now ig
    //but in future we might include lighting here...
    float opacity, gloss;
};

//per vertex data, complete
struct tri_vert_in {
    //simplest, just have the vertex normals and such
    //have to implement culling
    vector_float3 pos;
    vector_float3 norm;
    vector_float2 uv;
    vector_float4 col;
};

struct lin_vert_in {
    //line normal, conjoined normal, actual point (6 points in total?)
    //if it's the two base points it's easy
    //extruded points aren't bad either
    //but we also have to consider if it's degenerate normal
    vector_float3 pos;
    vector_float4 col;
    
    //hmmmm so what we do in case of degenerate, send 4 vectors?
    //for non degenerate, both normals are enough
    //if one is degenerate, both are necessarily degenerate
    vector_float3 tangent;//degenerate on the start line, otherwise never
    vector_float3 norm;//possibly degenerate
    vector_float3 prev_tan;//previous tangent, never degenerate
    vector_float3 prev_norm;
    int extrude;
};

//complete
struct dot_vert_in {
    //just the single point and vertex ind
    vector_float4 col;
    vector_float3 pos;
    vector_float3 norm;
};

#endif /* shader_h */
