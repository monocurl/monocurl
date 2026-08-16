# Custom operators — design & implementation spec

Goal: a user defines a fully first-class Monocurl operator — chainable,
retained in the structural tree, lerpable, label-addressable — with **at most
one attribute/derive plus the function body**. No manual trait plumbing, no
registration calls.

Read `API_DIRECTION.md` first; this document assumes its three-layer model.

## What "operator" means here

An operator transforms a mesh (or mesh-tree) and participates in animation:

- **Chainable**: `mesh_expr.wobble(0.2)` like the built-ins.
- **Retained**: the call becomes a node in the `MeshValue` tree carrying the
  operator's identity and typed argument slots, so structural lerp rule 3
  (same operator → lerp slots) and rule 4 (operator popping) apply.
- **Interpolatable**: the operator supplies an `(identity, modified)` endpoint
  pair — MCL's `[identity, modified]` return contract. The identity endpoint
  is what rule 4 pops through, making `x → wobble(x)` transitions animate.
- **Label-friendly**: inside `live!`, any argument can be labeled and becomes
  a typed field on the mesh handle.

## Authoring form A (recommended): `#[operator]` on a plain function

```rust
use monocurl::prelude::*;

#[operator]
fn wobble(target: MeshValue, amount: f64, frequency: f64) -> MeshValue {
    target.map_leaves(|m| m.point_map(|p| p + amount * (frequency * p.x).sin() * U))
}
```

Usage — indistinguishable from a built-in:

```rust
let m = circle(0.35).fill(BLUE.soft()).wobble(0.2, 10.0);
let mut ball = s.mesh(live! { Circle(radius: 0.35).wobble(amount: 0.2) });
ball.amount = 0.35;                                           // typed knob
```

Rules:

- First parameter must be `MeshValue` (any name); it is the operand and does
  not become a slot.
- Remaining parameters become **typed argument slots**, in order. Each slot
  type must implement `Keyed` (lerpable) — see "Slots and lerping" below.
- **No default arguments.** Rust has neither optional parameters nor
  overloading, and every way of faking them (one trait method per arity, an
  args-tuple `impl`, a differently-named short function) buys a call spelling
  at the cost of something un-Rust-like: name ambiguity, stray parentheses, or
  a second name for the same operation. Every parameter is required and
  positional. Operators that genuinely want ergonomic defaults use form B's
  struct-literal entry point, which is the idiomatic Rust answer:
  `mesh.with(Wobble { amount: 0.2, ..Default::default() })`. `#[default(..)]`
  on a parameter is rejected at macro-expansion time with a message pointing
  at that form.
- The body runs on **every evaluation** (memoized by the retained tree). It
  must be pure with respect to its inputs; document this, do not enforce it.

### Custom interpolation endpoints

The default identity endpoint is the untouched operand — correct for styling
and displacement operators. Operators that need a custom identity (MCL's
explicit `[identity, modified]` return, e.g. a rotation that must interpolate
along the arc rather than through lerped geometry) return the pair:

```rust
#[operator(endpoints)]
fn rotate_about(target: MeshValue, angle: f64, pivot: Vec3) -> (MeshValue, MeshValue) {
    let go = |a: f64| target.map_leaves(|m| m.rotated(a, 1.0 * B, pivot));
    (go(0.0), go(angle))
}
```

`endpoints` changes only the interpolation behavior; chaining, slots, and
labels work identically.

## Authoring form B: `#[derive(Operator)]` on a struct

For operators with many parameters, shared documentation, or reuse as data:

```rust
#[derive(Operator, Clone)]
pub struct Wobble {
    pub amount: f64,
    #[hold]
    pub axis_label: String,     // equality-only slot, not lerped
    pub frequency: f64,
}

impl Apply for Wobble {
    fn apply(&self, target: MeshValue) -> MeshValue {
        target.map_leaves(|m| /* ... */ m)
    }
}
```

- Fields are the slots (all fields; `#[hold]` for equality-only).
- The derive generates the same chaining method, named by snake-casing the
  type name (`.wobble(...)`), taking the fields in declaration order, plus a
  struct-literal entry point `mesh.with(Wobble { .. })` so `..Default::default()`
  can stand in for defaults.
- Custom endpoints: implement `ApplyEndpoints` instead of `Apply` (mutually
  exclusive; the derive detects which via the trait bound it emits — in
  practice: derive emits `impl Operator for Wobble where Wobble: OperatorBody`,
  and `Apply`/`ApplyEndpoints` each blanket-impl `OperatorBody`).

Form A should literally desugar to form B: the attribute macro synthesizes the
struct from the signature and the `Apply`/`ApplyEndpoints` impl from the body.
Implement B's machinery first; A is a thin front end.

## The trait layer (what the macros target)

Keep the hand-written trait surface small enough to implement manually — that
is the escape hatch and the test surface:

```rust
/// Field-wise interpolation. Implemented for f64, Vec2/3/4, Color, tuples,
/// Vec<T: Keyed>, Option<T: Keyed>, MeshValue, and Hold<T: PartialEq>.
pub trait Keyed: Clone + 'static {
    fn lerp(&self, other: &Self, t: f64) -> Result<Self>;
}

/// The operator body, one of two flavors.
pub trait Apply: Clone + 'static {
    fn apply(&self, target: MeshValue) -> MeshValue;
}
pub trait ApplyEndpoints: Clone + 'static {
    fn endpoints(&self, target: MeshValue) -> (MeshValue, MeshValue);
}

/// What the retained tree stores. Object-safe. Generated, never hand-written
/// except in tests.
pub trait OperatorNode: 'static {
    fn op_id(&self) -> OpId;                       // identity for lerp rule 3
    fn endpoints(&self, target: MeshValue) -> (MeshValue, MeshValue);
    fn lerp_slots(&self, other: &dyn OperatorNode, t: f64) -> Result<Box<dyn OperatorNode>>;
    fn slot(&self, index: usize) -> &dyn Slot;      // for live! field wiring
    fn slot_mut(&mut self, index: usize) -> &mut dyn Slot;
    fn slot_count(&self) -> usize;
    fn clone_node(&self) -> Box<dyn OperatorNode>;
    fn debug_name(&self) -> &'static str;
}
```

`OpId` is derived from `TypeId` of the operator struct. Two nodes lerp
slot-wise iff `op_id` matches (rule 3); otherwise rule 4 pops the deeper side
through `endpoints(..).0`.

`MeshValue`'s operator variant stores `Box<dyn OperatorNode>` + operand +
memo cell. Applying an operator = pushing a node; evaluation = fold from the
leaf outward, calling `endpoints(target).1` (or `apply`) at each layer,
memoized per node.

## Slots and lerping

- Slot lerp failure (e.g. two unequal `Hold` values) is a runtime error carrying
  the operator name and slot index/name — mirror the executor's
  "cannot lerp … argument at index N differs" wording.
- `MeshValue` itself implements `Keyed` (recursive structural lerp), so
  operators can take mesh-valued arguments and nesting works.
- `Hold<T>`: newtype wrapper for equality-only slots; `#[hold]` on a derive
  field wraps transparently.

## Mesh-trees and filters

- Operators receive the whole tree. `MeshValue::map_leaves(f)` applies `f` to
  each concrete mesh leaf, preserving list structure — the standard helper for
  per-mesh operators (MCL's implicit tree mapping, made explicit and cheap).
- Tag filters are not special-cased in the operator machinery. An operator
  wanting filtering takes a slot of type `Filter` (a cheap cloneable predicate
  over tag lists, `#[hold]` by default) and consults it inside `map_leaves`.
  Built-ins (`fill`, `stroke`) will follow the same pattern, so custom
  operators are not second-class.

## Interaction with `live!`

`live!` sees `.wobble(amount: 0.2)` as: positional argument 0 of the chained
call gets label `amount`. The generated anonymous struct's field writes through
to the retained node via `slot_mut(index)` with a downcast to the field's
inferred type. The macro does not need to know the operator exists — labels
are positional bindings, so this works for built-ins, form-A, and form-B
operators uniformly. (This is why `OperatorNode::slot_mut` is indexed, not
named: names live only in the `live!`-generated struct.)

## Constructors: the same machinery minus the operand

`#[constructor]` on `fn glyph(size: f64, seed: u64) -> MeshValue` generates the
leaf-node analog (`ConstructorNode`: `op_id` + slots + evaluate). Same slots,
same lerp rule 3, same `live!` interplay. Implement it in the same PR as the
operator attribute — it is the same code path with `endpoints` replaced by
`evaluate`.

## Implementation milestones (ordered, each independently testable)

1. **Trait layer + retained operator nodes** in `rust_scene/src/`:
   `Keyed` impls, `Apply`/`ApplyEndpoints`, `OperatorNode`, `MeshValue`
   operator variant with memoization, `map_leaves`. Hand-write one operator
   (`Shift`) against the raw traits as the reference test.
2. **Structural lerp rules 1–4** over `MeshValue`, including operator popping
   through `endpoints().0`. Port test cases from
   `crates/integration_tests/tests/basic_executor_tests/live_values.rs`
   (function-identity matching, unequal-depth popping, hold-slot equality).
3. **`#[derive(Operator)]`** in `rust_scene_macros`: emit `OperatorNode` impl,
   chaining extension trait, `with(..)` entry, `#[hold]`, snake-case naming.
   Trybuild tests for error cases (non-`Keyed` field without `#[hold]`,
   name collisions with built-in methods → compile error with a clear message).
4. **`#[operator]` / `#[operator(endpoints)]` fn attribute** desugaring to
   form B. `#[constructor]` alongside.
5. **`live!` slot wiring** (only once the `live!` macro itself lands): labeled
   args on chained operator calls resolve to `slot_mut` writes.

Milestones 1–4 do not depend on the async runtime, the scene cell, or `live!`,
and can be validated with `evaluate()` + `lerp(t)` unit tests alone.

## Acceptance example

The following must compile and behave as commented, using only public API:

```rust
#[operator]
fn squash(target: MeshValue, factor: f64) -> MeshValue {
    target.map_leaves(|m| m.scaled(Vec3::new(1.0, factor, 1.0)))
}

#[test]
fn squash_is_first_class() {
    let a = circle(1.0).squash(1.0);
    let b = circle(2.0).squash(0.5);
    let mid = a.lerp(&b, 0.5).unwrap();          // rule 3: radius 1.5, factor 0.75

    let plain = circle(1.0);
    let popped = plain.lerp(&b, 0.5).unwrap();    // rule 4: pops through identity
    let _ = (mid, popped);
}
```

## Non-goals (for this spec)

- Animation-wrapping operators (`delay`, `rate`, `slow`) — those wrap plays,
  not meshes, and belong to the anim-combinator layer.
- Bake-time embed state (`Trans`-class contour matching) — that is the
  `Animator` trait on plays, not mesh operators.
- The reassignment/retype surface — explicitly deferred, see
  `API_DIRECTION.md`.
