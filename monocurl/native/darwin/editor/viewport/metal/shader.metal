//
//  shaders.metal
//  Monocurl
//
//  Created by Manu Bhat on 10/30/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <metal_stdlib>

#include "shader.h"

using namespace metal;

/* allow for a bit of overflow */
constant float3 LIGHT_SRC = float3(1, 1, 0);
constant float GAMMA = 3;

struct rasterized_tri {
    float4 pos [[position]];
    float3 model;
    half3 normal;
    half4 col;
    float2 uv;
};

struct rasterized_lin {
    float4 pos [[position]];
    half4 col;
};

struct rasterized_dot {
    float4 pos [[position]];
    half4 col;
};

float lighting(float3 norm, float3 model, float gloss) {
    float3 const unit = normalize(LIGHT_SRC - model);
    float const strength = dot(unit, norm);
     
    float const ret = gloss * pow(strength, GAMMA);
    return isnan(ret) || isinf(ret) ? 0 : ret;
}

vertex rasterized_tri tri_vert_shader(uint vertex_id [[vertex_id]], const device tri_vert_in *vertices [[buffer(0)]], constant tri_vert_uniform &uniform [[buffer(1)]]) {
    
    tri_vert_in const in = vertices[vertex_id];
    
    float4 const model = uniform.mv * float4(in.pos, 1);
    
    rasterized_tri out{};
    out.pos = uniform.p * model;
    out.pos.z -= uniform.z_offset;
    out.col = half4(in.col);
    out.uv = in.uv;
    out.normal = half3((uniform.normal * float4(in.norm, 0)).xyz);
    out.model = model.xyz;
   
    return out;
}

fragment half4 tri_frag_shader(rasterized_tri interpolated [[stage_in]], constant tri_frag_uniform &uniform [[buffer(0)]], texture2d<half> image [[texture(0)]]) {
    constexpr sampler sampler(filter::linear, mip_filter::linear, address::clamp_to_edge);
    half4 const col = interpolated.col;
    float2 const uv = interpolated.uv;
    float3 const norm = float3(normalize(interpolated.normal));

    float const specular = lighting(norm, interpolated.model, uniform.gloss);
    
    return half4(col.xyz + (1 - col.xyz) * specular, col.w * uniform.opacity) * image.sample(sampler, uv);
}

// have to decide whats better to do cpu side and whats better to do gpu side
// benchmark later...
vertex rasterized_lin lin_vert_shader(uint vertex_id [[vertex_id]], uint instance_id [[instance_id]], const device lin_vert_in *vertices [[buffer(0)]], constant lin_vert_uniform &uniform [[buffer(1)]]) {
    //6 vertices per line
    lin_vert_in const in = vertices[instance_id * 6 + vertex_id];

    rasterized_lin out{};
    out.col = half4(in.col);
    
    float4 model = uniform.mv * float4(in.pos, 1);
    
    float3 const tangent = (uniform.mv * float4(in.tangent, 0)).xyz;
    float3 const prev_tangent = (uniform.mv * float4(in.prev_tan, 0)).xyz;
    
    float3 const proj_normal = cross(tangent, float3(0,0,1));
    float3 const used_normal = normalize(proj_normal);
    
    float3 const prev_proj_normal = cross(prev_tangent, float3(0,0,1));
    float3 const prev_used_normal = normalize(prev_proj_normal);
    
    //in our case, the vector [0,1,0] will always map to the top of the screen, and then it's square
    float const scale = 2 * uniform.radius * model.z * uniform.p.columns[2].w / uniform.inlet_size.x * in.extrude;
    
    float3 miter_clip = (prev_used_normal + used_normal) / 2;
    float const proj = 1 / dot(miter_clip, used_normal);
    float3 un_clip = isinf(proj) ? float3() : miter_clip * proj;
    
    float const miter_length_squared = length_squared(un_clip);
    
    miter_clip *= scale;
    un_clip *= scale;
    
    float const compare_length = uniform.max_miter_scale * uniform.max_miter_scale;

    float3 const full_normal = miter_length_squared > compare_length ? miter_clip : un_clip;

    model += float4(full_normal, 0);
    out.pos = uniform.p * model;
    out.pos.z -= uniform.z_offset;
    
    return out;
}

fragment half4 lin_frag_shader(rasterized_lin interpolated [[stage_in]], constant lin_frag_uniform &uniform [[buffer(0)]]) {
    half4 const col = interpolated.col;
    return half4(col.xyz, col.w * uniform.opacity);
}

//instanced over all dots, then vertex id is id within the current dot
//sort of dot
vertex rasterized_dot dot_vert_shader(uint vertex_id [[vertex_id]], uint instance_id [[instance_id]], const device dot_vert_in *vertices [[buffer(0)]], constant dot_vert_uniform &uniform [[buffer(1)]]) {
    
    dot_vert_in const in = vertices[instance_id];
    
    rasterized_dot out{};
    
    /* a vector not parallel to to in.norm */
    float3 const helper = in.norm + float3(1, abs(in.norm.y) < FLT_EPSILON, abs(in.norm.z) < FLT_EPSILON);
    float3 const i_hat = normalize(cross(helper, in.norm));
    float3 const j_hat = normalize(cross(i_hat, in.norm));
    
    float4 const raw_model = uniform.mv * float4(in.pos, 1);

    
    /* since [0,1,0] maps to the top of the screen */
    float const scale = 2 * uniform.radius * raw_model.z * uniform.p.columns[2].w / uniform.viewport_size.x;
    float const model_space_r = scale;
    
    float const t = vertex_id * 2 * M_PI_F / uniform.vertex_count;
    float3 const c = cos(t) * model_space_r * i_hat;
    float3 const s = sin(t) * model_space_r * j_hat;
    
    float4 const model = uniform.mv * float4(in.pos + c + s, 1);
    float4 const in_pos = uniform.p * model;
    
    out.col = half4(in.col);
    out.pos = in_pos;
    out.pos.z -= uniform.z_offset;
    
    return out;
}

fragment half4 dot_frag_shader(rasterized_tri interpolated [[stage_in]], constant dot_frag_uniform &uniform [[buffer(0)]]) {
    half4 const col = interpolated.col;
    return half4(col.xyz, col.w * uniform.opacity);
}
