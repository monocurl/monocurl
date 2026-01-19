use structs::text::Span8;

pub enum Expr {

}

pub enum ASTNode {
    Expr {

    },
    Declaration {

    },
    Assignment {

    },
    While {

    },
    For {

    },
    If {

    },
    Else {

    }
}

pub struct TaggedASTNode {
    pub node: ASTNode,
    pub span: Span8
}
