//
//  anim_transform.c
//  Monocurl
//
//  Created by Manu Bhat on 2/23/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <stdlib.h>
#include <string.h>

#include "anim_show.h"
#include "anim_transform.h"

#include "mesh_util.h"

#define TRANSFORM_PREFIX(null_action_ind)                                      \
    ANIM_TIME(t, 3);                                                           \
    if (owned_mesh_tree(executor, fields[1]) != MC_STATUS_SUCCESS) {           \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        return;                                                                \
    }                                                                          \
    LIBMC_FULL_CAST(null_action, null_action_ind, VECTOR_FIELD_TYPE_DOUBLE);   \
    struct mesh_tag_subset const prev = mesh_fullset(executor, fields[0]);     \
    struct mesh_tag_subset const curr = mesh_fullset(executor, fields[1]);     \
    struct mesh_tag_subset const targ = mesh_fullset(executor, fields[2]);     \
    if (timeline_mesh_hide(executor, fields[1]) != MC_STATUS_SUCCESS) {        \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        return;                                                                \
    }                                                                          \
    do {                                                                       \
    } while (0)

#define TRANSFORM_SUFFIX                                                       \
    if (timeline_mesh_show(executor, fields[1]) != MC_STATUS_SUCCESS) {        \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        return;                                                                \
    }                                                                          \
    mesh_subset_free(prev);                                                    \
    mesh_subset_free(curr);                                                    \
    mesh_subset_free(targ);                                                    \
    executor->return_register = double_init(executor, t >= 1);                 \
    do {                                                                       \
    } while (0)

// blips triangle, anti norm, and degen lines
// does not increment count
static void
_blip_tri(
    struct tetra_tri_vertex a, struct tetra_tri_vertex b,
    struct tetra_tri_vertex c, struct tetra_tri_vertex an,
    struct tetra_tri_vertex bn, struct tetra_tri_vertex cn,
    struct tetra_tri *tri_dump, int32_t out_tri_count,
    struct tetra_lin *lin_dump, int32_t out_lin_count
)
{
    struct tetra_tri const tri = {
        .a = a,
        .b = b,
        .c = c,
        .ab = -1 - out_lin_count,
        .bc = -2 - out_lin_count,
        .ca = -3 - out_lin_count,
        .antinorm = out_tri_count + 1,
        .is_dominant_sibling = 1,
    };

    struct tetra_tri const atri = {
        .a = an,
        .b = bn,
        .c = cn,
        .ab = -4 - out_lin_count,
        .bc = -5 - out_lin_count,
        .ca = -6 - out_lin_count,
        .antinorm = out_tri_count,
        .is_dominant_sibling = 0,
    };

    tri_dump[out_tri_count] = tri;
    tri_dump[out_tri_count + 1] = atri;

    // add degens (relatively straightforward)
    struct tetra_lin_vertex const aa = { .pos = a.pos, .col = { 0 } };
    struct tetra_lin_vertex const ab = { .pos = b.pos, .col = { 0 } };
    struct tetra_lin_vertex const ac = { .pos = c.pos, .col = { 0 } };
    struct tetra_lin_vertex const na = { .pos = an.pos, .col = { 0 } };
    struct tetra_lin_vertex const nb = { .pos = bn.pos, .col = { 0 } };
    struct tetra_lin_vertex const nc = { .pos = cn.pos, .col = { 0 } };

    struct vec3 const norm = a.norm;
    struct vec3 const anorm = an.norm;

    lin_dump[out_lin_count + 0] = (struct tetra_lin){
        .a = aa,
        .b = ab,
        .norm = norm,
        .prev = out_lin_count + 2,
        .next = out_lin_count + 1,
        .inverse = -1 - out_tri_count,
        .antinorm = out_lin_count + 3,
        .is_dominant_sibling = 1,
    };

    lin_dump[out_lin_count + 1] = (struct tetra_lin){
        .a = ab,
        .b = ac,
        .norm = norm,
        .prev = out_lin_count,
        .next = out_lin_count + 2,
        .inverse = -1 - out_tri_count,
        .antinorm = out_lin_count + 5,
        .is_dominant_sibling = 1,
    };

    lin_dump[out_lin_count + 2] = (struct tetra_lin){
        .a = ac,
        .b = aa,
        .norm = norm,
        .prev = out_lin_count + 1,
        .next = out_lin_count,
        .inverse = -1 - out_tri_count,
        .antinorm = out_lin_count + 4,
        .is_dominant_sibling = 1,
    };

    lin_dump[out_lin_count + 3] = (struct tetra_lin){
        .a = na,
        .b = nb,
        .norm = anorm,
        .prev = out_lin_count + 5,
        .next = out_lin_count + 4,
        .inverse = -2 - out_tri_count,
        .antinorm = out_lin_count,
        .is_dominant_sibling = 1,
    };

    lin_dump[out_lin_count + 4] = (struct tetra_lin){
        .a = nb,
        .b = nc,
        .norm = anorm,
        .prev = out_lin_count + 3,
        .next = out_lin_count + 5,
        .inverse = -2 - out_tri_count,
        .antinorm = out_lin_count + 2,
        .is_dominant_sibling = 1,
    };

    lin_dump[out_lin_count + 5] = (struct tetra_lin){
        .a = nc,
        .b = na,
        .norm = anorm,
        .prev = out_lin_count + 4,
        .next = out_lin_count + 3,
        .inverse = -2 - out_tri_count,
        .antinorm = out_lin_count + 1,
        .is_dominant_sibling = 1,
    };
}

// elides all four lines, and joins associated triangles
static void
_tri_tri_join(
    struct tetra_tri *tris, struct tetra_lin *lins, int32_t lin_a,
    int32_t lin_b, mc_count_t *lin_ref_count
)
{
    struct tetra_lin const a = lins[lin_a];
    struct tetra_lin const b = lins[lin_b];

    int32_t const lin_an = a.antinorm;
    int32_t const lin_bn = b.antinorm;

    struct tetra_lin const an = lins[lin_an];
    struct tetra_lin const bn = lins[lin_bn];

    int32_t const tri_a = -1 - a.inverse;
    int32_t const tri_b = -1 - b.inverse;

    int32_t const tri_an = -1 - an.inverse;
    int32_t const tri_bn = -1 - bn.inverse;

    // elide lines
    lins[a.prev].next = b.next;
    lins[b.next].prev = a.prev;

    lins[a.next].prev = b.prev;
    lins[b.prev].next = a.next;

    lins[an.prev].next = bn.next;
    lins[bn.next].prev = an.prev;

    lins[an.next].prev = bn.prev;
    lins[bn.prev].next = an.next;

    // join triangles
    tetramesh_tri_set_edge(
        &tris[tri_a], tetramesh_tri_edge_for(&tris[tri_a], -1 - lin_a), tri_b
    );

    tetramesh_tri_set_edge(
        &tris[tri_b], tetramesh_tri_edge_for(&tris[tri_b], -1 - lin_b), tri_a
    );

    tetramesh_tri_set_edge(
        &tris[tri_an], tetramesh_tri_edge_for(&tris[tri_an], -1 - lin_an),
        tri_bn
    );

    tetramesh_tri_set_edge(
        &tris[tri_bn], tetramesh_tri_edge_for(&tris[tri_bn], -1 - lin_bn),
        tri_an
    );

    lin_ref_count[lin_a] = 0;
    lin_ref_count[lin_an] = 0;
    lin_ref_count[lin_b] = 0;
    lin_ref_count[lin_bn] = 0;
}

static void
_find_pivot(
    struct tetramesh const *a, struct tetramesh const *b, int32_t *a_pivot,
    int32_t *b_pivot
)
{
    // find the direction that a - b is towardsj
    // find the triangles on each one
    struct vec3 com_a = { 0 };
    struct vec3 com_b = { 0 };

    for (mc_ind_t i = 0; i < a->tri_count; ++i) {
        com_a.x += a->tris[i].a.pos.x + a->tris[i].b.pos.x + a->tris[i].c.pos.x;
        com_a.y += a->tris[i].a.pos.y + a->tris[i].b.pos.y + a->tris[i].c.pos.y;
        com_a.z += a->tris[i].a.pos.z + a->tris[i].b.pos.z + a->tris[i].c.pos.z;
    }

    for (mc_ind_t i = 0; i < b->tri_count; ++i) {
        com_b.x += b->tris[i].a.pos.x + b->tris[i].b.pos.x + b->tris[i].c.pos.x;
        com_b.y += b->tris[i].a.pos.y + b->tris[i].b.pos.y + b->tris[i].c.pos.y;
        com_b.z += b->tris[i].a.pos.z + b->tris[i].b.pos.z + b->tris[i].c.pos.z;
    }

    com_a.x /= a->tri_count;
    com_a.y /= a->tri_count;
    com_a.z /= a->tri_count;
    com_b.x /= b->tri_count;
    com_b.y /= b->tri_count;
    com_b.z /= b->tri_count;

    int32_t _a_pivot = 0, _b_pivot = 0;
    double best_dot = DBL_MAX;

    for (mc_ind_t i = 0; i < a->tri_count; ++i) {
        if (!a->tris[i].is_dominant_sibling) {
            continue;
        }

        struct vec3 const avg = {
            a->tris[i].a.pos.x + a->tris[i].b.pos.x + a->tris[i].c.pos.x,
            a->tris[i].a.pos.y + a->tris[i].b.pos.y + a->tris[i].c.pos.y,
            a->tris[i].a.pos.z + a->tris[i].b.pos.z + a->tris[i].c.pos.z,
        };

        double const factor = vec3_norm(vec3_sub(avg, com_a));

        if (factor < best_dot) {
            best_dot = factor;
            _a_pivot = (int32_t) i;
        }
    }

    best_dot = DBL_MAX;

    for (mc_ind_t i = 0; i < b->tri_count; ++i) {
        if (!b->tris[i].is_dominant_sibling) {
            continue;
        }

        struct vec3 const avg = {
            b->tris[i].a.pos.x + b->tris[i].b.pos.x + b->tris[i].c.pos.x,
            b->tris[i].a.pos.y + b->tris[i].b.pos.y + b->tris[i].c.pos.y,
            b->tris[i].a.pos.z + b->tris[i].b.pos.z + b->tris[i].c.pos.z,
        };

        double const factor = vec3_norm(vec3_sub(avg, com_b));

        if (factor < best_dot) {
            best_dot = factor;
            _b_pivot = (int32_t) i;
        }
    }

    *a_pivot = _a_pivot;
    *b_pivot = _b_pivot;
}

static inline void
_rotate_triangle(struct tetra_tri_vertex *arr, int rot)
{
    if (rot == 0) {
        return;
    }
    else if (rot == 1) {
        struct tetra_tri_vertex tmp = arr[2];
        arr[2] = arr[1];
        arr[1] = arr[0];
        arr[0] = tmp;
    }
    else {
        assert(rot == 2);
        struct tetra_tri_vertex tmp = arr[0];
        arr[0] = arr[1];
        arr[1] = arr[2];
        arr[2] = tmp;
    }
}

static inline int
_b_offset(struct tetra_tri a, struct tetra_tri b)
{
    // which edge rotates best
    int best_edge = 0;
    double best_dist = DBL_MAX;

    struct tetra_tri_vertex verts[3];
    verts[0] = b.a;
    verts[1] = b.b;
    verts[2] = b.c;

    for (int r = 0; r < 3; ++r) {
        double dist = 0;

        dist += vec3_norm(vec3_sub(a.a.pos, verts[0].pos));
        dist += vec3_norm(vec3_sub(a.b.pos, verts[1].pos));
        dist += vec3_norm(vec3_sub(a.c.pos, verts[2].pos));

        if (dist < best_dist) {
            best_edge = r;
            best_dist = dist;
        }

        if (r < 2) {
            _rotate_triangle(verts, 2);
        }
    }

    return best_edge;
}

static void
match_tri_tri(
    struct tetramesh *tri_0, struct tetramesh *dmp, struct tetramesh *tri_1
)
{
    // tessellate
    // may experiment with this early
    if (tri_0->tri_count > tri_1->tri_count) {
        //        tetramesh_tesselate(tri_1, tri_0->tri_count);
    }
    else if (tri_1->tri_count > tri_0->tri_count) {
        //        tetramesh_tesselate(tri_0, tri_1->tri_count);
    }

    // match
    mc_bool_t *a_pushed = mc_calloc(tri_0->tri_count, sizeof(mc_bool_t));
    mc_bool_t *b_pushed = mc_calloc(tri_1->tri_count, sizeof(mc_bool_t));
    mc_ind_t *a_src = mc_malloc(tri_0->tri_count * sizeof(mc_ind_t));
    mc_ind_t *b_src = mc_malloc(tri_1->tri_count * sizeof(mc_ind_t));

    // size_max
    memset(a_src, -1, tri_0->tri_count * sizeof(mc_ind_t));
    memset(b_src, -1, tri_1->tri_count * sizeof(mc_ind_t));

    mc_count_t tri_count = 0;
    mc_count_t lin_count = 0;

    struct tetra_tri *a_tris = NULL;
    struct tetra_lin *a_lins = NULL;

    struct tetra_tri *b_tris = NULL;
    struct tetra_lin *b_lins = NULL;

    // for remapping after line elision is complete
    mc_count_t *lin_ref_count = NULL;
    int32_t *lin_ind_map = NULL;

    mc_ind_t read = 0, write = 0;

    struct _tri_lerp_q_entry {
        int32_t input_ind; // triangle or line that spawned this node

        // output index of the line, only applicable for lines
        // otherwise, set to negative 1
        mc_ind_t lin_output_ind;
        float lin_start;
        float lin_end;

        int32_t input_parent;   // -1 for initial
        mc_ind_t output_parent; // really only useful for degen scenarios
        int parent_out_edge;
    } *aq = NULL, *bq = NULL;

    // find and push pivots (guaranteed to exist)
    MC_MEM_RESERVE(aq, write);
    MC_MEM_RESERVE(bq, write);

    int32_t a_pivot;
    int32_t b_pivot;

    _find_pivot(tri_0, tri_1, &a_pivot, &b_pivot);

    // should make this a deque at some point
    *aq = (struct _tri_lerp_q_entry){
        .input_ind = a_pivot,
        .lin_output_ind = SIZE_MAX,
        .input_parent = INT32_MIN,
    };

    *bq = (struct _tri_lerp_q_entry){
        .input_ind = b_pivot,
        .lin_output_ind = SIZE_MAX,
        .input_parent = INT32_MIN,
    };

    a_pushed[a_pivot] = 1;
    b_pushed[b_pivot] = 1;

    write++;

    // dual rotating bfs
    while (read < write) {
        // poll
        struct _tri_lerp_q_entry const a = aq[read];
        struct _tri_lerp_q_entry const b = bq[read];

        // two lines
        if (a.lin_output_ind != SIZE_MAX && b.lin_output_ind != SIZE_MAX) {
            // a true line
            if (a.input_ind < 0) {
                float const t = a.lin_start;
                float const s = a.lin_end;

                struct tetra_lin *const src_c = &tri_0->lins[-1 - a.input_ind];
                struct tetra_lin *const src_n = &tri_0->lins[src_c->antinorm];
                struct tetra_lin *const curr = &a_lins[a.lin_output_ind];
                struct tetra_lin *const anti = &a_lins[curr->antinorm];

                curr->a.col = vec4_lerp(src_c->a.col, t, src_c->b.col);
                curr->b.col = vec4_lerp(src_c->a.col, s, src_c->b.col);

                anti->a.col = vec4_lerp(src_n->a.col, t, src_n->b.col);
                anti->b.col = vec4_lerp(src_n->a.col, s, src_n->b.col);
            }

            // b true line
            if (b.input_ind < 0) {
                float const t = b.lin_start;
                float const s = b.lin_end;

                struct tetra_lin *const src_c = &tri_1->lins[-1 - b.input_ind];
                struct tetra_lin *const src_n = &tri_1->lins[src_c->antinorm];
                struct tetra_lin *const curr = &b_lins[b.lin_output_ind];
                struct tetra_lin *const anti = &b_lins[curr->antinorm];

                curr->a.col = vec4_lerp(src_c->a.col, t, src_c->b.col);
                curr->b.col = vec4_lerp(src_c->a.col, s, src_c->b.col);

                anti->a.col = vec4_lerp(src_n->a.col, t, src_n->b.col);
                anti->b.col = vec4_lerp(src_n->a.col, s, src_n->b.col);
            }
        }
        // one triangle and one line
        else if (a.lin_output_ind != SIZE_MAX || b.lin_output_ind != SIZE_MAX) {
            float pivot;

            // mem reserve
            MC_MEM_RESERVEN(lin_ref_count, lin_count, 6);
            MC_MEM_RESERVEN(a_lins, lin_count, 6);
            MC_MEM_RESERVEN(b_lins, lin_count, 6);

            MC_MEM_RESERVEN(a_tris, tri_count, 2);
            MC_MEM_RESERVEN(b_tris, tri_count, 2);

            // for sprawling
            MC_MEM_RESERVEN(aq, write, 2);
            MC_MEM_RESERVEN(bq, write, 2);

            for (mc_ind_t k = 0; k < 6; ++k) {
                lin_ref_count[lin_count + k] = 1;
            }

            struct tetra_tri triangle, ntriangle, *main_tris, *lin_tris;
            struct tetra_lin *main_lins, *lin_lins;
            struct _tri_lerp_q_entry *lin_q, *main_q;
            mc_bool_t *push;
            int up_edge;
            mc_bool_t left_exists, right_exists;
            struct tetra_tri_vertex lin_verts[3], lin_nverts[3];

            if (a.lin_output_ind != SIZE_MAX) {
                // a is line, b is triangle
                triangle = tri_1->tris[b.input_ind];
                ntriangle = tri_1->tris[triangle.antinorm];

                main_tris = b_tris;
                main_lins = b_lins;
                main_q = bq;

                lin_tris = a_tris;
                lin_lins = a_lins;
                lin_q = aq;

                push = b_pushed;

                up_edge = tetramesh_tri_edge_for(&triangle, b.input_parent);

                tetramesh_tri_read_edge(
                    &a_tris[a.output_parent], a.parent_out_edge, lin_verts
                );
                tetramesh_tri_read_edge(
                    &a_tris[a_tris[a.output_parent].antinorm],
                    (3 - a.parent_out_edge) % 3, lin_nverts
                );

                // mark triangle as visited
                b_src[b.input_ind] = tri_count;
                b_src[tri_1->tris[b.input_ind].antinorm] = tri_count + 1;
            }
            else {
                // a is triangle, b is line
                triangle = tri_0->tris[a.input_ind];
                ntriangle = tri_0->tris[triangle.antinorm];

                main_tris = a_tris;
                main_lins = a_lins;
                main_q = aq;

                lin_tris = b_tris;
                lin_lins = b_lins;
                lin_q = bq;

                push = a_pushed;

                up_edge = tetramesh_tri_edge_for(&triangle, a.input_parent);

                tetramesh_tri_read_edge(
                    &b_tris[b.output_parent], b.parent_out_edge, lin_verts
                );
                tetramesh_tri_read_edge(
                    &b_tris[b_tris[b.output_parent].antinorm],
                    (3 - b.parent_out_edge) % 3, lin_nverts
                );

                // mark triangle as visited
                a_src[a.input_ind] = tri_count;
                a_src[tri_0->tris[a.input_ind].antinorm] = tri_count + 1;
            }

            int32_t const left_ind =
                tetramesh_tri_edge(&triangle, (up_edge + 2) % 3);
            int32_t const right_ind =
                tetramesh_tri_edge(&triangle, (up_edge + 1) % 3);

            // need to ensure that left exists case we're doing what we want
            left_exists = left_ind >= 0 && push[left_ind];
            right_exists = right_ind >= 0 && push[right_ind];

            _blip_tri(
                triangle.a, triangle.b, triangle.c, ntriangle.a, ntriangle.b,
                ntriangle.c, main_tris, (int32_t) tri_count, main_lins,
                (int32_t) lin_count
            );

            //            if (left_exists == right_exists) {
            //                pivot = 0.5;
            //            }
            //            else if (left_exists) {
            //                pivot = 1;
            //            }
            //            else {
            //                pivot = 0;
            //            }

            pivot = 0.5;

            // create third
            lin_verts[2] =
                tetramesh_tri_vertex_lerp(lin_verts[0], lin_verts[1], pivot);
            lin_nverts[2] = tetramesh_tri_vertex_lerp(
                lin_nverts[0], lin_nverts[1], 1 - pivot
            );
            //            lin_verts[0].pos.x = 0.0 / 0;
            //            lin_verts[1].pos.x = 0.0 / 0;
            //            lin_verts[2].pos.x = 0.0 / 0;
            //            lin_nverts[0].pos.x = 0.0 / 0;
            //            lin_nverts[1].pos.x = 0.0 / 0;
            //            lin_nverts[2].pos.x = 0.0 / 0;

            _rotate_triangle(lin_nverts, (3 - up_edge) % 3);
            _rotate_triangle(lin_verts, up_edge);

            _blip_tri(
                lin_verts[0], lin_verts[1], lin_verts[2], lin_nverts[0],
                lin_nverts[1], lin_nverts[2], lin_tris, (int32_t) tri_count,
                lin_lins, (int32_t) lin_count
            );

            // back join JUST with parent (lins are same for both by definition)
            int32_t const out_lin =
                -1 -
                tetramesh_tri_edge(&a_tris[a.output_parent], a.parent_out_edge);
            int32_t const in_lin =
                -1 - tetramesh_tri_edge(&a_tris[tri_count], up_edge);
            _tri_tri_join(a_tris, a_lins, out_lin, in_lin, lin_ref_count);
            _tri_tri_join(b_tris, b_lins, out_lin, in_lin, lin_ref_count);

            struct _tri_lerp_q_entry lq = lin_q[read];
            struct _tri_lerp_q_entry mq = main_q[read];

            // sprawl left
            int const left_edge = (up_edge + 2) % 3;
            main_q[write] = (struct _tri_lerp_q_entry){
                .input_ind = left_exists ? mq.input_ind : left_ind,

                .lin_output_ind = left_ind >= 0 && !push[left_ind]
                                      ? SIZE_MAX
                                      : lin_count + (mc_count_t) left_edge,
                .lin_start = 0,
                .lin_end = 1,

                .input_parent = mq.input_ind,
                .output_parent = tri_count,
                .parent_out_edge = left_edge,
            };

            lin_q[write] = (struct _tri_lerp_q_entry){
                .input_ind = lq.input_ind,

                .lin_output_ind = lin_count + (mc_count_t) left_edge,
                .lin_start = (lq.lin_end - lq.lin_start) * pivot + lq.lin_start,
                .lin_end = lq.lin_end,

                .output_parent = tri_count,
                .parent_out_edge = left_edge,
            };

            write++;

            // sprawl right
            int const right_edge = (up_edge + 1) % 3;
            main_q[write] = (struct _tri_lerp_q_entry){
                .input_ind = right_exists ? mq.input_ind : right_ind,

                .lin_output_ind = right_ind >= 0 && !push[right_ind]
                                      ? SIZE_MAX
                                      : lin_count + (mc_count_t) right_edge,
                .lin_start = 0,
                .lin_end = 1,

                .input_parent = mq.input_ind,
                .output_parent = tri_count,
                .parent_out_edge = right_edge,
            };

            lin_q[write] = (struct _tri_lerp_q_entry){
                .input_ind = lq.input_ind,

                .lin_output_ind = lin_count + (mc_count_t) right_edge,
                .lin_start = lq.lin_start,
                .lin_end = (lq.lin_end - lq.lin_start) * pivot + lq.lin_start,

                .output_parent = tri_count,
                .parent_out_edge = right_edge,
            };

            // mark as pushed
            if (left_ind >= 0) {
                push[left_ind] = 1;
            }

            if (right_ind >= 0) {
                push[right_ind] = 1;
            }

            write++;

            tri_count += 2;
            lin_count += 6;
        }
        // two triangles
        else {
            // mem reserve
            MC_MEM_RESERVEN(lin_ref_count, lin_count, 6);
            MC_MEM_RESERVEN(a_lins, lin_count, 6);
            MC_MEM_RESERVEN(b_lins, lin_count, 6);

            MC_MEM_RESERVEN(a_tris, tri_count, 2);
            MC_MEM_RESERVEN(b_tris, tri_count, 2);

            for (mc_ind_t k = 0; k < 6; ++k) {
                lin_ref_count[lin_count + k] = 1;
            }

            struct tetra_tri a_triangle = tri_0->tris[a.input_ind];
            struct tetra_tri an_triangle = tri_0->tris[a_triangle.antinorm];

            struct tetra_tri b_triangle = tri_1->tris[b.input_ind];
            struct tetra_tri bn_triangle = tri_1->tris[b_triangle.antinorm];

            _blip_tri(
                a_triangle.a, a_triangle.b, a_triangle.c, an_triangle.a,
                an_triangle.b, an_triangle.c, a_tris, (int32_t) tri_count,
                a_lins, (int32_t) lin_count
            );

            struct tetra_tri_vertex b_verts[3], b_nverts[3];
            b_verts[0] = b_triangle.a;
            b_verts[1] = b_triangle.b;
            b_verts[2] = b_triangle.c;

            b_nverts[0] = bn_triangle.a;
            b_nverts[1] = bn_triangle.b;
            b_nverts[2] = bn_triangle.c;

            int const aup_edge =
                tetramesh_tri_edge_for(&a_triangle, a.input_parent);
            int const bup_edge =
                tetramesh_tri_edge_for(&b_triangle, b.input_parent);

            // sprawl all three matching directions, if its the first time,
            // match rotations
            int b_offset;
            if (aup_edge == -1) {
                b_offset = _b_offset(a_triangle, b_triangle);
            }
            else {
                b_offset = (3 + bup_edge - aup_edge) % 3;
            }

            _rotate_triangle(b_verts, (3 - b_offset) % 3);
            _rotate_triangle(b_nverts, b_offset);

            _blip_tri(
                b_verts[0], b_verts[1], b_verts[2], b_nverts[0], b_nverts[1],
                b_nverts[2], b_tris, (int32_t) tri_count, b_lins,
                (int32_t) lin_count
            );

            for (int r = 0; r < 3; ++r) {
                int32_t const a_nbr = tetramesh_tri_edge(&a_triangle, r);
                int32_t const b_nbr =
                    tetramesh_tri_edge(&b_triangle, (r + b_offset) % 3);

                mc_bool_t const sprawl_a = a_nbr < 0 || !a_pushed[a_nbr];
                mc_bool_t const sprawl_b = b_nbr < 0 || !b_pushed[b_nbr];

                // if both not pushed, push
                if (sprawl_a && sprawl_b) {
                    MC_MEM_RESERVE(aq, write);
                    MC_MEM_RESERVE(bq, write);
                    aq[write] = (struct _tri_lerp_q_entry){
                        .input_ind = a_nbr,
                        .lin_output_ind =
                            a_nbr < 0 ? lin_count + (mc_count_t) r : SIZE_MAX,
                        .lin_start = 0,
                        .lin_end = 1,
                        .input_parent = a.input_ind,
                        .output_parent = tri_count,
                        .parent_out_edge = r,
                    };
                    bq[write] = (struct _tri_lerp_q_entry){
                        .input_ind = b_nbr,
                        .lin_output_ind =
                            b_nbr < 0 ? lin_count + (mc_count_t) r : SIZE_MAX,
                        .lin_start = 0,
                        .lin_end = 1,
                        .input_parent = b.input_ind,
                        .output_parent = tri_count,
                        .parent_out_edge = r,
                    };
                    write++;

                    if (a_nbr >= 0) {
                        a_pushed[a_nbr] = 1;
                    }
                    if (b_nbr >= 0) {
                        b_pushed[b_nbr] = 1;
                    }
                }
                else if (sprawl_a) {
                    MC_MEM_RESERVE(aq, write);
                    MC_MEM_RESERVE(bq, write);
                    // push using current
                    aq[write] = (struct _tri_lerp_q_entry){
                        .input_ind = a_nbr,
                        .lin_output_ind =
                            a_nbr < 0 ? lin_count + (mc_count_t) r : SIZE_MAX,
                        .lin_start = 0,
                        .lin_end = 1,
                        .input_parent = a.input_ind,
                        .output_parent = tri_count,
                        .parent_out_edge = r,
                    };
                    bq[write] = (struct _tri_lerp_q_entry){
                        .input_ind = b.input_ind,
                        .lin_output_ind = lin_count + (mc_count_t) r,
                        .lin_start = 0,
                        .lin_end = 1,
                        .input_parent = b.input_ind,
                        .output_parent = tri_count,
                        .parent_out_edge = r,
                    };
                    write++;

                    if (a_nbr >= 0) {
                        a_pushed[a_nbr] = 1;
                    }
                }
                else if (sprawl_b) {
                    MC_MEM_RESERVE(aq, write);
                    MC_MEM_RESERVE(bq, write);
                    // push using current
                    aq[write] = (struct _tri_lerp_q_entry){
                        .input_ind = a.input_ind,
                        .lin_output_ind = lin_count + (mc_count_t) r,
                        .lin_start = 0,
                        .lin_end = 1,
                        .input_parent = a.input_ind,
                        .output_parent = tri_count,
                        .parent_out_edge = r,
                    };
                    bq[write] = (struct _tri_lerp_q_entry){
                        .input_ind = b_nbr,
                        .lin_output_ind =
                            b_nbr < 0 ? lin_count + (mc_count_t) r : SIZE_MAX,
                        .lin_start = 0,
                        .lin_end = 1,
                        .input_parent = b.input_ind,
                        .output_parent = tri_count,
                        .parent_out_edge = r,
                    };
                    write++;

                    if (b_nbr >= 0) {
                        b_pushed[b_nbr] = 1;
                    }
                }
                else if (a_src[a_nbr] == b_src[b_nbr] && a_src[a_nbr] != SIZE_MAX) {
                    // otherwise if both visited and the same, connect

                    int const a_prime_rotation = tetramesh_tri_edge_for(
                        &tri_0->tris[a_nbr], a.input_ind
                    );
                    // remark: "A" triangle will always match rotation
                    // the only time it won't is when it's in a degen scenario
                    // but that will never map to a_src
                    // so, rotation of the input corresponds to output
                    // allowing us to properly connect
                    // otherwise, mapping of rotations is needed
                    int32_t const alpha_lin = (int32_t) (lin_count) + r;
                    int32_t const beta_lin =
                        -1 - tetramesh_tri_edge(
                                 &a_tris[a_src[a_nbr]], a_prime_rotation
                             );

                    _tri_tri_join(
                        a_tris, a_lins, alpha_lin, beta_lin, lin_ref_count
                    );
                    _tri_tri_join(
                        b_tris, b_lins, alpha_lin, beta_lin, lin_ref_count
                    );
                }
            }

            a_src[a.input_ind] = tri_count;
            a_src[tri_0->tris[a.input_ind].antinorm] = tri_count + 1;
            b_src[b.input_ind] = tri_count;
            b_src[tri_1->tris[b.input_ind].antinorm] = tri_count + 1;

            tri_count += 2;
            lin_count += 6;
        }

        // finish read
        read++;
    }

    // line ellision
    lin_ind_map = mc_malloc(lin_count * sizeof(int32_t));
    mc_count_t offset = 0;
    for (mc_ind_t l = 0; l < lin_count; ++l) {
        if (lin_ref_count[l]) {
            lin_ind_map[l] = (int32_t) offset;
            a_lins[offset] = a_lins[l];
            b_lins[offset] = b_lins[l];
            offset++;
        }
    }

    for (mc_ind_t i = 0; i < tri_count; ++i) {
        if (a_tris[i].ab < 0) {
            a_tris[i].ab = -1 - lin_ind_map[-1 - a_tris[i].ab];
            b_tris[i].ab = -1 - lin_ind_map[-1 - b_tris[i].ab];
        }

        if (a_tris[i].bc < 0) {
            a_tris[i].bc = -1 - lin_ind_map[-1 - a_tris[i].bc];
            b_tris[i].bc = -1 - lin_ind_map[-1 - b_tris[i].bc];
        }

        if (a_tris[i].ca < 0) {
            a_tris[i].ca = -1 - lin_ind_map[-1 - a_tris[i].ca];
            b_tris[i].ca = -1 - lin_ind_map[-1 - b_tris[i].ca];
        }
    }

    for (mc_ind_t j = 0; j < offset; ++j) {
        a_lins[j].antinorm = lin_ind_map[a_lins[j].antinorm];
        b_lins[j].antinorm = lin_ind_map[b_lins[j].antinorm];

        a_lins[j].next = lin_ind_map[a_lins[j].next];
        b_lins[j].next = lin_ind_map[b_lins[j].next];

        a_lins[j].prev = lin_ind_map[a_lins[j].prev];
        b_lins[j].prev = lin_ind_map[b_lins[j].prev];
    }

    // dispatch tris and lines
    mc_free(tri_0->tris);
    mc_free(tri_0->lins);
    tri_0->tris = a_tris;
    tri_0->lins = a_lins;

    dmp->tris = mc_reallocf(dmp->tris, sizeof(struct tetra_tri) * tri_count);
    memcpy(dmp->tris, a_tris, sizeof(struct tetra_tri) * tri_count);
    dmp->lins = mc_reallocf(dmp->lins, sizeof(struct tetra_lin) * lin_count);
    memcpy(dmp->lins, a_lins, sizeof(struct tetra_lin) * lin_count);

    mc_free(tri_1->tris);
    mc_free(tri_1->lins);
    tri_1->tris = b_tris;
    tri_1->lins = b_lins;

    tri_0->tri_count = tri_1->tri_count = dmp->tri_count = tri_count;
    tri_0->lin_count = tri_1->lin_count = dmp->lin_count = offset;

    // free aux
    mc_free(lin_ind_map);
    mc_free(lin_ref_count);

    mc_free(aq);
    mc_free(bq);

    mc_free(a_pushed);
    mc_free(b_pushed);

    mc_free(a_src);
    mc_free(b_src);
}

static void
match_tri_lin(
    struct tetramesh *tri_0, struct tetramesh *dmp, struct tetramesh *lin_1
)
{
    struct tetramesh *next = tetramesh_raw_copy(lin_1);
    if (!next || tetramesh_uprank(next, 0) != MC_STATUS_SUCCESS ||
        !next->tri_count) {
        lin_1->tri_count = lin_1->lin_count;
        lin_1->tris = mc_reallocf(
            lin_1->tris, sizeof(struct tetra_tri) * lin_1->lin_count
        );

        int32_t j = 0;
        while (lin_1->lins[j].prev > 0) {
            j = lin_1->lins[j].prev;
        }
        if (!lin_1->lins[j].is_dominant_sibling) {
            j = lin_1->lins[j].inverse;
        }

        int32_t const k = j;
        mc_ind_t i = 0;
        int32_t inv = lin_1->lins[j].inverse, ant = lin_1->lins[j].antinorm,
                anv = lin_1->lins[ant].inverse;

        int32_t pt = -1 - (int32_t) (lin_1->lin_count - 4),
                pb = -1 - (int32_t) (lin_1->lin_count - 3);

        if (lin_1->lins[j].prev < 0) {
            lin_1->lin_count += 4;
            lin_1->lins = mc_reallocf(
                lin_1->lins, lin_1->lin_count * sizeof(struct tetra_lin)
            );
            lin_1->lins[lin_1->lin_count - 4] = (struct tetra_lin){
                .a = lin_1->lins[inv].b,
                .b = lin_1->lins[j].a,
                .norm = lin_1->lins[j].norm,
                .prev = inv,
                .next = j,
                .inverse = -1,
                .antinorm = (int32_t) lin_1->lin_count - 3,
                .is_dominant_sibling = 1,
            };
            lin_1->lins[lin_1->lin_count - 3] = (struct tetra_lin){
                .a = lin_1->lins[ant].b,
                .b = lin_1->lins[anv].a,
                .norm = lin_1->lins[ant].norm,
                .prev = ant,
                .next = anv,
                .inverse = -3,
                .antinorm = (int32_t) lin_1->lin_count - 4,
                .is_dominant_sibling = 1,
            };

            lin_1->lins[j].prev = (int32_t) lin_1->lin_count - 4;
            lin_1->lins[inv].next = (int32_t) lin_1->lin_count - 4;
            lin_1->lins[anv].prev = (int32_t) lin_1->lin_count - 3;
            lin_1->lins[ant].next = (int32_t) lin_1->lin_count - 3;
        }

        do {
            lin_1->tris[i + 0] =
                (struct tetra_tri){ .a = { .pos = lin_1->lins[j].a.pos,
                                           .norm = lin_1->lins[j].norm,
                                           .col = { 0 } },
                                    .b = { .pos = lin_1->lins[j].b.pos,
                                           .norm = lin_1->lins[j].norm,
                                           .col = { 0 } },
                                    .c = { .pos = lin_1->lins[j].b.pos,
                                           .norm = lin_1->lins[j].norm,
                                           .col = { 0 } },
                                    .ab = -1 - j,
                                    .bc = (int32_t) i + 5,
                                    .ca = (int32_t) i + 1,
                                    .antinorm = (int32_t) i + 2,
                                    .is_dominant_sibling = 1 };
            lin_1->tris[i + 1] =
                (struct tetra_tri){ .a = lin_1->tris[i].b,
                                    .b = lin_1->tris[i].a,
                                    .c = lin_1->tris[i].a,
                                    .ab = -1 - inv,
                                    .bc = pt,
                                    .ca = (int32_t) i,
                                    .antinorm = (int32_t) i + 3,
                                    .is_dominant_sibling = 1 };
            lin_1->tris[i + 2] = (struct tetra_tri){
                .a = lin_1->tris[i].b,
                .b = lin_1->tris[i].a,
                .c = lin_1->tris[i].c,
                .ab = -1 - ant,
                .bc = (int32_t) i + 3,
                .ca = (int32_t) i + 7,
                .antinorm = (int32_t) i,
                .is_dominant_sibling = 0,
            };
            lin_1->tris[i + 3] = (struct tetra_tri){
                .a = lin_1->tris[i + 1].b,
                .b = lin_1->tris[i + 1].a,
                .c = lin_1->tris[i + 1].c,
                .ab = -1 - anv,
                .bc = (int32_t) i + 2,
                .ca = pb,
                .antinorm = (int32_t) i + 1,
                .is_dominant_sibling = 0,
            };

            lin_1->lins[j].inverse = -1 - (int32_t) i;
            lin_1->lins[inv].inverse = -1 - (int32_t) (i + 1);
            lin_1->lins[ant].inverse = -1 - (int32_t) (i + 2);
            lin_1->lins[anv].antinorm = -1 - (int32_t) (i + 3);

            pt = (int32_t) i;
            pb = (int32_t) i + 2;

            // map all 4
            if (lin_1->lins[j].next < 0) {
                lin_1->tris[i].bc = -1 - (int32_t) (lin_1->lin_count - 2);
                lin_1->tris[i + 2].ca = -1 - (int32_t) (lin_1->lin_count - 1);

                // connect top
                lin_1->lins[lin_1->lin_count - 2] = (struct tetra_lin){
                    .a = lin_1->lins[j].b,
                    .b = lin_1->lins[inv].a,
                    .norm = lin_1->lins[j].norm,
                    .prev = j,
                    .next = inv,
                    .antinorm = (int32_t) lin_1->lin_count - 1,
                    .is_dominant_sibling = 1,
                };
                lin_1->lins[lin_1->lin_count - 1] = (struct tetra_lin){
                    .a = lin_1->lins[anv].b,
                    .b = lin_1->lins[ant].a,
                    .norm = lin_1->lins[anv].norm,
                    .prev = anv,
                    .next = inv,
                    .antinorm = (int32_t) lin_1->lin_count - 2,
                    .is_dominant_sibling = 1,
                };

                lin_1->lins[j].next = (int32_t) lin_1->lin_count - 2;
                lin_1->lins[inv].prev = (int32_t) lin_1->lin_count - 2;
                lin_1->lins[anv].next = (int32_t) lin_1->lin_count - 1;
                lin_1->lins[ant].prev = (int32_t) lin_1->lin_count - 1;

                break;
            }
            else {
                j = lin_1->lins[j].next;
                inv = lin_1->lins[inv].prev;
                ant = lin_1->lins[ant].prev;
                anv = lin_1->lins[anv].next;
            }

            i += 4;
        } while (j != k);
    }
    else {
        // copy directly
        lin_1->lins = mc_reallocf(
            lin_1->lins, sizeof(struct tetra_lin) * next->lin_count
        );
        memcpy(
            lin_1->lins, next->lins, sizeof(struct tetra_lin) * next->lin_count
        );
        lin_1->lin_count = next->lin_count;

        lin_1->tris = mc_reallocf(
            lin_1->tris, sizeof(struct tetra_tri) * next->tri_count
        );
        memcpy(
            lin_1->tris, next->tris, sizeof(struct tetra_tri) * next->tri_count
        );
        lin_1->tri_count = next->tri_count;

        for (mc_ind_t i = 0; i < lin_1->tri_count; ++i) {
            lin_1->tris[i].a.col = (struct vec4){ 0 };
            lin_1->tris[i].b.col = (struct vec4){ 0 };
            lin_1->tris[i].c.col = (struct vec4){ 0 };
        }
    }
    tetramesh_unref(next);

    lin_1->dot_count = 0;
    free(lin_1->dots);
    lin_1->dots = NULL;

    match_tri_tri(tri_0, dmp, lin_1);
}

void
copy_lin(struct tetra_lin *dst, struct tetra_lin const *src, float u, float v)
{
    dst->norm = src->norm;
    dst->a.pos = vec3_lerp(src->a.pos, u, src->b.pos);
    dst->a.col = vec4_lerp(src->a.col, u, src->b.col);
    dst->b.pos = vec3_lerp(src->a.pos, v, src->b.pos);
    dst->b.col = vec4_lerp(src->a.col, v, src->b.col);
}

#pragma message("TODO, reduce duplciated symmetries")
static void
match_lin_lin(
    struct tetramesh *lin_0, struct tetramesh *dmp, struct tetramesh *lin_1
)
{
    // how much we value keeping translations over rotating
    float const translation_bias = 20;

    // whichever one is larger, split up
    if (lin_0->lin_count < lin_1->lin_count) {
        struct tetramesh *const aux = lin_0;
        lin_0 = lin_1;
        lin_1 = aux;
    }

    // build lin 1
    struct tetra_lin *new_lin_1 =
        mc_malloc(lin_0->lin_count * sizeof(struct tetra_lin));
    memcpy(new_lin_1, lin_0->lins, lin_0->lin_count * sizeof(struct tetra_lin));

    int32_t it_0 = 0, it_1 = 0;

    if (!lin_0->lins[it_0].is_dominant_sibling) {
        it_0 = lin_0->lins[it_0].inverse;
    }

    if (!lin_1->lins[it_1].is_dominant_sibling) {
        it_1 = lin_1->lins[it_1].inverse;
    }

    if (vec3_dot(lin_1->lins[it_1].norm, lin_0->lins[it_0].norm) < 0) {
        it_1 = lin_1->lins[it_1].antinorm;
    }

    // cycle back
    if (lin_0->dot_count) {
        while (lin_0->lins[it_0].prev >= 0) {
            it_0 = lin_0->lins[it_0].prev;
        }
    }

    if (lin_1->dot_count) {
        while (lin_1->lins[it_1].prev >= 0) {
            it_1 = lin_1->lins[it_1].prev;
        }
    }

    if (lin_1->dot_count && !lin_0->dot_count) {
        int32_t mid_1 = it_1;
        for (mc_count_t j = 0; j < lin_1->lin_count / 8; ++j) {
            mid_1 = lin_1->lins[mid_1].next;
        }
        // match middle to middle, then work backward
        int32_t const head = it_0;
        int32_t best_mid_0 = it_0;
        double best_dist = DBL_MAX;
        struct vec3 const a = lin_1->lins[mid_1].a.pos;
        struct vec3 const b = lin_1->lins[mid_1].b.pos;
        struct vec3 const unit = vec3_unit(vec3_sub(a, b));
        struct vec3 const comp_point = vec3_avg(a, b);

        do {
            struct vec3 const ap = lin_0->lins[it_0].a.pos;
            struct vec3 const bp = lin_0->lins[it_0].b.pos;
            struct vec3 const cunit = vec3_unit(vec3_sub(ap, bp));
            struct vec3 const mid = vec3_avg(ap, bp);

            double const dist = (translation_bias - vec3_dot(unit, cunit)) *
                                vec3_norm(vec3_sub(mid, comp_point));
            if (dist < best_dist) {
                best_dist = dist;
                best_mid_0 = it_0;
            }
            it_0 = lin_0->lins[it_0].next;
        } while (it_0 != head);

        it_0 = best_mid_0;
        for (mc_count_t j = 0; j < lin_0->lin_count / 8; ++j) {
            it_0 = lin_0->lins[it_0].next;
        }
    }
    else if (!lin_1->dot_count) {
        struct vec3 a_com = tetramesh_com(lin_0);
        struct vec3 b_com = tetramesh_com(lin_1);
        struct vec3 const delta = vec3_sub(a_com, b_com);

        double best_dist = 0;
        int32_t best_mid_0 = it_0;
        for (mc_ind_t j = 0; j < lin_0->lin_count / 4; ++j) {
            struct vec3 const avg =
                vec3_avg(lin_0->lins[it_0].a.pos, lin_0->lins[it_0].b.pos);
            double const dist = vec3_dot(delta, vec3_sub(avg, b_com));
            if (dist > best_dist) {
                best_dist = dist;
                best_mid_0 = it_0;
            }
            it_0 = lin_0->lins[it_0].next;
        }
        it_0 = best_mid_0;

        // match middle to middle, then work backward
        int32_t const head = it_1;
        int32_t best_mid_1 = it_1;

        best_dist = DBL_MAX;
        struct vec3 const a = lin_0->lins[it_0].a.pos;
        struct vec3 const b = lin_0->lins[it_0].b.pos;
        struct vec3 const unit = vec3_unit(vec3_sub(a, b));
        struct vec3 const comp_point = vec3_avg(a, b);

        do {
            struct vec3 const ap = lin_1->lins[it_1].a.pos;
            struct vec3 const bp = lin_1->lins[it_1].b.pos;
            struct vec3 const cunit = vec3_unit(vec3_sub(ap, bp));
            struct vec3 const mid = vec3_avg(ap, bp);

            double const dist = (translation_bias - vec3_dot(unit, cunit)) *
                                vec3_norm(vec3_sub(mid, comp_point));
            if (dist < best_dist) {
                best_dist = dist;
                best_mid_1 = it_1;
            }
            it_1 = lin_1->lins[it_1].next;
        } while (it_1 != head);

        it_1 = best_mid_1;
    }

    int32_t head = it_0;

    mc_ind_t j = 0, q = 0;
    struct tetramesh const *const smaller = lin_1;

    // mmmmm loooks like we kind of screwed up
    // dots so that the invariants aren't perfect
    // but it turns out that having duplicate dots
    // match up is fine
    int32_t small_dot_a_mapped[4] = { -1, -1, -1, -1 };
    int32_t small_dot_b_mapped[4] = { -1, -1, -1, -1 };
    int32_t small_dot_a_ind[4] = { -1, -1, -1, -1 };
    int32_t small_dot_b_ind[4] = { -1, -1, -1, -1 };

    do {
        mc_ind_t const curr_start =
            lin_0->lin_count / 4 * q / (lin_1->lin_count / 4);
        mc_ind_t const curr_end =
            lin_0->lin_count / 4 * (q + 1) / (lin_1->lin_count / 4);

        float const s = (float) (j - curr_start) / (curr_end - curr_start);
        float const t = s + 1.0f / (curr_end - curr_start);

        struct tetra_lin *const dst = &new_lin_1[it_0];
        struct tetra_lin *const dst_inv = &new_lin_1[lin_0->lins[it_0].inverse];
        struct tetra_lin *const dst_anti =
            &new_lin_1[lin_0->lins[it_0].antinorm];
        struct tetra_lin *const dst_inv_anti =
            &new_lin_1[lin_0->lins[lin_0->lins[it_0].antinorm].inverse];

        struct tetra_lin *const src = &lin_1->lins[it_1];
        struct tetra_lin *const src_inv = &lin_1->lins[src->inverse];
        struct tetra_lin *const src_anti = &lin_1->lins[src->antinorm];
        struct tetra_lin *const src_inv_anti = &lin_1->lins[src_anti->inverse];

        copy_lin(dst, src, s, t);
        copy_lin(dst_inv, src_inv, 1 - t, 1 - s);
        copy_lin(dst_anti, src_anti, 1 - t, 1 - s);
        copy_lin(dst_inv_anti, src_inv_anti, s, t);

        if (src->prev < 0 && small_dot_a_ind[0] == -1) {
            small_dot_a_mapped[0] = it_0;
            small_dot_a_mapped[1] = lin_0->lins[it_0].inverse;
            small_dot_a_mapped[2] = lin_0->lins[it_0].antinorm;
            small_dot_a_mapped[3] =
                lin_0->lins[lin_0->lins[it_0].antinorm].inverse;
            small_dot_a_ind[0] = -1 - src->prev;
            small_dot_a_ind[1] = -1 - src_inv->next;
            small_dot_a_ind[2] = -1 - src_anti->next;
            small_dot_a_ind[3] = -1 - src_inv_anti->prev;
        }

        if (src->next < 0) {
            small_dot_b_mapped[0] = it_0;
            small_dot_b_mapped[1] = lin_0->lins[it_0].inverse;
            small_dot_b_mapped[2] = lin_0->lins[it_0].antinorm;
            small_dot_b_mapped[3] =
                lin_0->lins[lin_0->lins[it_0].antinorm].inverse;
            small_dot_b_ind[0] = -1 - src->next;
            small_dot_b_ind[1] = -1 - src_inv->prev;
            small_dot_b_ind[2] = -1 - src_anti->prev;
            small_dot_b_ind[3] = -1 - src_inv_anti->next;
        }

        it_0 = lin_0->lins[it_0].next;
        if (++j == curr_end) {
            it_1 = src->next;
            ++q;
        }
    } while (it_0 >= 0 && it_0 != head);

    mc_free(lin_1->lins);
    lin_1->lins = new_lin_1;
    lin_1->lin_count = lin_0->lin_count;

    // if dot count mismatching, split open one
    if (!!lin_0->dot_count != !!lin_1->dot_count) {
        if (!lin_0->dot_count) {
            struct tetramesh *const aux = lin_0;
            lin_0 = lin_1;
            lin_1 = aux;
        }

        if (smaller == lin_0) {
            // remap to
            for (mc_ind_t k = 0; k < 4; ++k) {
                int32_t dot = small_dot_a_ind[k];
                int32_t lin = small_dot_a_mapped[k];
                lin_0->dots[dot].inverse = -1 - lin;
                if (1 <= k && k <= 2) {
                    lin_0->lins[lin].next = -1 - dot;
                }
                else {
                    lin_0->lins[lin].prev = -1 - dot;
                }

                dot = small_dot_b_ind[k];
                lin = small_dot_b_mapped[k];
                lin_0->dots[dot].inverse = -1 - lin;
                if (1 <= k && k <= 2) {
                    lin_0->lins[lin].prev = -1 - dot;
                }
                else {
                    lin_0->lins[lin].next = -1 - dot;
                }
            }
        }

        lin_1->dot_count = lin_0->dot_count;
        lin_1->dots = mc_malloc(lin_1->dot_count * sizeof(struct tetra_dot));
        memcpy(
            lin_1->dots, lin_0->dots,
            lin_1->dot_count * sizeof(struct tetra_dot)
        );

        for (mc_ind_t i = 0; i < lin_1->lin_count; ++i) {
            if ((lin_1->lins[i].next = lin_0->lins[i].next) < 0) {
                lin_1->dots[-1 - lin_1->lins[i].next].col.w = 0;
                lin_1->dots[-1 - lin_1->lins[i].next].pos =
                    lin_1->lins[i].b.pos;
            }
            if ((lin_1->lins[i].prev = lin_0->lins[i].prev) < 0) {
                lin_1->dots[-1 - lin_1->lins[i].prev].col.w = 0;
                lin_1->dots[-1 - lin_1->lins[i].prev].pos =
                    lin_1->lins[i].a.pos;
            }
        }
    }

    // copy to dump
    dmp->dot_count = lin_0->dot_count;
    dmp->dots =
        mc_reallocf(dmp->dots, dmp->dot_count * sizeof(struct tetra_dot));
    memcpy(dmp->dots, lin_0->dots, dmp->dot_count * sizeof(struct tetra_dot));

    dmp->lin_count = lin_0->lin_count;
    dmp->lins =
        mc_reallocf(dmp->lins, dmp->lin_count * sizeof(struct tetra_lin));
    memcpy(dmp->lins, lin_0->lins, dmp->lin_count * sizeof(struct tetra_lin));
}

static void
match_tri_dot(
    struct tetramesh *tri_0, struct tetramesh *dmp, struct tetramesh *dot_1
)
{
    // free (just collapse into a single tri)
    struct vec3 const center = tetramesh_com(tri_0);

    tri_0->dot_count = dot_1->dot_count;
    tri_0->dots = mc_malloc(sizeof(struct tetra_dot) * dot_1->dot_count);
    memcpy(
        tri_0->dots, dot_1->dots, sizeof(struct tetra_dot) * dot_1->dot_count
    );

    for (mc_ind_t i = 0; i < dot_1->dot_count; ++i) {
        tri_0->dots[i].pos = center;
    }

    dot_1->tri_count = tri_0->tri_count;
    dot_1->tris = mc_malloc(sizeof(struct tetra_tri) * dot_1->tri_count);
    memcpy(
        dot_1->tris, tri_0->tris, sizeof(struct tetra_tri) * dot_1->tri_count
    );

    dot_1->lin_count = tri_0->lin_count;
    dot_1->lins = mc_malloc(sizeof(struct tetra_lin) * dot_1->lin_count);
    memcpy(
        dot_1->lins, tri_0->lins, sizeof(struct tetra_lin) * dot_1->lin_count
    );

    struct vec3 const collapse = dot_1->dots[0].pos;

    for (mc_ind_t i = 0; i < dot_1->tri_count; ++i) {
        dot_1->tris[i].a.pos = dot_1->tris[i].b.pos = dot_1->tris[i].c.pos =
            collapse;
    }

    for (mc_ind_t i = 0; i < dot_1->lin_count; ++i) {
        dot_1->lins[i].a.pos = dot_1->lins[i].b.pos = collapse;
    }

    dmp->dot_count = dot_1->dot_count;
    dmp->dots =
        mc_reallocf(dmp->dots, sizeof(struct tetra_dot) * dmp->dot_count);
    memcpy(dmp->dots, dot_1->dots, sizeof(struct tetra_dot) * dmp->dot_count);

    dmp->lin_count = dot_1->lin_count;
    dmp->lins =
        mc_reallocf(dmp->lins, sizeof(struct tetra_lin) * dmp->lin_count);
    memcpy(dmp->lins, dot_1->lins, sizeof(struct tetra_lin) * dmp->lin_count);

    dmp->tri_count = dot_1->tri_count;
    dmp->tris =
        mc_reallocf(dmp->tris, sizeof(struct tetra_tri) * dmp->tri_count);
    memcpy(dmp->tris, dot_1->tris, sizeof(struct tetra_tri) * dmp->tri_count);
}

static void
match_lin_dot(
    struct tetramesh *lin_0, struct tetramesh *dmp, struct tetramesh *dot_1
)
{
    dot_1->lin_count = lin_0->lin_count;
    dot_1->lins =
        _mc_reallocf(dot_1->lins, dot_1->lin_count * sizeof(struct tetra_lin));
    memcpy(
        dot_1->lins, lin_0->lins, dot_1->lin_count * sizeof(struct tetra_lin)
    );
    for (mc_ind_t i = 0; i < dot_1->lin_count; ++i) {
        dot_1->lins[i].a.pos = dot_1->dots[0].pos;
        dot_1->lins[i].b.pos = dot_1->dots[0].pos;
    }

    // insert lins
    if (lin_0->dot_count) {
        // offset the dots in the dot
        for (mc_ind_t i = 0; i < dot_1->dot_count; ++i) {
            dot_1->dots[i].inverse += (int32_t) lin_0->dot_count;
            dot_1->dots[i].antinorm += (int32_t) lin_0->dot_count;
        }

        dot_1->dot_count += lin_0->dot_count;
        dot_1->dots = mc_reallocf(
            dot_1->dots, dot_1->dot_count * sizeof(struct tetra_dot)
        );

        for (mc_ind_t i = 0; i < dot_1->dot_count - lin_0->dot_count; ++i) {
            if (i < lin_0->dot_count) {
                dot_1->dots[i] = lin_0->dots[i];
                dot_1->dots[i].pos = dot_1->dots[0].pos;
            }
            else {
                dot_1->dots[i] = dot_1->dots[i - lin_0->dot_count];
            }
        }
    }

    // append dots to line
    lin_0->dots =
        mc_reallocf(lin_0->dots, dot_1->dot_count * sizeof(struct tetra_dot));
    for (mc_ind_t i = lin_0->dot_count; i < dot_1->dot_count; ++i) {
        lin_0->dots[i].pos = lin_0->lins[0].a.pos;
        lin_0->dots[i].col = (struct vec4){ 0 };
    }
    lin_0->dot_count = dot_1->dot_count;

    dmp->dot_count = lin_0->dot_count;
    dmp->dots =
        mc_reallocf(dmp->dots, dmp->dot_count * sizeof(struct tetra_dot));
    memcpy(dmp->dots, lin_0->dots, dmp->dot_count * sizeof(struct tetra_dot));

    dmp->lin_count = lin_0->lin_count;
    dmp->lins =
        mc_reallocf(dmp->lins, dmp->lin_count * sizeof(struct tetra_lin));
    memcpy(dmp->lins, lin_0->lins, dmp->lin_count * sizeof(struct tetra_lin));
}

static void
_dot_swap(struct tetramesh *dot, int32_t a, int32_t b)
{
    struct tetra_dot const x = dot->dots[a];
    dot->dots[a] = dot->dots[b];
    dot->dots[b] = x;

    for (mc_ind_t i = 0; i < dot->dot_count; ++i) {
        if (dot->dots[i].antinorm == a) {
            dot->dots[i].antinorm = b;
        }
        if (dot->dots[i].antinorm == b) {
            dot->dots[i].antinorm = a;
        }
        if (dot->dots[i].inverse == a) {
            dot->dots[i].inverse = b;
        }
        if (dot->dots[i].inverse == b) {
            dot->dots[i].inverse = a;
        }
    }
}

// dmp assumed to initially match dot_0
static void
match_dot_dot(
    struct tetramesh *dot_0, struct tetramesh *dmp, struct tetramesh *dot_1
)
{
    if (dot_0->dots[0].is_dominant_sibling !=
        dot_1->dots[0].is_dominant_sibling) {
        _dot_swap(dot_1, 0, dot_1->dots[0].inverse);
    }

    if (dot_0->dots[0].antinorm != dot_1->dots[0].antinorm) {
        _dot_swap(dot_1, dot_0->dots[0].antinorm, dot_1->dots[0].antinorm);
    }

    if (dot_0->dots[0].inverse != dot_1->dots[0].inverse) {
        _dot_swap(dot_1, dot_0->dots[0].inverse, dot_1->dots[0].inverse);
    }
}

static mc_bool_t
topologies_match(struct tetramesh *a, struct tetramesh *b)
{
    if (a->tri_count != b->tri_count || a->lin_count != b->lin_count ||
        a->dot_count != b->dot_count) {
        return 0;
    }

    for (mc_ind_t i = 0; i < a->tri_count; ++i) {
        if (a->tris[i].ab != b->tris[i].ab || a->tris[i].bc != b->tris[i].bc ||
            a->tris[i].ca != b->tris[i].ca ||
            a->tris[i].antinorm != b->tris[i].antinorm ||
            a->tris[i].is_dominant_sibling != b->tris[i].is_dominant_sibling) {
            return 0;
        }
    }

    for (mc_ind_t i = 0; i < a->lin_count; ++i) {
        if (a->lins[i].next != b->lins[i].next ||
            a->lins[i].prev != b->lins[i].prev ||
            a->lins[i].antinorm != b->lins[i].antinorm ||
            a->lins[i].inverse != b->lins[i].inverse ||
            a->lins[i].is_dominant_sibling != b->lins[i].is_dominant_sibling) {
            return 0;
        }
    }

    for (mc_ind_t i = 0; i < a->dot_count; ++i) {
        if (a->dots[i].antinorm != b->dots[i].antinorm ||
            a->dots[i].inverse != b->dots[i].inverse ||
            a->dots[i].is_dominant_sibling != b->dots[i].is_dominant_sibling) {
            return 0;
        }
    }

    return 1;
}

static mc_bool_t
on_plane(struct vec3 base, struct vec3 norm, struct vec3 test)
{
    struct vec3 const sub = vec3_unit(vec3_sub(test, base));
    return vec3_dot(sub, norm) < GEOMETRIC_EPSILON;
}

static mc_bool_t
is_planar_albedo(struct tetramesh *a)
{
    if (!a->tri_count) {
        return 1;
    }

    mc_count_t found = 0;
    struct vec3 pqr[3];

    for (mc_ind_t i = 0; i < a->tri_count; ++i) {
        mc_bool_t equal = 0;
        for (mc_ind_t j = 0; j < found; ++j) {
            if (vec3_equals(pqr[j], a->tris[i].a.pos)) {
                equal = 1;
                break;
            }
        }

        if (!equal) {
            pqr[found] = a->tris[i].a.pos;
            if (++found == 3) {
                break;
            }
        }

        equal = 0;
        for (mc_ind_t j = 0; j < found; ++j) {
            if (vec3_equals(pqr[j], a->tris[i].b.pos)) {
                equal = 1;
                break;
            }
        }

        if (!equal) {
            pqr[found] = a->tris[i].b.pos;
            if (++found == 3) {
                break;
            }
        }

        equal = 0;
        for (mc_ind_t j = 0; j < found; ++j) {
            if (vec3_equals(pqr[j], a->tris[i].c.pos)) {
                equal = 1;
                break;
            }
        }

        if (!equal) {
            pqr[found] = a->tris[i].c.pos;
            if (++found == 3) {
                break;
            }
        }
    }

    if (found == 3) {
        struct vec3 const norm = vec3_unit(
            vec3_cross(vec3_sub(pqr[1], pqr[0]), vec3_sub(pqr[2], pqr[0]))
        );
        for (mc_ind_t i = 0; i < a->tri_count; ++i) {
            if (!on_plane(pqr[0], norm, a->tris[i].a.pos) ||
                !on_plane(pqr[0], norm, a->tris[i].b.pos) ||
                !on_plane(pqr[0], norm, a->tris[i].c.pos)) {
                return 0;
            }
        }
    }

    struct vec4 const comp = a->tris[0].a.col;
    for (mc_ind_t i = 0; i < a->tri_count; ++i) {
        if (!vec4_equals(a->tris[i].a.col, comp) ||
            !vec4_equals(a->tris[i].b.col, comp) ||
            !vec4_equals(a->tris[i].c.col, comp)) {
            return 0;
        }
    }

    return 1;
}

static struct tetramesh **
loop_separate(
    struct timeline_execution_context *executor, struct tetramesh *a,
    mc_graph_color_t *a_visited, mc_count_t *out_count
)
{
    mc_count_t a_mesh_count = 0;
    struct tetramesh **a_mesh = NULL;

    for (mc_ind_t i = 0; i < a->lin_count; ++i) {
        if (a_visited[i] || !a->lins[i].is_dominant_sibling) {
            continue;
        }

        MC_MEM_RESERVE(a_mesh, a_mesh_count);
        struct vector_field const vf = tetramesh_init(executor);
        struct tetramesh *const mesh = vf.value.pointer;
        a_mesh[a_mesh_count++] = mesh;

        int32_t j = (int32_t) i;
        do {
            MC_MEM_RESERVEN(mesh->lins, mesh->lin_count, 4);

            struct tetra_lin const curr = a->lins[j];

            int32_t k = (int32_t) mesh->lin_count;
            struct tetra_lin base = curr;
            base.antinorm = k + 2;
            base.inverse = k + 1;
            base.prev = j == (int32_t) i ? INT32_MIN : k - 4;
            base.next = k + 4;
            struct tetra_lin inve = a->lins[curr.inverse];
            inve.antinorm = k + 3;
            inve.inverse = k;
            inve.next = j == (int32_t) i ? INT32_MIN : k - 3;
            inve.prev = k + 5;
            struct tetra_lin anti = a->lins[curr.antinorm];
            anti.antinorm = k;
            anti.inverse = k + 3;
            anti.next = j == (int32_t) i ? INT32_MIN : k - 2;
            anti.prev = k + 6;
            struct tetra_lin antv = a->lins[a->lins[curr.antinorm].inverse];
            antv.antinorm = k + 1;
            antv.inverse = k + 2;
            antv.next = k + 7;
            antv.prev = j == (int32_t) i ? INT32_MIN : k - 1;
            mesh->lins[mesh->lin_count++] = base;
            mesh->lins[mesh->lin_count++] = inve;
            mesh->lins[mesh->lin_count++] = anti;
            mesh->lins[mesh->lin_count++] = antv;

            a_visited[j] = 1;
            a_visited[curr.antinorm] = 1;

            j = a->lins[j].next;
        } while (j != (int32_t) i);

        tetramesh_line_close(mesh);
    }

    *out_count = a_mesh_count;
    return a_mesh;
}

static mc_count_t
gcd(mc_count_t a, mc_count_t b)
{
    if (a < b) {
        mc_count_t const tmp = a;
        a = b;
        b = tmp;
    }

    while (b != 0) {
        mc_count_t const aux = a % b;
        a = b;
        b = aux;
    }

    return a;
}

static mc_bool_t
match_planar(
    struct timeline_execution_context *executor, struct tetramesh *a,
    struct tetramesh *dmp, struct tetramesh *b
)
{
    if (!a->lin_count || !b->lin_count || a->dot_count || b->dot_count ||
        !is_planar_albedo(a) || !is_planar_albedo(b)) {
        return 0;
    }

    struct vec4 const a_albedo = a->tri_count ? a->tris[0].a.col : VEC4_0;
    struct vec4 const b_albedo = b->tri_count ? b->tris[0].a.col : VEC4_0;

    if (a->tri_count) {
        tetramesh_downrank(executor, a);
    }
    if (b->tri_count) {
        tetramesh_downrank(executor, b);
    }
    if (dmp->tri_count) {
        tetramesh_downrank(executor, dmp);
    }

    for (mc_ind_t i = 0; i < a->lin_count; ++i) {
        if (!a->lins[i].is_dominant_sibling) {
            a->lins[i].a.col = a->lins[i].b.col = a_albedo;
        }
    }

    for (mc_ind_t i = 0; i < b->lin_count; ++i) {
        if (!b->lins[i].is_dominant_sibling) {
            b->lins[i].a.col = b->lins[i].b.col = b_albedo;
        }
    }

    //    mc_bool_t *a_visited
    mc_graph_color_t *a_visited =
        mc_calloc(a->lin_count, sizeof(mc_graph_color_t));
    mc_graph_color_t *b_visited =
        mc_calloc(b->lin_count, sizeof(mc_graph_color_t));

    mc_count_t a_mesh_count = 0;
    mc_count_t b_mesh_count = 0;
    struct tetramesh **a_mesh =
        loop_separate(executor, a, a_visited, &a_mesh_count);
    struct tetramesh **b_mesh =
        loop_separate(executor, b, b_visited, &b_mesh_count);

    a->lin_count = b->lin_count = dmp->lin_count = 0;
    /* using a cartesian product helps in 3 -> 2 scenarios (and thereby ensuring
     * winding order) */
    mc_count_t const lcm =
        a_mesh_count * b_mesh_count / gcd(a_mesh_count, b_mesh_count);
    for (mc_ind_t i = 0; i < lcm; ++i) {
        int32_t const u = (int32_t) (i % a_mesh_count);
        int32_t const v = (int32_t) (i % b_mesh_count);

        struct tetramesh *const dump = tetramesh_raw_copy(a_mesh[u]);

        // might match twice which technically can be a problem
        // but eh not a huge practical issue
        match_lin_lin(a_mesh[u], dump, b_mesh[v]);

        int32_t const kp = (int32_t) a->lin_count;
        mc_count_t const k = a->lin_count;
        int32_t const next = kp + (int32_t) a_mesh[u]->lin_count;

        a->lins =
            mc_reallocf(a->lins, sizeof(struct tetra_lin) * (mc_count_t) next);
        b->lins =
            mc_reallocf(b->lins, sizeof(struct tetra_lin) * (mc_count_t) next);
        dmp->lins = mc_reallocf(
            dmp->lins, sizeof(struct tetra_lin) * (mc_count_t) next
        );
        for (mc_ind_t j = 0; j < a_mesh[u]->lin_count; ++j) {
            a->lins[k + j] = a_mesh[u]->lins[j];
            a->lins[k + j].antinorm += kp;
            a->lins[k + j].inverse += kp;
            a->lins[k + j].next += kp;
            a->lins[k + j].prev += kp;

            b->lins[k + j] = b_mesh[v]->lins[j];
            b->lins[k + j].antinorm += kp;
            b->lins[k + j].inverse += kp;
            b->lins[k + j].next += kp;
            b->lins[k + j].prev += kp;

            dmp->lins[k + j] = dump->lins[j];
            dmp->lins[k + j].antinorm += kp;
            dmp->lins[k + j].inverse += kp;
            dmp->lins[k + j].next += kp;
            dmp->lins[k + j].prev += kp;
        }

        tetramesh_unref(dump);
        dmp->lin_count = b->lin_count = a->lin_count = (mc_count_t) next;
    }

    tetramesh_assert_invariants(a);
    tetramesh_assert_invariants(dmp);
    tetramesh_assert_invariants(b);

    for (mc_ind_t i = 0; i < a_mesh_count; ++i) {
        tetramesh_unref(a_mesh[i]);
    }
    mc_free(a_visited);
    mc_free(a_mesh);

    for (mc_ind_t i = 0; i < b_mesh_count; ++i) {
        tetramesh_unref(b_mesh[i]);
    }
    mc_free(b_visited);
    mc_free(b_mesh);

    /* essentially mark it as needing special interpolation */
#pragma message("TODO find better solution")
    a->flags = 0;
    dmp->flags = TETRAMESH_FLAG_WANTS_PLANAR_TRANSFORM;
    b->flags = 0;

    return 1;
}

static void
match_mesh(
    struct timeline_execution_context *executor, struct tetramesh *a,
    struct tetramesh *dmp, struct tetramesh *b
)
{
    if (topologies_match(a, b)) {
        /* use normal interpolation */
        a->flags = dmp->flags = b->flags = 0;
        return;
    }
    else if (match_planar(executor, a, dmp, b)) {
        return;
    }

    a->flags = dmp->flags = b->flags = 0;

    if (a->tri_count) {
        if (b->tri_count) {
            match_tri_tri(a, dmp, b);
        }
        else if (b->lin_count) {
            match_tri_lin(a, dmp, b);
        }
        else if (b->dot_count) {
            match_tri_dot(a, dmp, b);
        }
    }
    else if (a->lin_count) {
        if (b->tri_count) {
            match_tri_lin(b, dmp, a);
        }
        else if (b->lin_count) {
            match_lin_lin(a, dmp, b);
        }
        else if (b->dot_count) {
            match_lin_dot(a, dmp, b);
        }
    }
    else if (a->dot_count) {
        if (b->tri_count) {
            match_tri_dot(b, dmp, a);
        }
        else if (b->lin_count) {
            match_lin_dot(b, dmp, a);
        }
        else if (b->dot_count) {
            match_dot_dot(a, dmp, b);
        }
    }
}

void
match_group(
    struct timeline_execution_context *executor, struct vector_field *a_src,
    struct vector_field *b_src, struct vector_field a, struct vector_field dmp,
    struct vector_field b, mc_count_t a_count, mc_count_t b_count
)
{
    mc_bool_t const inverted = a_count < b_count;
    if (inverted) {
        mc_count_t const tmp = a_count;
        a_count = b_count;
        b_count = tmp;
        struct vector_field const tmp2 = a;
        a = b;
        b = tmp2;
        struct vector_field *const tmp3 = a_src;
        a_src = b_src;
        b_src = tmp3;
    }
    // take larger, and each time match it with closest one remaining
    // not perfect or permutation invariant or fast, but good enough

    mc_count_t *const visited = mc_calloc(b_count, sizeof(mc_count_t));

    //    mc_count_t const block_size = !b_count ? SIZE_MAX : (a_count + b_count
    //    - 1) / b_count;
    mc_ind_t start_j = 0;
    mc_ind_t j = 0;
    for (mc_ind_t i = 0; i < a_count; ++i) {
        // naive matching helps out generally..
        /* amount of times */
        //        mc_count_t const check_rem = !b_count;
        mc_ind_t best_j;
        if (!b_count) {
            best_j = SIZE_MAX;
        }
        else {
            /* amount of solutions to x % b_count = rem where 0 <= x < a_count
             */
            best_j = j;
            if ((a_count - j % b_count + b_count - 1) / b_count ==
                i - start_j + 1) {
                ++j;
                start_j = i + 1;
            }
        }

        //        double best_dist = DBL_MAX;
        //
        //        struct tetramesh const *i_mesh = a_src[i].value.pointer;
        //        struct vec3 const i_com = tetramesh_com(i_mesh);
        //        for (mc_ind_t j = 0; j < b_count; ++j) {
        //            if (visited[j] > i / b_count) {
        //                continue;
        //            }
        //
        //            struct tetramesh const *j_mesh = b_src[j].value.pointer;
        //            struct vec3 const j_com = tetramesh_com(j_mesh);
        //
        //            double const dist = vec3_norm(vec3_sub(j_com, i_com));
        //            if (dist < best_dist) {
        //                best_j = j;
        //                best_dist = dist;
        //            }
        //        }

        struct vector_field a_copy = tetramesh_owned(executor, a_src[i]);
        struct vector_field dmp_copy = tetramesh_owned(executor, a_src[i]);
        struct vector_field b_copy;
        if (best_j != SIZE_MAX) {
            b_copy = tetramesh_owned(executor, b_src[best_j]);
            visited[best_j]++;
        }
        else {
            b_copy = tetramesh_init(executor);
        }

        struct tetramesh *a_mesh = a_copy.value.pointer;
        struct tetramesh *d_mesh = dmp_copy.value.pointer;
        struct tetramesh *b_mesh = b_copy.value.pointer;
        match_mesh(executor, a_mesh, d_mesh, b_mesh);

        vector_plus(executor, a, &a_copy);
        vector_plus(executor, dmp, &dmp_copy);
        vector_plus(executor, b, &b_copy);
    }

    mc_free(visited);
}

static int
_tag_compare(void const *a, void const *b)
{
    struct vector_field const *ap = a;
    struct vector_field const *bp = b;

    struct tetramesh const *amesh = ap->value.pointer;
    struct tetramesh const *bmesh = bp->value.pointer;

    for (mc_ind_t i = 0; i < amesh->tag_count && i < bmesh->tag_count; ++i) {
        if (amesh->tags[i] > bmesh->tags[i]) {
            return 1;
        }
        else if (amesh->tags[i] < bmesh->tags[i]) {
            return -1;
        }
    }

    return (int) (amesh->tag_count) - (int) (bmesh->tag_count);
}

static int
_tag_stable_compare(void const *a, void const *b)
{
    int diff;
    if ((diff = _tag_compare(a, b))) {
        return diff;
    }

    struct vector_field const *ap = a;
    struct vector_field const *bp = b;

    struct tetramesh const *amesh = ap->value.pointer;
    struct tetramesh const *bmesh = bp->value.pointer;

    return (int) (amesh->payload - bmesh->payload);
}

static int
_hash_compare(void const *a, void const *b)
{
    struct vector_field const *ap = a;
    struct vector_field const *bp = b;

    struct tetramesh const *amesh = ap->value.pointer;
    struct tetramesh const *bmesh = bp->value.pointer;

    return (int) amesh->payload - (int) bmesh->payload;
}

static mc_status_t
_really_write_index(
    struct timeline_execution_context *executor, struct vector_field index_map,
    struct vector_field child_vector, mc_ind_t index
)
{
    // we don't really have ownership so might have to do unnecessary copy...
    struct vector_field vector_f = vector_field_nocopy_extract_type_message(
        executor, child_vector, VECTOR_FIELD_TYPE_VECTOR,
        "Expected tag entry to be a vector. Received %s."
    );
    if (!vector_f.vtable) {
        return MC_STATUS_FAIL;
    }
    vector_f = VECTOR_FIELD_COPY(executor, vector_f);
    struct vector *vector = vector_f.value.pointer;
    for (mc_ind_t i = 0; i < vector->field_count; ++i) {
        struct vector_field element = vector_field_nocopy_extract_type_message(
            executor, vector->fields[i], VECTOR_FIELD_TYPE_DOUBLE,
            "Expected tag sub entry to be a number. Received %s."
        );
        if (!element.vtable) {
            VECTOR_FIELD_FREE(executor, vector_f);
            return MC_STATUS_FAIL;
        }
    }

    struct vector_field const lvalue =
        map_index(executor, index_map, &vector_f);

    VECTOR_FIELD_FREE(executor, vector_f);
    struct vector_field *wrapped = lvalue.value.pointer;
    if (wrapped->vtable) {
        VECTOR_FIELD_ERROR(executor, "Duplication mention of tag!\n");
        return MC_STATUS_FAIL;
    }

    *wrapped = double_init(executor, (double) index);
    return MC_STATUS_SUCCESS;
}

static mc_status_t
_write_index(
    struct timeline_execution_context *executor, struct vector_field index_map,
    struct vector_field entry, mc_ind_t index
)
{
    struct vector_field const parent_vec =
        vector_field_nocopy_extract_type_message(
            executor, entry, VECTOR_FIELD_TYPE_VECTOR,
            "Expected tag entry to be a vector. Received %s."
        );
    if (!parent_vec.vtable) {
        return MC_STATUS_FAIL;
    }

    struct vector *parent = parent_vec.value.pointer;
    if (!parent->field_count) {
        return MC_STATUS_SUCCESS;
    }

    struct vector_field const child = vector_field_nocopy_extract_type_message(
        executor, parent->fields[0],
        VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_DOUBLE,
        "Expected tag sub entry to be either a vector or double. Received %s."
    );

    if (child.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        for (mc_ind_t i = 0; i < parent->field_count; ++i) {
            if (_really_write_index(
                    executor, index_map, parent->fields[i], index
                ) != MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }
        }

        return MC_STATUS_SUCCESS;
    }
    else if (child.vtable->type & VECTOR_FIELD_TYPE_DOUBLE) {
        return _really_write_index(executor, index_map, parent_vec, index);
    }
    else {
        return MC_STATUS_FAIL;
    }
}

static void
_write_mesh_index(
    struct timeline_execution_context *executor, struct vector_field index_map,
    struct vector_field mesh_v, mc_ind_t max_index
)
{
    struct tetramesh *mesh = mesh_v.value.pointer;
    mc_ind_t index;

    struct vector_field tag_v = vector_init(executor);
    for (mc_ind_t t = 0; t < mesh->tag_count; ++t) {
        struct vector_field tag = double_init(executor, (double) mesh->tags[t]);
        vector_plus(executor, tag_v, &tag);
    }

    struct vector_field lvalue = map_index(executor, index_map, &tag_v);
    struct vector_field dbl = *(struct vector_field *) lvalue.value.pointer;
    index = dbl.vtable ? (size_t) dbl.value.doub : max_index;

    VECTOR_FIELD_FREE(executor, tag_v);

    mesh->payload = index;
}

static void
_sort_arrange(
    struct timeline_execution_context *executor, struct vector const *a_vec,
    struct vector const *b_vec, int (*cmp)(void const *x, void const *y),
    int (*stable_cmp)(void const *x, void const *y), struct vector_field a_dmp,
    struct vector_field dmp_dmp, struct vector_field b_dmp
)
{
    qsort(
        a_vec->fields, a_vec->field_count, sizeof(struct vector_field),
        stable_cmp
    );
    qsort(
        b_vec->fields, b_vec->field_count, sizeof(struct vector_field),
        stable_cmp
    );

    mc_ind_t i = 0, j = 0;
    while (i < a_vec->field_count || j < b_vec->field_count) {
        mc_ind_t ip = i, jp = j;
        if (j == b_vec->field_count ||
            (i < a_vec->field_count &&
             cmp(&a_vec->fields[i], &b_vec->fields[j]) <= 0)) {
            while (ip < a_vec->field_count &&
                   !cmp(&a_vec->fields[i], &a_vec->fields[ip])) {
                ++ip;
            }
        }

        if (i == a_vec->field_count ||
            (j < b_vec->field_count &&
             cmp(&a_vec->fields[i], &b_vec->fields[j]) >= 0)) {
            while (jp < b_vec->field_count &&
                   !cmp(&b_vec->fields[j], &b_vec->fields[jp])) {
                ++jp;
            }
        }

        /* applying zero offset to nullpointer is undefined? */
        match_group(
            executor, a_vec->fields ? a_vec->fields + i : NULL,
            b_vec->fields ? b_vec->fields + j : NULL, a_dmp, dmp_dmp, b_dmp,
            ip - i, jp - j
        );

        i = ip;
        j = jp;
    }
}

mc_status_t
match_tree(
    struct timeline_execution_context *executor, struct vector_field a,
    struct vector_field b, struct vector_field a_dmp,
    struct vector_field dmp_dmp, struct vector_field b_dmp,
    mc_ind_t tag_map_index, struct vector_field tag_mapping
)
{
    struct vector const *a_vec = a.value.pointer;
    struct vector const *b_vec = b.value.pointer;

    // direct tag match
    if (tag_map_index == 0) {
        match_group(
            executor, a_vec->fields, b_vec->fields, a_dmp, dmp_dmp, b_dmp,
            a_vec->field_count, b_vec->field_count
        );
    }
    else if (tag_map_index == 1) {
        for (mc_ind_t i = 0; i < a_vec->field_count; ++i) {
            struct tetramesh *const mesh = a_vec->fields[i].value.pointer;
            mesh->payload = i;
        }

        for (mc_ind_t i = 0; i < b_vec->field_count; ++i) {
            struct tetramesh *const mesh = b_vec->fields[i].value.pointer;
            mesh->payload = i;
        }

        // tag to tag (sort)
        _sort_arrange(
            executor, a_vec, b_vec, &_tag_compare, &_tag_stable_compare, a_dmp,
            dmp_dmp, b_dmp
        );
    }
    else {
        struct vector_field fields[1] = { tag_mapping };
        // custom grouping, just go through and make sure there are no
        // contradictions
        LIBMC_FULL_CAST_RETURN(
            map, 0, VECTOR_FIELD_TYPE_MAP, return MC_STATUS_FAIL
        );
        struct map *tag_dict = map.value.pointer;

        struct vector_field a_index_map = map_init(executor);
        struct vector_field b_index_map = map_init(executor);

        mc_ind_t i = 0;
        for (struct map_node *head = tag_dict->head.next_ins; head;
             head = head->next_ins, ++i) {
            if (_write_index(executor, a_index_map, head->field, i) !=
                    MC_STATUS_SUCCESS ||
                _write_index(executor, b_index_map, head->value, i) !=
                    MC_STATUS_SUCCESS) {
                VECTOR_FIELD_ERROR(executor, "Invalid tag map");
                VECTOR_FIELD_FREE(executor, a_index_map);
                VECTOR_FIELD_FREE(executor, b_index_map);
                return MC_STATUS_FAIL;
            }
        }

        for (mc_ind_t j = 0; j < a_vec->field_count; ++j) {
            _write_mesh_index(executor, a_index_map, a_vec->fields[j], i);
        }

        for (mc_ind_t j = 0; j < b_vec->field_count; ++j) {
            _write_mesh_index(executor, b_index_map, b_vec->fields[j], i);
        }

        _sort_arrange(
            executor, a_vec, b_vec, &_hash_compare, &_hash_compare, a_dmp,
            dmp_dmp, b_dmp
        );

        VECTOR_FIELD_FREE(executor, a_index_map);
        VECTOR_FIELD_FREE(executor, b_index_map);
    }

    return MC_STATUS_SUCCESS;
}

static void
_null_action(
    struct tetramesh *prev, struct tetramesh *curr, struct tetramesh *next,
    float t, double action
)
{
    mc_bool_t const show =
        next->tri_count || next->lin_count || next->dot_count;
    if (action == 0) {
        // fade
        if (show) {
            curr->uniform.opacity = t * next->uniform.opacity;
        }
        else {
            curr->uniform.opacity = (1 - t) * prev->uniform.opacity;
        }
    }
    else {
        // write
        if (show) {
            write_interpolate(0, 1, next, curr, 0, t);
        }
        else {
            write_interpolate(0, 1, prev, curr, 0, 1 - t);
        }
    }
}

//  native transform(mesh_tree, pull, subtags, push, t, config, time, unit_map!,
//  path_arc)
void
lib_mc_transform(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vec3 path_arc;
    LIBMC_FULL_CAST(path_index, 4, VECTOR_FIELD_TYPE_DOUBLE);
    if (path_index.value.doub == 1) {
        LIBMC_VEC3(path_arc, 7);
    }
    else {
        path_arc = VEC3_0;
    }

    TRANSFORM_PREFIX(8);

    for (mc_ind_t i = 0; i < curr.subset_count; ++i) {
        struct tetramesh *const prv = prev.meshes[i];
        struct tetramesh *const tag = curr.meshes[i];
        struct tetramesh *const src = targ.meshes[i];

        if ((!prv->tri_count && !prv->lin_count && !prv->dot_count) ||
            (!src->tri_count && !src->lin_count && !src->dot_count)) {
            _null_action(prv, tag, src, t, null_action.value.doub);
        }
        /* planar */
        else if (!prv->flags && tag->flags && !prv->flags) {
            tag->tri_count = 0;

            tag->lin_count = src->lin_count;
            tag->lins = mc_reallocf(
                tag->lins, sizeof(struct tetra_lin) * src->lin_count
            );
            memcpy(
                tag->lins, src->lins, sizeof(struct tetra_lin) * src->lin_count
            );

            mesh_patharc_lerp(prv, tag, src, t, path_arc);
            if (tetramesh_uprank(tag, 1) != MC_STATUS_SUCCESS) {
                VECTOR_FIELD_ERROR(executor, "Error upranking!");
                executor->return_register = VECTOR_FIELD_NULL;
                return;
            }

            int32_t const j =
                prv->lins[0].is_dominant_sibling ? prv->lins[0].inverse : 0;
            struct vec4 const a_col = prv->lins[j].a.col;
            int32_t const k =
                src->lins[0].is_dominant_sibling ? src->lins[0].inverse : 0;
            struct vec4 const b_col = src->lins[k].a.col;
            struct vec4 const col = vec4_lerp(a_col, t, b_col);

            for (mc_ind_t q = 0; q < tag->tri_count; ++q) {
                tag->tris[q].a.col = tag->tris[q].b.col = tag->tris[q].c.col =
                    col;
            }
        }
        else {
            mesh_patharc_lerp(prv, tag, src, t, path_arc);
        }

        tag->modded = tag->dirty_hash_cache = 1;
    }

    TRANSFORM_SUFFIX;
}

void
lib_mc_bend(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    TRANSFORM_PREFIX(7);

    mc_bool_t failed = 0;
    for (mc_ind_t i = 0; i < curr.subset_count; ++i) {
        struct tetramesh *const prv = prev.meshes[i];
        struct tetramesh *const tag = curr.meshes[i];
        struct tetramesh *const src = targ.meshes[i];

        if (!tag->lin_count || tag->tri_count) {
            VECTOR_FIELD_ERROR(
                executor, "Cannot currently apply bend to meshes with "
                          "triangles; line meshes only"
            );
            failed = 1;
            break;
        }

        mesh_bend_lerp(prv, tag, src, t);

        tag->modded = tag->dirty_hash_cache = 1;
    }

    TRANSFORM_SUFFIX;
    if (failed) {
        executor->return_register = VECTOR_FIELD_NULL;
    }
}
