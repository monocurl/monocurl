use schemars::JsonSchema;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, JsonSchema)]
pub enum DocumentType {
    #[default]
    Scene,
    Library,
}

impl DocumentType {
    pub const fn extension(&self) -> &'static str {
        match self {
            DocumentType::Scene => "mcs",
            DocumentType::Library => "mcl",
        }
    }

    pub const fn default_file(&self) -> &'static str {
        match self {
            DocumentType::Scene => {
                "import std.util\nimport std.math\nimport std.color\nimport std.mesh\nimport std.anim\nimport std.scene\n\n"
            }
            DocumentType::Library => {
                "import std.util\nimport std.math\nimport std.color\nimport std.mesh\nimport std.anim\nimport std.scene\n\n"
            }
        }
    }
}
