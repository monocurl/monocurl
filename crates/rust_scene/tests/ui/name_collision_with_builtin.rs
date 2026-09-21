// The derive snake-cases the struct name to `with`, which collides with the
// built-in `MeshValue::with` entry point — must be a compile error with a
// clear message, not a silently-shadowed method.
use rust_scene::Operator;

#[derive(Operator, Clone)]
struct With {
    pub amount: f64,
}

impl rust_scene::Apply for With {
    fn apply(&self, target: rust_scene::MeshValue) -> rust_scene::MeshValue {
        target
    }
}

fn main() {}
