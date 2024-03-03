//
//  geo.h
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#pragma once

#include <float.h>
#include <limits.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

#include "mc_env.h"
#include "mc_types.h"

// look into vectorization at some point
// but that's a bit hard if we're targetting arm...
struct vec2 {
    float x, y;
};

struct vec3 {
    float x, y, z;
};

struct vec4 {
    float x, y, z, w;
};

struct plane3 {
    struct vec3 a, b;
};

// column major...
struct mat4x4 {
    struct vec4 columns[4];
};

#define MAT_I4                                                                 \
    (struct mat4x4)                                                            \
    {                                                                          \
        {                                                                      \
            { 1, 0, 0, 0 }, { 0, 1, 0, 0 }, { 0, 0, 1, 0 }, { 0, 0, 0, 1 }     \
        }                                                                      \
    }

#define VEC3_0                                                                 \
    (struct vec3) { 0 }
#define VEC4_0                                                                 \
    (struct vec4) { 0 }
#define VEC4_1                                                                 \
    (struct vec4) { 1, 1, 1, 1 }

#if MC_INTERNAL

#if MC_ENV_OS & MC_ENV_OS_POSIX
#define MC_PI M_PI
#else
#define MC_PI 3.14159265358979323846
#endif

/* seems like FLT_EPSILON is a bit too large for our needs */
#define GEOMETRIC_EPSILON 1e-5

mc_bool_t
vec3_equals(struct vec3 a, struct vec3 b);

mc_bool_t
vec4_equals(struct vec4 a, struct vec4 b);

float
vec3_manhattan(struct vec3 a, struct vec3 b);

inline float
vec3_dot(struct vec3 main, struct vec3 unit)
{
    return main.x * unit.x + main.y * unit.y + main.z * unit.z;
}

inline struct vec3
vec3_unit(struct vec3 a)
{
    float const norm = sqrtf(a.x * a.x + a.y * a.y + a.z * a.z);
    if (norm < FLT_EPSILON) {
        return a;
    }

    return (struct vec3){ a.x / norm, a.y / norm, a.z / norm };
}

inline struct vec3
vec3_proj_onto(struct vec3 main, struct vec3 onto)
{
    float const dot = main.x * onto.x + main.y * onto.y + main.z * onto.z;
    float const mag = onto.x * onto.x + onto.y * onto.y + onto.z * onto.z;
    if (mag < GEOMETRIC_EPSILON) {
        return VEC3_0;
    }

    return (struct vec3){ onto.x * dot / mag, onto.y * dot / mag,
                          onto.z * dot / mag };
}

inline struct vec3
vec3_cross(struct vec3 a, struct vec3 b)
{
    return (struct vec3){
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    };
}

inline struct vec2
vec2_lerp(struct vec2 a, float t, struct vec2 b)
{
    return (struct vec2){
        a.x * (1 - t) + t * b.x,
        a.y * (1 - t) + t * b.y,
    };
}

inline struct vec3
vec3_lerp(struct vec3 a, float t, struct vec3 b)
{
    return (struct vec3){
        a.x * (1 - t) + t * b.x,
        a.y * (1 - t) + t * b.y,
        a.z * (1 - t) + t * b.z,
    };
}

inline struct vec3
vec3_norm_lerp(struct vec3 a, float t, struct vec3 b)
{
    struct vec3 const raw = vec3_lerp(a, t, b);

    if (vec3_equals(raw, VEC3_0)) {
        return t < 0.5 ? a : b;
    }
    else {
        return vec3_unit(raw);
    }
}

struct vec3
vec3_patharc_lerp(struct vec3 a, float t, struct vec3 b, struct vec3 path_arc);

inline struct vec4
vec4_lerp(struct vec4 a, float t, struct vec4 b)
{
    return (struct vec4){
        a.x * (1 - t) + t * b.x,
        a.y * (1 - t) + t * b.y,
        a.z * (1 - t) + t * b.z,
        a.w * (1 - t) + t * b.w,
    };
}

inline struct vec3
vec3_add(struct vec3 a, struct vec3 b)
{
    return (struct vec3){ a.x + b.x, a.y + b.y, a.z + b.z };
}

inline struct vec3
vec3_avg(struct vec3 a, struct vec3 b)
{
    return (struct vec3){
        (a.x + b.x) / 2,
        (a.y + b.y) / 2,
        (a.z + b.z) / 2,
    };
}

inline struct vec3
vec3_sub(struct vec3 a, struct vec3 b)
{
    return (struct vec3){ a.x - b.x, a.y - b.y, a.z - b.z };
}

inline struct vec3
vec3_mul_scalar(float scalar, struct vec3 v)
{
    return (struct vec3){ scalar * v.x, scalar * v.y, scalar * v.z };
}

inline struct vec3
vec3_mul_vec3(struct vec3 v, struct vec3 v2)
{
    return (struct vec3){ v.x * v2.x, v.y * v2.y, v.z * v2.z };
}

inline float
vec3_norm(struct vec3 a)
{
    return sqrtf(a.x * a.x + a.y * a.y + a.z * a.z);
}

/* gives double zero for a degenerate normal..., otherwise both are
 * perpendicular */
inline struct plane3
vec3_plane_basis(struct vec3 normal)
{
    struct vec3 i_hat;
    if (fabsf(normal.x) > FLT_EPSILON) {
        i_hat = (struct vec3){ -normal.z, 0, normal.x };
    }
    else {
        i_hat = (struct vec3){ 0, normal.z, -normal.y };
    }

    i_hat = vec3_unit(i_hat);

    return (struct plane3){ vec3_unit(vec3_cross(i_hat, normal)), i_hat };
}

struct vec3
vec3_rotate_about_axis(struct vec3 vec, struct vec3 axis, float alpha);

inline struct vec3
triangle_cross_product(struct vec3 p, struct vec3 q, struct vec3 r)
{
    return vec3_cross(vec3_sub(q, p), vec3_sub(r, p));
}

inline float
triangle_area(struct vec3 p, struct vec3 q, struct vec3 r)
{
    float const a = vec3_norm(vec3_sub(q, p));
    float const b = vec3_norm(vec3_sub(r, q));
    float const c = vec3_norm(vec3_sub(p, r));
    float const s = (a + b + c) / 2;

    return sqrtf(s * (s - a) * (s - b) * (s - c));
}

#endif
