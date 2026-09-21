// A field that isn't `Keyed` and isn't marked `#[hold]` must fail to
// compile: the derive's `lerp_slots` calls `Keyed::lerp` on every non-hold
// field, so a missing `Keyed` impl surfaces as an ordinary trait-bound error
// at that call site.
use rust_scene::Operator;

#[derive(Clone)]
struct NotKeyed;

#[derive(Operator, Clone)]
struct BadOperator {
    pub value: NotKeyed,
}

impl rust_scene::Apply for BadOperator {
    fn apply(&self, target: rust_scene::MeshValue) -> rust_scene::MeshValue {
        target
    }
}

fn main() {}
