use super::RcValue;

/// a stateful value node that tracks reactive dependencies.
/// when a follower depends on `$x`, the stateful graph records
/// how to recompute the follower when x's follower value changes.
#[derive(Clone)]
pub enum StatefulNode {
    /// references a leader's follower value
    LeaderRef(RcValue),
    /// a constant value embedded in the graph
    Constant(Box<super::Value>),
    /// binary operation on two sub-nodes
    BinaryOp {
        op: StatefulOp,
        lhs: Box<StatefulNode>,
        rhs: Box<StatefulNode>,
    },
    /// unary operation on a sub-node
    UnaryOp {
        op: StatefulOp,
        operand: Box<StatefulNode>,
    },
    /// function application: node for the function, nodes for arguments
    FunctionApp {
        func: Box<StatefulNode>,
        args: Vec<StatefulNode>,
    },
}

#[derive(Clone, Copy)]
pub enum StatefulOp {
    Add,
    Sub,
    Mul,
    Div,
    Negate,
    Not,
}

#[derive(Clone)]
pub struct Stateful {
    pub root: StatefulNode,
}
