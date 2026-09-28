# Fuzz findings

Executor bugs found by the differential fuzzer (`crates/scene_harness/src/fuzz`,
`mcfuzz`). While a finding is open, the reference evaluator rejects the programs
that reach it (search `fuzz/reference.rs` for the finding number) so the fuzzer
keeps passing and still covers everything else. When you fix one, delete its
rejection, move it to the fixed list below and run `mcfuzz --count 20000`.

## Open

None.

## Fixed

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
