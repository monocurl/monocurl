//
//  mc_util.c
//  Monocurl
//
//  Created by Manu Bhat on 2/19/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

#include <math.h>
#include <stdlib.h>

#include "lvalue.h"
#include "mc_util.h"
#include "vector.h"

#define LIBMC_ERROR(executor, ...)                                             \
    do {                                                                       \
        VECTOR_FIELD_ERROR(executor, __VA_ARGS__);                             \
        executor->return_register = VECTOR_FIELD_NULL;                         \
        return;                                                                \
    } while (0)

#define DEFAULT_EPSILON 1e-4
#define DEFAULT_INTEGRATION_STEPS 257

static mc_bool_t error_flag = 0;
static struct timeline_execution_context *ex;

static int
compare(void const *x, void const *y)
{
    struct vector_field const *const lhs = x;
    struct vector_field rhs = *(struct vector_field const *) y;
    if (!lhs->vtable) {
        error_flag = 1;
        return -1;
    }
    struct vector_field const ret =
        VECTOR_FIELD_BINARY(ex, *lhs, op_comp, &rhs);
    if (ret.vtable) {
        return (int) ret.value.doub;
    }
    error_flag = 1;
    return -1;
}

void
lib_mc_sort(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);
    error_flag = 0;
    ex = executor;
    struct vector_field const vec = VECTOR_FIELD_COPY(executor, arg);
    struct vector *const v = vec.value.pointer;
    qsort(v->fields, v->field_count, sizeof(struct vector_field), &compare);

    if (error_flag) {
        VECTOR_FIELD_FREE(executor, vec);
        executor->return_register = VECTOR_FIELD_NULL;
    }
    executor->return_register = vec;
}

void
lib_mc_left_key(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const v = arg.value.pointer;

    struct vector_field const ret = vector_init(executor);

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field inner = vector_init(executor);
        struct vector_field push = double_init(executor, (double) i);
        vector_plus(executor, inner, &push);
        push = VECTOR_FIELD_COPY(executor, v->fields[i]);
        vector_plus(executor, inner, &push);

        vector_plus(executor, ret, &inner);
    }

    executor->return_register = ret;
}

void
lib_mc_right_key(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const v = arg.value.pointer;

    struct vector_field const ret = vector_init(executor);

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field inner = vector_init(executor);
        struct vector_field push = VECTOR_FIELD_COPY(executor, v->fields[i]);
        vector_plus(executor, inner, &push);
        push = double_init(executor, (double) i);
        vector_plus(executor, inner, &push);

        vector_plus(executor, ret, &inner);
    }

    executor->return_register = ret;
}

void
lib_mc_reverse(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const v = arg.value.pointer;

    struct vector_field const ret = vector_init(executor);

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field push =
            VECTOR_FIELD_COPY(executor, v->fields[v->field_count - 1 - i]);
        vector_plus(executor, ret, &push);
    }

    executor->return_register = ret;
}

void
lib_mc_zip(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_VECTOR);
    LIBMC_FULL_CAST(r, 1, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const lv = l.value.pointer;
    struct vector *const rv = r.value.pointer;

    if (lv->field_count != rv->field_count) {
        LIBMC_ERROR(
            executor,
            "Cannot zip vector of length `%zu` with vector of length %%zu",
            lv->field_count, rv->field_count
        );
    }

    struct vector_field const ret = vector_init(executor);

    for (mc_ind_t i = 0; i < lv->field_count; ++i) {
        struct vector_field inner = vector_init(executor);
        struct vector_field push = VECTOR_FIELD_COPY(executor, lv->fields[i]);
        vector_plus(executor, inner, &push);
        push = VECTOR_FIELD_COPY(executor, rv->fields[i]);
        vector_plus(executor, inner, &push);

        vector_plus(executor, ret, &push);
    }

    executor->return_register = ret;
}

void
lib_mc_map(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);
    LIBMC_FULL_CAST(func, 1, VECTOR_FIELD_TYPE_FUNCTION);

    struct vector *const v = arg.value.pointer;

    struct vector_field const ret = vector_init(executor);

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field current = v->fields[i];

        function_call(executor, func, 1, &current);
        if (!executor->return_register.vtable ||
            !vector_plus(executor, ret, &executor->return_register).vtable) {
            VECTOR_FIELD_FREE(executor, ret);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
    }

    executor->return_register = ret;
}

void
lib_mc_reduce(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);
    struct vector_field curr[2];
    curr[0] = fields[1];
    LIBMC_FULL_CAST(func, 2, VECTOR_FIELD_TYPE_FUNCTION);

    struct vector *const v = arg.value.pointer;

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        curr[1] = v->fields[i];
        function_call(executor, func, 2, curr);
        if (i > 0) {
            VECTOR_FIELD_FREE(executor, curr[0]);
        }
        curr[0] = executor->return_register;
        if (!curr[0].vtable) {
            return;
        }
    }
}

void
lib_mc_len(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_MAP);

    if (arg.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        executor->return_register = double_init(
            executor,
            (double) ((struct vector *) arg.value.pointer)->field_count
        );
    }
    else {
        /* map */
        executor->return_register = double_init(
            executor, (double) ((struct map *) arg.value.pointer)->field_count
        );
    }
}

void
lib_mc_depth(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vector_field const curr = vector_field_safe_extract_type(
        executor, caller, VECTOR_FIELD_TYPE_VECTOR
    );
    if (curr.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *const vec = curr.value.pointer;
        double max = 0;
        for (mc_ind_t i = 0; i < vec->field_count; ++i) {
            lib_mc_depth(executor, VECTOR_FIELD_NULL, 1, &vec->fields[i]);
            if (!executor->return_register.vtable) {
                return;
            }
            if (executor->return_register.value.doub > max) {
                max = executor->return_register.value.doub;
            }
        }

        executor->return_register = double_init(executor, max + 1);
    }
    else if (curr.vtable) {
        executor->return_register = double_init(executor, 0);
    }
    else {
        executor->return_register = VECTOR_FIELD_NULL;
    }
}

void
lib_mc_count(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);
    LIBMC_FULL_CAST(func, 1, VECTOR_FIELD_TYPE_FUNCTION);

    struct vector *const v = arg.value.pointer;

    mc_count_t ret = 0;

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field current = v->fields[i];

        function_call(executor, func, 1, &current);
        if (!executor->return_register.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        else if (!executor->return_register.vtable->op_bool) {
            VECTOR_FIELD_FREE(executor, executor->return_register);
            VECTOR_FIELD_ERROR(executor, "Cannot coerce field to boolean");
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        struct vector_field const read =
            VECTOR_FIELD_UNARY(executor, executor->return_register, op_bool);
        if (!read.vtable) {
            VECTOR_FIELD_FREE(executor, executor->return_register);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        else if (VECTOR_FIELD_DBOOL(read)) {
            ++ret;
        }
    }

    executor->return_register = double_init(executor, (double) ret);
}

void
lib_mc_filter(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);
    LIBMC_FULL_CAST(func, 1, VECTOR_FIELD_TYPE_FUNCTION);

    struct vector *const v = arg.value.pointer;

    struct vector_field const ret = vector_init(executor);

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field current = v->fields[i];

        function_call(executor, func, 1, &current);
        if (!executor->return_register.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        else if (!executor->return_register.vtable->op_bool) {
            VECTOR_FIELD_FREE(executor, executor->return_register);
            VECTOR_FIELD_FREE(executor, ret);
            VECTOR_FIELD_ERROR(executor, "Cannot coerce field to boolean");
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        struct vector_field const read =
            VECTOR_FIELD_UNARY(executor, executor->return_register, op_bool);
        if (!read.vtable) {
            VECTOR_FIELD_FREE(executor, ret);
            VECTOR_FIELD_FREE(executor, executor->return_register);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        else if (VECTOR_FIELD_DBOOL(read)) {
            current = VECTOR_FIELD_COPY(executor, current);
            vector_plus(executor, ret, &current);
        }
    }

    executor->return_register = ret;
}

void
lib_mc_sum(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const v = arg.value.pointer;
    double sum = 0;

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field const doub = vector_field_nocopy_extract_type(
            executor, v->fields[i], VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!doub.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        sum += doub.value.doub;
    }

    executor->return_register = double_init(executor, sum);
}

void
lib_mc_product(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const v = arg.value.pointer;
    double prod = 1;

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field const doub = vector_field_nocopy_extract_type(
            executor, v->fields[i], VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!doub.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        prod *= doub.value.doub;
    }

    executor->return_register = double_init(executor, prod);
}

void
lib_mc_all(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const v = arg.value.pointer;

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        if (!v->fields[i].vtable || !v->fields[i].vtable->op_bool) {
            LIBMC_ERROR(executor, "Cannot coerce to expected type of bool");
        }
        struct vector_field const read =
            VECTOR_FIELD_UNARY(executor, v->fields[i], op_bool);
        if (!read.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        if (!VECTOR_FIELD_DBOOL(read)) {
            executor->return_register = double_init(executor, 0);
            return;
        }
    }

    executor->return_register = double_init(executor, 1);
}

void
lib_mc_any(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const v = arg.value.pointer;

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        if (!v->fields[i].vtable || !v->fields[i].vtable->op_bool) {
            LIBMC_ERROR(executor, "Cannot coerce to expected type of bool");
        }
        struct vector_field const read =
            VECTOR_FIELD_UNARY(executor, v->fields[i], op_bool);
        if (!read.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        if (VECTOR_FIELD_DBOOL(read)) {
            executor->return_register = double_init(executor, 1);
            return;
        }
    }

    executor->return_register = double_init(executor, 0);
}

void
lib_mc_map_keys(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_MAP);

    struct map *const map = arg.value.pointer;

    struct vector_field ret = vector_init(executor);
    for (struct map_node *head = map->head.next_ins; head;
         head = head->next_ins) {
        struct vector_field key = VECTOR_FIELD_COPY(executor, head->field);
        vector_plus(executor, ret, &key);
    }

    executor->return_register = ret;
}

void
lib_mc_map_values(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_MAP);

    struct map *const map = arg.value.pointer;

    struct vector_field ret = vector_init(executor);
    for (struct map_node *head = map->head.next_ins; head;
         head = head->next_ins) {
        struct vector_field key = VECTOR_FIELD_COPY(executor, head->value);
        vector_plus(executor, ret, &key);
    }

    executor->return_register = ret;
}

void
lib_mc_map_items(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_MAP);

    struct map *const map = arg.value.pointer;

    struct vector_field ret = vector_init(executor);
    for (struct map_node *head = map->head.next_ins; head;
         head = head->next_ins) {
        struct vector_field inner = vector_init(executor);
        struct vector_field push = VECTOR_FIELD_COPY(executor, head->field);
        vector_plus(executor, inner, &push);
        push = VECTOR_FIELD_COPY(executor, head->value);
        vector_plus(executor, inner, &push);

        vector_plus(executor, ret, &push);
    }

    executor->return_register = ret;
}

void
lib_mc_mean(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const v = arg.value.pointer;
    if (v->field_count == 0) {
        VECTOR_FIELD_ERROR(
            executor, "Expected input to have at least one entry"
        );
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    double sum = 0;

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field const doub = vector_field_nocopy_extract_type(
            executor, v->fields[i], VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!doub.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        sum += doub.value.doub;
    }

    executor->return_register = double_init(executor, sum / v->field_count);
}

void
lib_mc_std_dev(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const v = arg.value.pointer;
    double sum = 0;

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field const doub = vector_field_nocopy_extract_type(
            executor, v->fields[i], VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!doub.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        sum += doub.value.doub;
    }

    sum /= v->field_count;

    double std = 0;

    for (mc_ind_t i = 0; i < v->field_count; ++i) {
        struct vector_field const doub = vector_field_nocopy_extract_type(
            executor, v->fields[i], VECTOR_FIELD_TYPE_DOUBLE
        );
        std += (doub.value.doub - sum) * (doub.value.doub - sum);
    }

    std = sqrt(std / v->field_count);

    executor->return_register = double_init(executor, std);
}

// func integrate([args] {[default_n] {f(x), a, b}, [custom] {f(x), a, b, n}) =
// native integrate(args, f!, a, b, n)
void
lib_mc_integrate(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vector_field args = fields[0];
    LIBMC_FULL_CAST(func, 1, VECTOR_FIELD_TYPE_FUNCTION);
    LIBMC_FULL_CAST(a, 2, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(b, 3, VECTOR_FIELD_TYPE_DOUBLE);
    struct vector_field n_pos = fields[4];

    double n = DEFAULT_INTEGRATION_STEPS;
    if (args.value.doub == 1) {
        n_pos = vector_field_nocopy_extract_type(
            executor, n_pos, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!n_pos.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        n = n_pos.value.doub;
    }
    double const step = (b.value.doub - a.value.doub) / n;
    double sum = 0;

    for (mc_ind_t i = 0; i < n; ++i) {
        struct vector_field curr =
            double_init(executor, a.value.doub + step * i);
        function_call(executor, func, 1, &curr);

        struct vector_field const d = vector_field_nocopy_extract_type(
            executor, executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!d.vtable) {
            VECTOR_FIELD_FREE(executor, executor->return_register);
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        sum += d.value.doub;
    }

    executor->return_register = double_init(executor, sum);
}

// func derivative([args] {[default_epsilon] {f(x), x}, [custom] {f(x), x, eps})
// = native derivative(args, f!, x, eps)
void
lib_mc_derivative(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vector_field args = fields[0];
    LIBMC_FULL_CAST(func, 1, VECTOR_FIELD_TYPE_FUNCTION);
    LIBMC_FULL_CAST(x, 2, VECTOR_FIELD_TYPE_DOUBLE);
    struct vector_field epsilon = fields[3];

    double eps = DEFAULT_EPSILON;
    if (args.value.doub == 1) {
        epsilon = vector_field_nocopy_extract_type(
            executor, epsilon, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!epsilon.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        eps = epsilon.value.doub;
    }

    struct vector_field y;

    y = double_init(executor, x.value.doub - eps);
    function_call(executor, func, 1, &y);
    struct vector_field const a = vector_field_extract_type(
        executor, &executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
    );
    if (!a.vtable) {
        VECTOR_FIELD_FREE(executor, executor->return_register);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    y = double_init(executor, x.value.doub + eps);
    function_call(executor, func, 1, &y);
    struct vector_field const b = vector_field_extract_type(
        executor, &executor->return_register, VECTOR_FIELD_TYPE_DOUBLE
    );
    if (!b.vtable) {
        VECTOR_FIELD_FREE(executor, executor->return_register);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    executor->return_register =
        double_init(executor, (b.value.doub - a.value.doub) / (2 * eps));
}

// func limit([args] {[default_epsilon] {f(x), x}, [custom] {f(x), x, eps}}) =
// native limit(args, f!, x, eps)
void
lib_mc_limit(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vector_field args = fields[0];
    LIBMC_FULL_CAST(func, 1, VECTOR_FIELD_TYPE_FUNCTION);
    LIBMC_FULL_CAST(x, 2, VECTOR_FIELD_TYPE_DOUBLE);
    struct vector_field epsilon = fields[3];

    double eps = DEFAULT_EPSILON;
    if (args.value.doub == 1) {
        epsilon = vector_field_nocopy_extract_type(
            executor, epsilon, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!epsilon.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }
        eps = epsilon.value.doub;
    }

    struct vector_field y = double_init(executor, x.value.doub - eps);
    function_call(executor, func, 1, &y);
}

void
lib_mc_ln(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);
    double const res = log(arg.value.doub);
    if (res != res) {
        VECTOR_FIELD_ERROR(executor, "Invalid log argument");
        executor->return_register = VECTOR_FIELD_NULL;
    }
    else {
        executor->return_register = double_init(executor, res);
    }
}

void
lib_mc_log2(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);
    double const res = log2(arg.value.doub);
    if (res != res) {
        VECTOR_FIELD_ERROR(executor, "Invalid log argument");
        executor->return_register = VECTOR_FIELD_NULL;
    }
    else {
        executor->return_register = double_init(executor, res);
    }
}

void
lib_mc_log10(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);
    double const res = log10(arg.value.doub);
    if (res != res) {
        VECTOR_FIELD_ERROR(executor, "Invalid log argument");
        executor->return_register = VECTOR_FIELD_NULL;
    }
    else {
        executor->return_register = double_init(executor, res);
    }
}

void
lib_mc_log(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(num, 1, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(base, 0, VECTOR_FIELD_TYPE_DOUBLE);

    double const res = log(num.value.doub) / log(base.value.doub);
    if (res != res) {
        VECTOR_FIELD_ERROR(executor, "Invalid log argument");
        executor->return_register = VECTOR_FIELD_NULL;
    }
    else {
        executor->return_register = double_init(executor, res);
    }
}

void
lib_mc_sin(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, sin(arg.value.doub));
}

void
lib_mc_cos(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, cos(arg.value.doub));
}

void
lib_mc_tan(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, tan(arg.value.doub));
}

void
lib_mc_cot(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, 1 / tan(arg.value.doub));
}

void
lib_mc_sec(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, 1 / cos(arg.value.doub));
}

void
lib_mc_csc(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, 1 / sin(arg.value.doub));
}

void
lib_mc_arcsin(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);
    double const res = asin(arg.value.doub);
    if (res != res) {
        executor->return_register = VECTOR_FIELD_NULL;
    }
    else {
        executor->return_register = double_init(executor, res);
    }
}

void
lib_mc_arccos(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);

    double const res = acos(arg.value.doub);
    if (res != res) {
        executor->return_register = VECTOR_FIELD_NULL;
    }
    else {
        executor->return_register = double_init(executor, res);
    }
}

void
lib_mc_arctan(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);

    double const res = atan(arg.value.doub);
    if (res != res) {
        executor->return_register = VECTOR_FIELD_NULL;
    }
    else {
        executor->return_register = double_init(executor, res);
    }
}

void
lib_mc_factorial(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(arg, 0, VECTOR_FIELD_TYPE_DOUBLE);

    if (arg.value.doub < 0) {
        VECTOR_FIELD_ERROR(
            executor, "Cannot take factorial with negative number x `%f`",
            arg.value.doub
        );
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    mc_long_t ret = 1;
    for (mc_long_t i = 2; i <= arg.value.doub; ++i) {
        ret *= i;
    }

    executor->return_register = double_init(executor, (double) ret);
}

void
lib_mc_choose(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(r, 1, VECTOR_FIELD_TYPE_DOUBLE);

    if (n.value.doub < 0) {
        LIBMC_ERROR(
            executor, "Cannot choose with negative number x `%f`", n.value.doub
        );
    }
    else if (n.value.doub + 1 <= r.value.doub) {
        LIBMC_ERROR(
            executor, "Cannot choose with r `%f` greater than n `%f`",
            r.value.doub, n.value.doub
        );
    }
    else if (r.value.doub < 0) {
        LIBMC_ERROR(
            executor, "Cannot choose with negative number r `%f%", r.value.doub
        );
    }

    double const r_prime = n.value.doub - r.value.doub;

    mc_long_t ret = 1;
    if (r_prime > r.value.doub) {
        for (mc_long_t i = (mc_long_t) r_prime + 1; i <= n.value.doub; ++i) {
            ret *= i;
        }
        for (mc_long_t i = 2; i <= r.value.doub; ++i) {
            ret /= i;
        }
    }
    else {
        for (mc_long_t i = (mc_long_t) r.value.doub + 1; i <= n.value.doub;
             ++i) {
            ret *= i;
        }
        for (mc_long_t i = 2; i <= r_prime; ++i) {
            ret /= i;
        }
    }

    executor->return_register = double_init(executor, (double) ret);
}

void
lib_mc_permute(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(r, 1, VECTOR_FIELD_TYPE_DOUBLE);

    if (n.value.doub < 0) {
        LIBMC_ERROR(
            executor, "Cannot permute with negative number x `%f`", n.value.doub
        );
    }
    else if (n.value.doub + 1 <= r.value.doub) {
        LIBMC_ERROR(
            executor, "Cannot permute with r `%f` greater than n `%f`",
            r.value.doub, n.value.doub
        );
    }
    else if (r.value.doub < 0) {
        LIBMC_ERROR(
            executor, "Cannot permute with negative number r `%f%", r.value.doub
        );
    }

    double const r_prime = n.value.doub - r.value.doub;

    mc_long_t ret = 1;
    for (mc_long_t i = (mc_long_t) r_prime + 1; i <= n.value.doub; ++i) {
        ret *= i;
    }

    executor->return_register = double_init(executor, (double) ret);
}

static mc_long_t
gcd(mc_long_t n, mc_long_t m)
{
    if (n < 0) {
        return gcd(-n, m);
    }
    if (m < 0) {
        return gcd(n, -n);
    }
    if (n < m) {
        return gcd(m, n);
    }
    if (n == 0) {
        return m;
    }
    else {
        return gcd(n % m, m);
    }
}

void
lib_mc_gcd(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(m, 1, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(
        executor,
        (double) gcd((mc_long_t) n.value.doub, (mc_long_t) m.value.doub)
    );
}

void
lib_mc_max(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(m, 1, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(
        executor, n.value.doub > m.value.doub ? n.value.doub : m.value.doub
    );
}

void
lib_mc_min(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(m, 1, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(
        executor, n.value.doub < m.value.doub ? n.value.doub : m.value.doub
    );
}

void
lib_mc_abs(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);
    executor->return_register =
        double_init(executor, n.value.doub < 0 ? -n.value.doub : n.value.doub);
}

void
lib_mc_clamp(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(m, 1, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(r, 2, VECTOR_FIELD_TYPE_DOUBLE);

    if (l.value.doub > r.value.doub) {
        LIBMC_ERROR(
            executor, "Cannot clamp with l `%f` greater than r `%f`",
            l.value.doub, r.value.doub
        );
    }

    if (m.value.doub < l.value.doub) {
        executor->return_register = double_init(executor, l.value.doub);
    }
    else if (m.value.doub > r.value.doub) {
        executor->return_register = double_init(executor, r.value.doub);
    }
    else {
        executor->return_register = double_init(executor, m.value.doub);
    }
}

static mc_bool_t
is_prime(mc_long_t n)
{
    /* sieve of euler not necessary since we're just taking a single number? */
    mc_long_t const s = (mc_long_t) sqrt((double) n);
    for (mc_long_t i = 2; i <= s; ++i) {
        if (n % i == 0) {
            return 0;
        }
    }

    return 1;
}

void
lib_mc_is_prime(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_DOUBLE);
    executor->return_register =
        double_init(executor, is_prime((mc_long_t) l.value.doub));
}

void
lib_mc_sign(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);
    if (n.value.doub < 0) {
        executor->return_register = double_init(executor, -1);
    }
    if (n.value.doub == 0) {
        executor->return_register = double_init(executor, 0);
    }
    if (n.value.doub > 0) {
        executor->return_register = double_init(executor, +1);
    }
}

void
lib_mc_mod(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(m, 1, VECTOR_FIELD_TYPE_DOUBLE);

    double raw = fmod(n.value.doub, m.value.doub);
    if (raw < 0) {
        raw += m.value.doub;
    }

    executor->return_register = double_init(executor, raw);
}

void
lib_mc_floor(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, floor(n.value.doub));
}

void
lib_mc_round(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, round(n.value.doub));
}

void
lib_mc_ceil(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, ceil(n.value.doub));
}

void
lib_mc_trunc(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(n, 0, VECTOR_FIELD_TYPE_DOUBLE);

    executor->return_register = double_init(executor, trunc(n.value.doub));
}

void
lib_mc_random(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(r, 1, VECTOR_FIELD_TYPE_DOUBLE);

    if (l.value.doub > r.value.doub) {
        LIBMC_ERROR(
            executor, "call to random found l '%f' to be higher than r '%f'",
            l.value.doub, r.value.doub
        );
    }

    executor->return_register = double_init(
        executor,
        l.value.doub + ((r.value.doub - l.value.doub) * rand() / RAND_MAX)
    );
}

void
lib_mc_randint(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_DOUBLE);
    LIBMC_FULL_CAST(r, 1, VECTOR_FIELD_TYPE_DOUBLE);

    if (l.value.doub > r.value.doub) {
        LIBMC_ERROR(
            executor, "call to randint found l '%f' to be higher than r '%f'",
            l.value.doub, r.value.doub
        );
    }

    executor->return_register = double_init(
        executor,
        floor(
            l.value.doub + ((r.value.doub - l.value.doub) * rand() / RAND_MAX)
        )
    );
}

void
lib_mc_norm(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const lv = l.value.pointer;

    double sum = 0;
    for (mc_ind_t i = 0; i < lv->field_count; ++i) {
        struct vector_field lhs = lv->fields[i];

        lhs = vector_field_nocopy_extract_type(
            executor, lhs, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!lhs.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        sum += lhs.value.doub * lhs.value.doub;
    }

    executor->return_register = double_init(executor, sqrt(sum));
}

void
lib_mc_normalize(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const lv = l.value.pointer;

    double sum = 0;
    for (mc_ind_t i = 0; i < lv->field_count; ++i) {
        struct vector_field lhs = lv->fields[i];

        lhs = vector_field_nocopy_extract_type(
            executor, lhs, VECTOR_FIELD_TYPE_DOUBLE
        );
        if (!lhs.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        sum += lhs.value.doub * lhs.value.doub;
    }

    double norm = sqrt(sum);
    if (norm < GEOMETRIC_EPSILON) {
        norm = 1;
    }

    struct vector_field const ret = vector_init(executor);
    for (mc_ind_t i = 0; i < lv->field_count; ++i) {
        struct vector_field const lhs = vector_field_nocopy_extract_type(
            executor, lv->fields[i], VECTOR_FIELD_TYPE_DOUBLE
        );

        struct vector_field push = double_init(executor, lhs.value.doub / norm);
        vector_plus(executor, ret, &push);
    }

    executor->return_register = ret;
}

void
lib_mc_dot(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_VECTOR);
    LIBMC_FULL_CAST(r, 1, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const lv = l.value.pointer;
    struct vector *const rv = r.value.pointer;

    if (lv->field_count != rv->field_count) {
        LIBMC_ERROR(
            executor,
            "Cannot dot vector of length `%zu` with vector of length %%zu",
            lv->field_count, rv->field_count
        );
    }

    double sum = 0;

    for (mc_ind_t i = 0; i < lv->field_count; ++i) {
        struct vector_field lhs = lv->fields[i];
        lhs = vector_field_nocopy_extract_type(
            executor, lhs, VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field rhs = rv->fields[i];
        rhs = vector_field_nocopy_extract_type(
            executor, rhs, VECTOR_FIELD_TYPE_DOUBLE
        );

        if (!lhs.vtable || !rhs.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        sum += lhs.value.doub * rhs.value.doub;
    }

    executor->return_register = double_init(executor, sum);
}

/* both have to be a two or three vectors */
void
lib_mc_cross(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_VECTOR);
    LIBMC_FULL_CAST(r, 1, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const lv = l.value.pointer;
    struct vector *const rv = r.value.pointer;

    if (lv->field_count != rv->field_count) {
        LIBMC_ERROR(
            executor,
            "Cannot cross vector of length `%zu` with vector of length %zu",
            lv->field_count, rv->field_count
        );
    }
    if (lv->field_count < 2 || lv->field_count > 3) {
        LIBMC_ERROR(
            executor,
            "Cannot cross vector of length `%zu` with vector of length %zu",
            lv->field_count, rv->field_count
        );
    }

    if (lv->field_count == 2) {
        struct vector_field const a = vector_field_nocopy_extract_type(
            executor, lv->fields[0], VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field const b = vector_field_nocopy_extract_type(
            executor, lv->fields[1], VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field const c = vector_field_nocopy_extract_type(
            executor, rv->fields[0], VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field const d = vector_field_nocopy_extract_type(
            executor, rv->fields[1], VECTOR_FIELD_TYPE_DOUBLE
        );

        if (!a.vtable || !b.vtable || !c.vtable || !d.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        executor->return_register = double_init(
            executor, a.value.doub * d.value.doub - b.value.doub * c.value.doub
        );
    }
    else {
        struct vector_field const a = vector_field_nocopy_extract_type(
            executor, lv->fields[0], VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field const b = vector_field_nocopy_extract_type(
            executor, lv->fields[1], VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field const c = vector_field_nocopy_extract_type(
            executor, lv->fields[2], VECTOR_FIELD_TYPE_DOUBLE
        );

        struct vector_field const d = vector_field_nocopy_extract_type(
            executor, rv->fields[0], VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field const e = vector_field_nocopy_extract_type(
            executor, rv->fields[1], VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field const f = vector_field_nocopy_extract_type(
            executor, rv->fields[2], VECTOR_FIELD_TYPE_DOUBLE
        );

        if (!a.vtable || !b.vtable || !c.vtable || !d.vtable || !e.vtable ||
            !f.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        struct vector_field const ret = vector_init(executor);

        struct vector_field x = double_init(
            executor, b.value.doub * f.value.doub - c.value.doub * e.value.doub
        );
        struct vector_field y = double_init(
            executor, -a.value.doub * f.value.doub + c.value.doub * d.value.doub
        );
        struct vector_field z = double_init(
            executor, a.value.doub * e.value.doub - b.value.doub * d.value.doub
        );

        vector_plus(executor, ret, &x);
        vector_plus(executor, ret, &y);
        vector_plus(executor, ret, &z);

        executor->return_register = ret;
    }
}

/* project u onto v */
void
lib_mc_proj(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(l, 0, VECTOR_FIELD_TYPE_VECTOR);
    LIBMC_FULL_CAST(r, 1, VECTOR_FIELD_TYPE_VECTOR);

    struct vector *const lv = r.value.pointer;
    struct vector *const rv = l.value.pointer;

    if (lv->field_count != rv->field_count) {
        LIBMC_ERROR(
            executor,
            "Cannot proj vector of length `%zu` onto vector of length %%zu",
            rv->field_count, lv->field_count
        );
    }

    double sum = 0;
    double l_mag = 0;

    for (mc_ind_t i = 0; i < lv->field_count; ++i) {
        struct vector_field lhs = lv->fields[i];
        lhs = vector_field_nocopy_extract_type(
            executor, lhs, VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field rhs = rv->fields[i];
        rhs = vector_field_nocopy_extract_type(
            executor, rhs, VECTOR_FIELD_TYPE_DOUBLE
        );

        if (!lhs.vtable || !rhs.vtable) {
            executor->return_register = VECTOR_FIELD_NULL;
            return;
        }

        sum += lhs.value.doub * rhs.value.doub;
        l_mag += lhs.value.doub * lhs.value.doub;
    }

    double const scalar = l_mag < GEOMETRIC_EPSILON ? sum : sum / l_mag;

    struct vector_field const ret = vector_init(executor);
    for (mc_ind_t i = 0; i < lv->field_count; ++i) {
        struct vector_field curr =
            double_init(executor, lv->fields[i].value.doub * scalar);
        vector_plus(executor, ret, &curr);
    }

    executor->return_register = ret;
}

struct vector_field
vector_add(
    struct timeline_execution_context *executor, struct vector_field *fields
)
{
    LIBMC_FULL_CAST_RETURN(
        l, 0, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_DOUBLE,
        return VECTOR_FIELD_NULL
    );
    LIBMC_FULL_CAST_RETURN(r, 1, l.vtable->type, return VECTOR_FIELD_NULL);

    if (l.vtable->type & VECTOR_FIELD_TYPE_DOUBLE) {
        return double_init(executor, l.value.doub + r.value.doub);
    }

    struct vector *const lv = l.value.pointer;
    struct vector *const rv = r.value.pointer;

    if (lv->field_count != rv->field_count) {
        VECTOR_FIELD_ERROR(
            executor,
            "Cannot add vector of length `%zu` with vector of length %zu",
            lv->field_count, rv->field_count
        );
        return VECTOR_FIELD_NULL;
    }

    struct vector_field const ret = vector_init(executor);

    for (mc_ind_t i = 0; i < lv->field_count; ++i) {
        struct vector_field lhs = lv->fields[i];
        lhs = vector_field_nocopy_extract_type(
            executor, lhs, VECTOR_FIELD_TYPE_DOUBLE
        );
        struct vector_field rhs = rv->fields[i];
        rhs = vector_field_nocopy_extract_type(
            executor, rhs, VECTOR_FIELD_TYPE_DOUBLE
        );

        if (!lhs.vtable || !rhs.vtable) {
            VECTOR_FIELD_FREE(executor, ret);
            return VECTOR_FIELD_NULL;
        }

        struct vector_field push =
            double_init(executor, lhs.value.doub + rhs.value.doub);
        vector_plus(executor, ret, &push);
    }

    return ret;
}

void
lib_mc_vec_add(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    executor->return_register = vector_add(executor, fields);
}

struct vector_field
vector_multiply(
    struct timeline_execution_context *executor, struct vector_field *fields
)
{
    LIBMC_FULL_CAST_RETURN(
        l, 0, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_DOUBLE,
        return VECTOR_FIELD_NULL
    );
    LIBMC_FULL_CAST_RETURN(
        r, 1, VECTOR_FIELD_TYPE_VECTOR | VECTOR_FIELD_TYPE_DOUBLE,
        return VECTOR_FIELD_NULL
    );

    mc_bool_t const l_doub = l.vtable->type & VECTOR_FIELD_TYPE_DOUBLE;
    mc_bool_t const r_doub = r.vtable->type & VECTOR_FIELD_TYPE_DOUBLE;
    if (l_doub && r_doub) {
        return double_init(executor, l.value.doub * r.value.doub);
    }

    struct vector *const lv = l.value.pointer;
    struct vector *const rv = r.value.pointer;

    if (!l_doub && !r_doub && lv->field_count != rv->field_count) {
        VECTOR_FIELD_ERROR(
            executor,
            "Cannot multiply vector of length `%zu` with vector of length %%zu",
            lv->field_count, rv->field_count
        );
        return VECTOR_FIELD_NULL;
    }

    struct vector_field const ret = vector_init(executor);
    mc_count_t const count = l_doub ? rv->field_count : lv->field_count;

    for (mc_ind_t i = 0; i < count; ++i) {
        struct vector_field lhs =
            l_doub ? l
                   : vector_field_nocopy_extract_type(
                         executor, lv->fields[i], VECTOR_FIELD_TYPE_DOUBLE
                     );
        struct vector_field rhs =
            r_doub ? r
                   : vector_field_nocopy_extract_type(
                         executor, rv->fields[i], VECTOR_FIELD_TYPE_DOUBLE
                     );

        if (!lhs.vtable || !rhs.vtable) {
            VECTOR_FIELD_FREE(executor, ret);
            return VECTOR_FIELD_NULL;
        }

        struct vector_field push =
            double_init(executor, lhs.value.doub * rhs.value.doub);
        vector_plus(executor, ret, &push);
    }

    return ret;
}

void
lib_mc_vec_mul(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    executor->return_register = vector_multiply(executor, fields);
}

void
lib_mc_str_replace(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    char const *str = vector_field_str(executor, fields[0]);
    if (!str) {
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    char const *substr = vector_field_str(executor, fields[1]);
    if (!substr) {
        mc_free((char *) str);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    char const *with = vector_field_str(executor, fields[2]);
    if (!with) {
        mc_free((char *) substr);
        mc_free((char *) with);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }

    /* not particular efficient... */
    struct str_dynamic ret = str_dynamic_init();
    for (char const *read = str; *read; ++read) {
        char const *sub_read = read;
        mc_bool_t matches = 1;
        for (char const *sub_cmp = substr; *sub_cmp; ++sub_cmp, ++sub_read) {
            if (*sub_read != *sub_cmp) {
                matches = 0;
                break;
            }
        }

        if (matches) {
            str_dynamic_append(&ret, with);
            read = sub_read;
            read--; /* about to increment */
        }
        else {
            char buffer[2] = { *read, 0 };
            str_dynamic_append(&ret, buffer);
        }
    }

    struct vector_field vector = vector_init(executor);
    for (char *c = ret.pointer; *c; ++c) {
        struct vector_field sub = char_init(executor, *c);
        vector_plus(executor, vector, &sub);
    }
    executor->return_register = vector;

    mc_free((char *) str);
    mc_free((char *) substr);
    mc_free((char *) with);
}

static mc_status_t
reference_map_dfs(
    struct timeline_execution_context *executor, struct vector_field curr,
    struct vector_field map, struct vector_field out
)
{
    if (curr.vtable == &reference_vtable) {
        return reference_map_dfs(
            executor, *(struct vector_field *) curr.value.pointer, map, out
        );
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        function_call(executor, map, 1, &curr);
        if (!executor->return_register.vtable) {
            return MC_STATUS_FAIL;
        }
        vector_plus(executor, out, &executor->return_register);

        return MC_STATUS_SUCCESS;
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_VECTOR) {
        struct vector *v = curr.value.pointer;
        for (mc_ind_t i = 0; i < v->field_count; ++i) {
            if (reference_map_dfs(executor, v->fields[i], map, out) !=
                MC_STATUS_SUCCESS) {
                return MC_STATUS_FAIL;
            }
        }

        return MC_STATUS_SUCCESS;
    }
    else {
        VECTOR_FIELD_ERROR(executor, "Invalid reference variable");
        return MC_STATUS_FAIL;
    }
}

void
lib_mc_reference_map(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    LIBMC_FULL_CAST(func, 1, VECTOR_FIELD_TYPE_FUNCTION);

    struct vector_field ret = vector_init(executor);
    if (reference_map_dfs(executor, fields[0], func, ret) !=
        MC_STATUS_SUCCESS) {
        VECTOR_FIELD_FREE(executor, ret);
        executor->return_register = VECTOR_FIELD_NULL;
        return;
    }
    else {
        executor->return_register = ret;
    }
}

static struct vector_field
read_followers_map_dfs(
    struct timeline_execution_context *executor, struct vector_field curr
)
{
    if (curr.vtable == &reference_vtable) {
        return read_followers_map_dfs(
            executor, *(struct vector_field *) curr.value.pointer
        );
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        struct vector_field *stack_memory = curr.value.pointer;
        long long ind = stack_memory - executor->stack;
        struct vector_field value = executor->creation_follower_stack[ind];
        if (!value.vtable) {
            VECTOR_FIELD_ERROR(
                executor,
                "Trying to read follower of variable that is not an iterator!"
            );
            return VECTOR_FIELD_NULL;
        }
        return VECTOR_FIELD_COPY(executor, value);
    }
    else {
        struct vector *v = curr.value.pointer;
        struct vector_field out = vector_init(executor);
        for (mc_ind_t i = 0; i < v->field_count; ++i) {
            struct vector_field sub =
                read_followers_map_dfs(executor, v->fields[i]);
            if (!sub.vtable) {
                VECTOR_FIELD_FREE(executor, sub);
                return VECTOR_FIELD_NULL;
            }
            vector_plus(executor, out, &sub);
        }

        return out;
    }
}

static void
set_followers_map_dfs(
    struct timeline_execution_context *executor, struct vector_field curr
)
{
    if (curr.vtable == &reference_vtable) {
        set_followers_map_dfs(
            executor, *(struct vector_field *) curr.value.pointer
        );
    }
    else if (curr.vtable->type & VECTOR_FIELD_TYPE_LVALUE) {
        struct vector_field *stack_memory = curr.value.pointer;
        long long ind = stack_memory - executor->stack;
        struct vector_field lvalue =
            lvalue_init(executor, &executor->creation_follower_stack[ind]);
        VECTOR_FIELD_BINARY(executor, lvalue, assign, &curr);
    }
    else {
        struct vector *v = curr.value.pointer;
        for (mc_ind_t i = 0; i < v->field_count; ++i) {
            set_followers_map_dfs(executor, v->fields[i]);
        }
    }
}

void
lib_mc_read_followers(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    executor->return_register = read_followers_map_dfs(executor, fields[0]);
}

void
lib_mc_set_followers(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    set_followers_map_dfs(executor, fields[0]);
    executor->return_register = double_init(executor, 0);
}

void
lib_mc_is_scene_variable(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    struct vector_field it = fields[0];
    while (it.vtable == &reference_vtable) {
        it = *(struct vector_field *) it.value.pointer;
    }
    assert(it.vtable->type & VECTOR_FIELD_TYPE_LVALUE);
    mc_ind_t const frame =
        (mc_ind_t) ((struct vector_field *) it.value.pointer - executor->stack);
    mc_ind_t const ind = executor->follower_stack[frame];
    mc_bool_t const is_scene = timeline_is_scene_variable(executor, ind);
    executor->return_register = double_init(executor, is_scene != 0);
}

void
lib_mc_not_implemented_yet(
    struct timeline_execution_context *executor, struct vector_field caller,
    mc_count_t fc, struct vector_field *fields
)
{
    VECTOR_FIELD_ERROR(executor, "Function not implemented yet!");
    executor->return_register = VECTOR_FIELD_NULL;
}
