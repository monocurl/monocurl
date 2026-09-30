# Fuzz findings

Executor bugs found by the differential fuzzer (`crates/scene_harness/src/fuzz`,
`mcfuzz`). While a finding is open, the reference evaluator rejects the programs
that reach it (search `fuzz/reference.rs` for the finding number) so the fuzzer
keeps passing and still covers everything else. When you fix one, delete its
rejection, move it to the fixed list below and run `mcfuzz --count 20000`.

`mcfuzz --batches` also samples each program's lambdas through the batch
constructors (`ExplicitFunc`, `ExplicitFunc2d`, `Shader`, `point_map`,
`color_map`) and runs it with the kernel tier off, on and in verify mode. A
verify panic names the kernel engines that disagree (dynamic, typed or lane
machine, or the tier against the interpreter); an off/on difference is a
finding too.

## Open

## Fixed

5. A kernel single call drops the "live function" wrapper of a result it
   returns from a lambda with default parameters, so `lerp` of two such
   results interpolates the values instead of the arguments. Only
   `KernelMode::On` takes single calls (verify mode leaves them in the
   interpreter), so this is the single-call path of the dynamic register
   machine (`KernelTier` / `try_kernel_call`) against the interpreter:

   ```
   let f = |x, k = 2| x * x * k
   let g = |x| f(x, 3)
   print lerp(g(1), g(3), 0.5)
   ```

   prints `12.0` with the tier off (`f(2, 3)`) and `15.0` with it on
   (`lerp(3, 27, 0.5)`). Batch seed 33589 (`mcfuzz --batches --seed 33589`
   before the rejection). The reference rejects a `lerp` of call results that
   were returned out of another lambda.

   Fixed: the single-call path now declines when any closure the call can
   reach fills default arguments, so such results stay in the interpreter
   and keep their wrapper. The reference no longer rejects these programs.


1. `//` on two ints truncated towards zero while the float form floors
   (`-7 // 2` gave `-3`). `ops.rs` now floors both, as does the kernel tier.
   Seed 127.
2. `sign(0.0)` returned `1.0` and `sign(-0.0)` returned `-1.0` through
   `f64::signum`; both are `0.0` now.
3. Calling a lambda with a default parameter produced a "live function" wrapper
   that several consumers failed to read through: `==` / `!=` and `in`,
   elementwise list operators, `abs` / `sign`, calling the returned value, and
   `lerp`'s equal-endpoint shortcut. Each now reads through a finished wrapper.
   Seed 16.
4. `sum` of a list of lists failed because the fold started from the int `0`;
   `std.util` now starts from the first element. `dot([], [])` also printed
   `-0.0` because Rust's float `Sum` starts from `-0.0`.
