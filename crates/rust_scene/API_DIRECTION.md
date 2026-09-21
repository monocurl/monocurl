# Rust native API — converged direction (2026-08-15)

This supersedes parts of `DESIGN.md` (kept for history). It records the design
converged after several iterations, so implementation work can proceed without
the original discussion. The guiding requirement throughout: **recreate MCL's
ergonomics with the least possible indirection.** No per-mesh struct
declarations, no macros on ordinary invocations, no string-literal attribute
access, no free-standing signal/var plumbing — each of those was tried on paper
and rejected.

## The three separated layers

The design factors what MCL fuses into one concept into three independent
layers. Each can be built and tested on its own.

### 1. Structure — `MeshValue` is a retained lazy tree

Every mesh expression records its structure: constructor/operator identity plus
argument slots, whether or not anything is labeled. Evaluation is lazy and
memoized; `Clone` is copy-on-write (matching MCL's deep-copy value semantics).

- Built-in constructors are plain functions: `circle(0.35)`, `text("hi", 0.7)`.
- Built-in operators are ordinary **chaining methods**, in application order:
  `circle(0.35).fill(BLUE.soft()).stroke(BLUE, 2.0).center(1.4 * L)`.
- Custom operators chain the same way (see `OPERATORS.md`).
- Interpolation ("lerp matching") is **structural and label-independent**,
  porting the executor's rules (`crates/executor/src/executor/lerp.rs`):
  1. equal values → either side;
  2. numbers → linear blend;
  3. same constructor/operator identity → lerp argument slots element-wise
     (slots that can't lerp must be equal);
  4. mismatched operator depth → pop a layer from the deeper side through the
     operator's identity endpoint ("operator popping");
  plus containers recursively.

Because structure is always retained, a **plain unlabeled reassignment still
animates** when function identities match. Labels are never consulted for
matching.

### 2. Identity — leader/follower lives in an untyped scene cell

A drawn mesh is a scene **cell** holding `leader` (what code writes) and
`follower` (what is drawn), both retained trees. Writing the leader is instant
and invisible; only a play moves the follower — exact MCL semantics, including
auto-sync of all leaders at the end of the init section.

The typed handle `Mesh<A>` is only a **view** of a cell. Views are consumed by
operations that change the leader's type, so the borrow checker rules out
stale-handle bugs.

> **Deferred decision — reassignment surface.** How re-typing/reassignment is
> spelled is intentionally not settled. Candidates on the table:
> `let ball = ball.assign(new_expr)` (consuming method, explicit rebinding);
> something like `Leader::retype(&ball, new_expr)`; or a macro form. Whatever
> the spelling, the semantics are fixed: same cell, follower untouched, nothing
> visible until a play, interpolation by the structural rules above. Do not
> block other work on this; implement the cell/view split with a provisional
> `assign` and keep the surface easy to rename.

### 3. Labels — `live!` is a typed veneer, nothing more

`live!` takes an **ordinary Rust expression** and additionally permits a
`label:` token before any argument of any call or method call:

```rust
let mut ball = s.mesh(live! {
    Circle(radius: 0.35).fill(BLUE.soft()).center(pos: 1.4 * L)
});
ball.radius = 0.6;      // real typed field; typo = compile error
ball.pos = 1.4 * R;
```

The macro emits one anonymous struct per call site with one **generic** field
per label (`struct __Ball<T0, T1> { pos: T0, radius: T1 }` — field types come
from inference, the macro never needs to know them). Unlabeled arguments become
hidden held slots. `Mesh<_>` derefs to the struct, making field mutation plain
Rust. Skipping `live!` loses nothing except named knobs: not lerpability, not
identity, not leader/follower.

## Time and playback

- The scene is an `async fn`. Every `play` returns a future; a single-threaded
  executor owns a **virtual clock** and advances it exactly when every live
  task is parked on a play. `join!`, `select!`, `FuturesUnordered` compose
  animations; `s.slide("name").await` is a barrier and cut point.
- **Run-once model**: user code executes exactly once, forward. Each play bakes
  a pure segment (embed once at bake time; frame function per `t`). Scrubbing,
  backward seek, export, and the web player operate on the baked timeline and
  never re-run user code. No determinism requirements on scene code.
- Params (`s.param(v)`) use signal-style read tracking with version counters
  (the executor's `StatefulCache` scheme), with the dual leader/follower read
  mode so animating a param drives dependent meshes.

## Reuse, not rewrite

Mesh geometry is `crates/geo::Mesh`; constructors, operators, tessellation,
text, and the animation embed/lerp kernels already exist as native Rust in
`crates/stdlib` (the `__monocurl__native__` functions). This crate wraps those
under typed signatures. Do not re-implement geometry.

## Iteration history (why the rejected shapes are rejected)

1. Slide closures + heavy `#[live]`-generated typed structs and attribute
   proxies — too much ceremony, slides must be flat statements.
2. Declare-a-`Keyed`-struct-per-mesh + closure — declaring a struct for every
   mesh is unacceptable; no ad-hoc retyping.
3. A macro (`m!`) on **every** invocation with a dynamic label tree — macros on
   ordinary calls are unacceptable.
4. Free-standing `Var<T>` signals + mesh closures — the indirection (vars
   declared apart from the mesh, `.get()` everywhere) kills MCL's feel.
5. Declaration-site `#[live]` with string-literal attribute access
   (`ball.set("radius", ..)`) — labels invisible at the call site.
6. MCL-style pipeline syntax inside `live!` and a hidden-rebinding
   `live!{ x = ... }` statement — operators should look like Rust (method
   chains); hidden rebinding rejected.

Current shape (layers 1–3 above) is the survivor. Operator authoring is
specified in `OPERATORS.md`.
