//! `MeshValue`: the retained, lazily-evaluated, memoized structural tree
//! described in `API_DIRECTION.md` layer 1.
//!
//! `Clone` is cheap (an `Rc` bump) and shares memoized evaluation results —
//! this is the "copy-on-write" value semantics the design calls for: clones
//! are independent from the caller's point of view (nothing written through
//! one clone is visible through another, since nothing is ever mutated in
//! place through a shared `MeshValue`), while still being O(1) to produce and
//! able to share computed results.

use std::{cell::RefCell, rc::Rc};

use crate::{
    error::Result,
    keyed::Keyed,
    operator::{ConstructorNode, OperatorNode, OperatorStruct},
    vector::Vec3,
};

/// A concrete mesh shape. Deliberately tiny — see the module-level note in
/// `vector.rs`: this is not `crates/geo::Mesh`, just enough payload for the
/// operator/lerp machinery to move around and for tests to assert on.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Circle { radius: f64 },
}

/// A single concrete mesh leaf: a shape plus the transform operators like
/// `shift`/`squash` accumulate onto it.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaf {
    pub shape: Shape,
    pub center: Vec3,
    pub scale: Vec3,
}

impl Leaf {
    pub fn new(shape: Shape) -> Self {
        Self {
            shape,
            center: Vec3::ZERO,
            scale: Vec3::ONE,
        }
    }

    pub fn translated(&self, delta: Vec3) -> Self {
        Self {
            center: self.center + delta,
            ..self.clone()
        }
    }

    pub fn scaled(&self, factor: Vec3) -> Self {
        Self {
            scale: self.scale.scale_by(factor),
            ..self.clone()
        }
    }
}

impl Keyed for Leaf {
    fn lerp(&self, other: &Self, t: f64) -> Result<Self> {
        let shape = match (&self.shape, &other.shape) {
            (Shape::Circle { radius: ar }, Shape::Circle { radius: br }) => Shape::Circle {
                radius: ar + (br - ar) * t,
            },
        };
        Ok(Self {
            shape,
            center: self.center.lerp(&other.center, t)?,
            scale: self.scale.lerp(&other.scale, t)?,
        })
    }
}

pub(crate) struct ConstructorCell {
    pub(crate) node: Box<dyn ConstructorNode>,
    pub(crate) memo: RefCell<Option<MeshValue>>,
}

impl Clone for ConstructorCell {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone_node(),
            memo: RefCell::new(self.memo.borrow().clone()),
        }
    }
}

pub(crate) struct OperatorCell {
    pub(crate) node: Box<dyn OperatorNode>,
    pub(crate) operand: MeshValue,
    /// Memoized `(identity, modified)` endpoint pair, keyed by nothing beyond
    /// "has this been computed once" — the node/operand are immutable once
    /// constructed in the milestones implemented here (mutation via
    /// `slot_mut` is for `live!` wiring, milestone 5, out of scope).
    pub(crate) memo: RefCell<Option<(MeshValue, MeshValue)>>,
}

impl Clone for OperatorCell {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone_node(),
            operand: self.operand.clone(),
            memo: RefCell::new(self.memo.borrow().clone()),
        }
    }
}

pub(crate) enum Node {
    Leaf(Leaf),
    Group(Vec<MeshValue>),
    Constructor(ConstructorCell),
    Operator(OperatorCell),
}

/// The retained tree. See the module doc for `Clone` semantics.
#[derive(Clone)]
pub struct MeshValue(pub(crate) Rc<Node>);

impl MeshValue {
    pub fn leaf(leaf: Leaf) -> Self {
        Self(Rc::new(Node::Leaf(leaf)))
    }

    pub fn group(children: impl IntoIterator<Item = MeshValue>) -> Self {
        Self(Rc::new(Node::Group(children.into_iter().collect())))
    }

    pub fn from_constructor_node(node: Box<dyn ConstructorNode>) -> Self {
        Self(Rc::new(Node::Constructor(ConstructorCell {
            node,
            memo: RefCell::new(None),
        })))
    }

    /// Build a retained operator layer from an already-boxed node (used by
    /// `lerp.rs` rule 3, which produces a `Box<dyn OperatorNode>` from
    /// `OperatorNode::lerp_slots` rather than a concrete `Op: OperatorStruct`).
    pub(crate) fn from_operator_node(node: Box<dyn OperatorNode>, operand: MeshValue) -> Self {
        Self(Rc::new(Node::Operator(OperatorCell {
            node,
            operand,
            memo: RefCell::new(None),
        })))
    }

    /// Push an operator layer onto this value, retaining it in the tree (the
    /// entry point `#[derive(Operator)]`-generated chaining methods and
    /// `mesh.with(Wobble { .. })` both go through).
    pub fn with<Op: OperatorStruct>(self, op: Op) -> Self {
        Self(Rc::new(Node::Operator(OperatorCell {
            node: Box::new(op),
            operand: self,
            memo: RefCell::new(None),
        })))
    }

    /// Force this value down to a concrete tree of `Leaf`/`Group` nodes,
    /// evaluating and memoizing any constructor/operator layers along the
    /// way.
    pub fn evaluate(&self) -> MeshValue {
        match &*self.0 {
            Node::Leaf(_) | Node::Group(_) => self.clone(),
            Node::Constructor(cell) => {
                if let Some(cached) = cell.memo.borrow().as_ref() {
                    return cached.clone();
                }
                let result = cell.node.evaluate().evaluate();
                *cell.memo.borrow_mut() = Some(result.clone());
                result
            }
            Node::Operator(_) => {
                let (_, modified) = self.operator_endpoints_evaluated();
                modified
            }
        }
    }

    /// If this value is an operator layer, returns its memoized
    /// `(identity, modified)` endpoint pair with `modified` fully evaluated.
    /// `identity` is left as returned by the operator body (typically the
    /// evaluated operand, for the default `Apply`-based identity) since rule
    /// 4 only ever recurses `lerp` into it, which evaluates lazily as needed.
    fn operator_endpoints_evaluated(&self) -> (MeshValue, MeshValue) {
        match &*self.0 {
            Node::Operator(cell) => {
                if let Some(pair) = cell.memo.borrow().as_ref() {
                    return pair.clone();
                }
                let operand = cell.operand.evaluate();
                let (identity, modified) = cell.node.endpoints(operand);
                let modified = modified.evaluate();
                let pair = (identity, modified);
                *cell.memo.borrow_mut() = Some(pair.clone());
                pair
            }
            _ => (self.clone(), self.evaluate()),
        }
    }

    /// Number of consecutive retained operator layers at the top of the tree,
    /// without evaluating anything — the executor's `operator_count`, ported.
    pub(crate) fn operator_depth(&self) -> usize {
        match &*self.0 {
            Node::Operator(cell) => 1 + cell.operand.operator_depth(),
            _ => 0,
        }
    }

    pub(crate) fn as_operator(&self) -> Option<(&dyn OperatorNode, &MeshValue)> {
        match &*self.0 {
            Node::Operator(cell) => Some((cell.node.as_ref(), &cell.operand)),
            _ => None,
        }
    }

    pub(crate) fn as_constructor(&self) -> Option<&dyn ConstructorNode> {
        match &*self.0 {
            Node::Constructor(cell) => Some(cell.node.as_ref()),
            _ => None,
        }
    }

    /// If this (already-evaluated) value is a single concrete leaf, returns
    /// it. Meant for inspecting `evaluate()`'s output (tests, debugging);
    /// returns `None` for an unevaluated constructor/operator layer rather
    /// than forcing evaluation implicitly — call `.evaluate()` first.
    pub fn as_leaf(&self) -> Option<&Leaf> {
        match &*self.0 {
            Node::Leaf(leaf) => Some(leaf),
            _ => None,
        }
    }

    /// If this (already-evaluated) value is a group, returns its children.
    /// Same evaluation caveat as `as_leaf`.
    pub fn as_group(&self) -> Option<&[MeshValue]> {
        match &*self.0 {
            Node::Group(children) => Some(children),
            _ => None,
        }
    }

    /// Apply `f` to every concrete mesh leaf, forcing evaluation first and
    /// preserving group/list structure. The standard helper for per-mesh
    /// operator bodies.
    pub fn map_leaves(&self, f: impl Fn(&Leaf) -> Leaf) -> MeshValue {
        fn walk(value: &MeshValue, f: &impl Fn(&Leaf) -> Leaf) -> MeshValue {
            match &*value.0 {
                Node::Leaf(leaf) => MeshValue::leaf(f(leaf)),
                Node::Group(children) => {
                    MeshValue::group(children.iter().map(|child| walk(child, f)))
                }
                Node::Constructor(_) | Node::Operator(_) => walk(&value.evaluate(), f),
            }
        }
        walk(&self.evaluate(), &f)
    }

    /// Structural equality on the *evaluated* tree — used as the cheap
    /// short-circuit for lerp rule 1. Retained constructor/operator identity
    /// is compared separately in `lerp.rs` (rules 3/4) before evaluation is
    /// forced, so this only needs to handle `Leaf`/`Group`.
    pub(crate) fn evaluated_eq(a: &MeshValue, b: &MeshValue) -> bool {
        match (&*a.0, &*b.0) {
            (Node::Leaf(x), Node::Leaf(y)) => x == y,
            (Node::Group(x), Node::Group(y)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(x, y)| Self::evaluated_eq(x, y))
            }
            _ => false,
        }
    }
}

impl std::fmt::Debug for MeshValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &*self.0 {
            Node::Leaf(leaf) => f.debug_tuple("Leaf").field(leaf).finish(),
            Node::Group(children) => f.debug_tuple("Group").field(children).finish(),
            Node::Constructor(cell) => f
                .debug_tuple("Constructor")
                .field(&cell.node.debug_name())
                .finish(),
            Node::Operator(cell) => f
                .debug_tuple("Operator")
                .field(&cell.node.debug_name())
                .field(&cell.operand)
                .finish(),
        }
    }
}

// `impl Keyed for MeshValue` (the full structural lerp) lives in `lerp.rs` —
// milestone 2, not milestone 1's structural/retained-tree layer.
