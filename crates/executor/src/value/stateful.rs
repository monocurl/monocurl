enum StatefulNode {
    Leader(),
    Constant(),
    BinaryOperator(),
    UnaryOperator()
}

pub struct Stateful {
    root: StatefulNode,
    dependencies: Vec<StatefulNode>
}
