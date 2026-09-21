// Monocurl operators have no default arguments: every parameter is required
// and positional. `#[default(..)]` on a parameter must fail to compile with a
// message pointing at the struct-literal form instead.
// (the macro replaces the fn with a `compile_error!`, so `MeshValue` ends up
// unused — that warning is noise, not part of what this fixture asserts)
#![allow(unused_imports)]

use rust_scene::{MeshValue, operator};

#[operator]
fn wobble(target: MeshValue, amount: f64, #[default(10.0)] frequency: f64) -> MeshValue {
    let _ = (amount, frequency);
    target
}

fn main() {}
