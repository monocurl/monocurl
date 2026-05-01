#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StaticAnalysisData {
    #[default]
    None,
    FunctionInvocation,
    OperatorInvocation,
}

impl StaticAnalysisData {
    pub const fn class_name(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::FunctionInvocation => Some("mc-function-invocation"),
            Self::OperatorInvocation => Some("mc-operator-invocation"),
        }
    }
}
