//! `#[derive(Operator)]` / `#[operator]` error-case tests.
//!
//! `.stderr` golden files are checked in alongside each fixture (trybuild's
//! default `compile_fail` behavior requires one). The
//! `non_keyed_field_without_hold.stderr` snapshot includes rustc's ordinary
//! trait-bound diagnostic, including its "other types implementing Keyed"
//! hint list — that list grows as this crate gains more `Keyed` impls, so
//! this snapshot may need `TRYBUILD=overwrite cargo test -p rust_scene
//! --test trybuild` re-generation occasionally; only the "does this fail to
//! compile, and does the compile_error! wording stay clear" properties are
//! actually load-bearing for `OPERATORS.md`'s requirement.

#[test]
fn derive_operator_error_cases() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/non_keyed_field_without_hold.rs");
    t.compile_fail("tests/ui/name_collision_with_builtin.rs");
    t.compile_fail("tests/ui/default_argument_rejected.rs");
}
