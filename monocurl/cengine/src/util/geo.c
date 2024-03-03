//
//  geo.c
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include "geo.h"

mc_bool_t
vec3_equals(struct vec3 a, struct vec3 b)
{
    return fabsf(a.x - b.x) < GEOMETRIC_EPSILON &&
           fabsf(a.y - b.y) < GEOMETRIC_EPSILON &&
           fabsf(a.z - b.z) < GEOMETRIC_EPSILON;
}

mc_bool_t
vec4_equals(struct vec4 a, struct vec4 b)
{
    return fabsf(a.x - b.x) < GEOMETRIC_EPSILON &&
           fabsf(a.y - b.y) < GEOMETRIC_EPSILON &&
           fabsf(a.z - b.z) < GEOMETRIC_EPSILON &&
           fabsf(a.w - b.w) < GEOMETRIC_EPSILON;
}

float
vec3_manhattan(struct vec3 a, struct vec3 b)
{
    return fabsf(a.x - b.x) + fabsf(a.y - b.y) + fabsf(a.z - b.z);
}

struct vec3
vec3_patharc_lerp(struct vec3 a, float t, struct vec3 b, struct vec3 path_arc)
{
    // if on same plane then it's trivial
    // if not, assume plane is xy
    // from there
    if (vec3_equals(path_arc, VEC3_0)) {
        return vec3_lerp(a, t, b);
    }

    struct vec3 const dist = vec3_sub(b, a);
    if (vec3_equals(dist, VEC3_0)) {
        return a;
    }

    struct vec3 const cross = vec3_cross(path_arc, dist);

    float const alpha = vec3_norm(path_arc);
    float const cross_norm = vec3_norm(cross);

    float const mag = vec3_norm(dist) / (2.0f * tanf(alpha / 2));

    struct vec3 const pivot = (struct vec3){
        (a.x + b.x) / 2 + cross.x * mag / cross_norm,
        (a.y + b.y) / 2 + cross.y * mag / cross_norm,
        (a.z + b.z) / 2 + cross.z * mag / cross_norm,
    };

    float const theta = t * alpha;
    float const radius = vec3_norm(vec3_sub(a, pivot));
    float const cos = cosf(theta) * radius;
    float const sin = sinf(theta) * radius;

    struct vec3 const a_prime = vec3_mul_scalar(1 / radius, vec3_sub(a, pivot));
    struct vec3 const a_prime_norm = vec3_unit(vec3_cross(path_arc, a_prime));

    return vec3_add(
        pivot,
        vec3_add(
            vec3_mul_scalar(cos, a_prime), vec3_mul_scalar(sin, a_prime_norm)
        )
    );
}

/* https://en.wikipedia.org/wiki/Rotation_matrix#:~:text=Rotation%20matrix-,from,-axis%20and%20angle
 */
struct vec3
vec3_rotate_about_axis(struct vec3 vec, struct vec3 axis, float alpha)
{
    float const x = axis.x;
    float const y = axis.y;
    float const z = axis.z;

    float const cos = cosf(alpha);
    float const sin = sinf(alpha);

    return (struct vec3){
        vec.x * (cos + x * x * (1 - cos)) +
            vec.y * (x * y * (1 - cos) - z * sin) +
            vec.z * (x * z * (1 - cos) + y * sin),
        vec.x * (y * x * (1 - cos) + z * sin) +
            vec.y * (cos + y * y * (1 - cos)) +
            vec.z * (y * z * (1 - cos) - x * sin),
        vec.x * (z * y * (1 - cos) - y * sin) +
            vec.y * (z * y * (1 - cos) + x * sin) +
            vec.z * (cos + z * z * (1 - cos)),
    };
}

extern inline float
vec3_dot(struct vec3 main, struct vec3 unit);

extern inline struct vec3
vec3_proj_onto(struct vec3 main, struct vec3 onto);

extern inline struct vec3
vec3_cross(struct vec3 a, struct vec3 b);

extern inline struct vec2
vec2_lerp(struct vec2 a, float t, struct vec2 b);

extern inline struct vec3
vec3_lerp(struct vec3 a, float t, struct vec3 b);

extern inline struct vec3
vec3_norm_lerp(struct vec3 a, float t, struct vec3 b);

extern inline struct vec4
vec4_lerp(struct vec4 a, float t, struct vec4 b);

extern inline struct vec3
vec3_add(struct vec3 a, struct vec3 b);

extern inline struct vec3
vec3_avg(struct vec3 a, struct vec3 b);

extern inline struct vec3
vec3_sub(struct vec3 a, struct vec3 b);

extern inline struct vec3
vec3_unit(struct vec3 a);

extern inline float
vec3_norm(struct vec3 a);

extern inline struct vec3
vec3_mul_scalar(float scalar, struct vec3 v);

extern inline struct vec3
vec3_mul_vec3(struct vec3 v, struct vec3 v2);

extern inline struct plane3
vec3_plane_basis(struct vec3 normal);

extern inline struct vec3
triangle_cross_product(struct vec3 p, struct vec3 q, struct vec3 r);

extern inline float
triangle_area(struct vec3 p, struct vec3 q, struct vec3 r);
