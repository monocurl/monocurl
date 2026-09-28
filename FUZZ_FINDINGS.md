# Fuzz findings

Executor bugs found by the differential fuzzer (`crates/scene_harness/src/fuzz`,
`mcfuzz`). None of them is fixed yet. Until one is, the reference evaluator
rejects the programs that reach it (search `fuzz/reference.rs` for the finding
number), so the fuzzer keeps passing and still covers everything else. When
you fix one, delete its rejection and run `mcfuzz --count 20000`.

Seeds refer to the generator as committed alongside this file; each one
reproduces only after its rejection is removed, since the fuzzer skips the
construct otherwise.

## 1. `//` on two ints truncates instead of flooring

`language-basics.md` calls `//` floor division, and with a float operand it
does floor. With two ints, `eval_non_list_binary` in
`crates/executor/src/executor/ops.rs` uses Rust's `a / b`, which rounds towards
zero. The two disagree whenever the operands have opposite signs and do not
divide evenly.

```monocurl
import std.math
print [-7 // 2, 7 // -2, -7.0 // 2, 7.0 // -2]
```

- expected: `[-4, -4, -4, -4]`
- actual: `[-3, -3, -4, -4]`

Fuzzer seed: 127. Likely fix: `a.div_euclid(b)` is not it either (wrong for a
negative divisor); use `a / b - ((a % b != 0) && ((a < 0) != (b < 0))) as i64`.
Note that `mod` is `rem_euclid`, so after the fix `a == b * (a // b) + mod(a, b)`
holds for positive `b` only; that is a separate design question.

## 2. `sign(0.0)` is `1.0`

`std.math` documents `sign` as "-1, 0, or 1", and `sign(0)` is `0`. The float
branch of `sign` in `crates/stdlib/src/math/scalar.rs` uses `f64::signum`, which
returns `1.0` for `0.0` and `-1.0` for `-0.0`.

```monocurl
import std.math
print [sign(0), sign(0.0), sign(-0.0)]
```

- expected: `[0, 0.0, 0.0]`
- actual: `[0, 1.0, -1.0]`

Found while writing the reference evaluator from the documentation rather than
by a failing seed (the generator rarely produces an exact float zero).

## 3. calls to lambdas with default parameters leak a "live function" wrapper

Calling a lambda that has any default parameter goes through the labeled-call
path in `exec_lambda_invoke` (`crates/executor/src/executor/invoke.rs`), which
pushes a `Value::InvokedFunction` with the result cached, even when the call is
not labeled. Most consumers elide that wrapper, but several do not, so the
value behaves differently from the plain number or list it stands for. This
includes stdlib wrappers with defaults, notably `range(start, stop, step = 1)`.

```monocurl
import std.math
import std.util

let f = |a, b = 2| a + b
let k = |a = 1| |x| x + a

print f(1) == 3                 # expected 1, actual 0
print [f(1)] == [3]             # expected 1, actual 0
print range(0, 3) == [0, 1, 2]  # expected 1, actual 0
print lerp(f(1), 3, 0.5)        # expected 3, actual 3.0 (endpoint equality missed)
print [f(1)] * 2                # expected [6], actual runtime error:
                                #   cannot apply * to list element [0]:
                                #   unsupported binary op * on live function and int
```

The same wrapper also breaks (each is a runtime error where a value is
expected):

- `[f(1)] + [1]`, `[f(1)] / 2`, `-[f(1)]`, `[range(0, 2)] * 2`: elementwise list
  operators (`combine_lists`, `apply_list_scalar`, `negate_list` in `ops.rs`)
  call the synchronous `eval_binary` on raw heap elements
- `abs(f(1))`, `sign(f(1))`: `abs` and `sign` in `stdlib/src/math/scalar.rs`
  match the raw stack value instead of reading through `read_float`
- `k()(5)`: calling the lambda returned by a default-bearing lambda reports
  `type error: expected lambda, got live function`
- `==` / `!=` go through `Value::values_equal`, which never equates a live
  call with a plain value and compares two live calls by lambda and arguments
  rather than by result

The wrapper survives `let` bindings, lambda parameters, `return`, list literals,
`.=` and indexed stores; indexing, arithmetic on the value itself, `for`
iteration, truthiness, printing, and most natives unwrap it.

Fuzzer seed: 16 (`2 * [.., f(9.959), ..]` where `f` has a default parameter).

