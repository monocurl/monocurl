use std::{path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use structs::text::Span8;

pub const MONOCURL_VERSION: &str = env!("MONOCURL_VERSION");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LambdaPrototype {
    pub section: u16,
    pub ip: u32,
    pub required_args: u32,
    pub default_arg_count: u32,
    pub reference_args: Vec<bool>,
    pub arg_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimPrototype {
    pub section: u16,
    pub ip: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CopyValueMode {
    Read,
    Reference,
    Raw,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instruction {
    /* push constants */
    PushNil,
    PushInt {
        index: u32,
    },
    PushFloat {
        index: u32,
    },
    // pushes complex(0, float_pool[index])
    PushImaginary {
        index: u32,
    },
    PushChar {
        char: char,
    },
    PushString {
        index: u32,
    },
    PushEmptyMap,
    PushEmptyList,

    // register tos as a leader; name_index into the string pool for debugging
    ConvertParam {
        name_index: u32,
    },
    ConvertMesh {
        name_index: u32,
    },
    ConvertVar {
        allow_stateful: bool,
    },
    // sync all leader followers to their leader values; emitted at end of init section
    SyncAllLeaders,

    PushDeepCopy {
        stack_delta: i32,
    },
    // pops old tos if flag is true
    // used for map
    PushCopy {
        copy_mode: CopyValueMode,
        pop_tos: bool,
        stack_delta: i32,
    },
    PushLvalue {
        force_ephemeral: bool,
        stack_delta: i32,
    },

    PushStateful {
        stack_delta: i32,
    },

    // u16::MAX indicates unlabeled
    BufferLabelOrAttribute {
        string_index: u32,
    },

    // pops capture_count captured values + prototype.default_arg_count default values, pushes lambda
    MakeLambda {
        capture_count: u16,
        prototype_index: u32,
    },
    // pops capture_count captured values, pushes anim
    MakeAnim {
        capture_count: u16,
        prototype_index: u32,
    },
    // pops a lambda, pushes an operator wrapping it
    MakeOperator,

    OperatorInvoke {
        stateful: bool,
        labeled: bool,
        num_args: u32,
    },
    LambdaInvoke {
        stateful: bool,
        labeled: bool,
        num_args: u32,
    },
    // pops the operator lambda result ([initial, modified] list) and pushes the live value.
    // for labeled invocations the InvokedOperator is already on stack; this is a no-op then.
    ConvertToLiveOperator,
    Jump {
        section: u16,
        to: u32,
    },
    // pops TOS; jumps when truthy
    ConditionalJump {
        section: u16,
        to: u32,
    },
    Return {
        stack_delta: i32,
    },
    Pop {
        count: u32,
    },

    NativeInvoke {
        index: u16,
        arg_count: u16,
    },

    IncrementByOne {
        stack_delta: i32,
    },

    Play,
    Observe,

    /* unary */
    Negate,
    Not,

    Subscript {
        mutable: bool,
    },
    Attribute {
        mutable: bool,
        string_index: u32,
    },

    /* binary (pop 2, push 1) */
    Add,
    Sub,
    Mul,
    Div,
    Power,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    IntDiv,
    In,
    Assign,
    AppendAssign,
    Append,

    EndOfExecutionHead,
}
const _: () = assert!(std::mem::size_of::<Instruction>() == 8);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstructionAnnotation {
    pub source_loc: Span8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SectionFlags {
    pub is_stdlib: bool,
    pub is_library: bool,
    pub is_init: bool,
    pub is_root_module: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SectionBytecode {
    pub flags: SectionFlags,
    pub name: Option<String>,
    pub source_file_name: Option<String>,
    pub source_file_path: Option<PathBuf>,
    pub import_display_index: Option<usize>,
    pub instructions: Vec<Instruction>,
    pub annotations: Vec<InstructionAnnotation>,
    pub int_pool: Vec<i64>,
    pub float_pool: Vec<f64>,
    pub string_pool: Vec<String>,
    pub lambda_prototypes: Vec<LambdaPrototype>,
    pub anim_prototypes: Vec<AnimPrototype>,
}

impl SectionBytecode {
    pub fn new(flags: SectionFlags) -> Self {
        Self {
            flags,
            name: None,
            source_file_name: None,
            source_file_path: None,
            import_display_index: None,
            instructions: Vec::new(),
            annotations: Vec::new(),
            int_pool: Vec::new(),
            float_pool: Vec::new(),
            string_pool: Vec::new(),
            lambda_prototypes: Vec::new(),
            anim_prototypes: Vec::new(),
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bytecode {
    pub sections: Vec<Arc<SectionBytecode>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VersionedBytecode {
    pub monocurl_version: String,
    pub bytecode: Bytecode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BytecodeVersionError {
    pub expected: &'static str,
    pub found: String,
}

impl std::fmt::Display for BytecodeVersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bytecode version mismatch: expected Monocurl {}, found {}",
            self.expected, self.found
        )
    }
}

impl std::error::Error for BytecodeVersionError {}

#[derive(Debug)]
pub enum BytecodeJsonError {
    Json(serde_json::Error),
    Version(BytecodeVersionError),
}

impl std::fmt::Display for BytecodeJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(f, "failed to decode bytecode json: {error}"),
            Self::Version(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for BytecodeJsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Version(error) => Some(error),
        }
    }
}

impl From<serde_json::Error> for BytecodeJsonError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl Bytecode {
    pub fn new(sections: Vec<Arc<SectionBytecode>>) -> Self {
        assert!(!sections.is_empty());
        Self { sections }
    }

    pub fn library_sections(&self) -> usize {
        self.sections
            .iter()
            .take_while(|s| s.flags.is_library)
            .count()
    }

    pub fn non_slide_sections(&self) -> usize {
        self.sections
            .iter()
            .take_while(|s| s.flags.is_library || s.flags.is_init)
            .count()
    }

    pub fn to_versioned_json(&self) -> Result<String, serde_json::Error> {
        VersionedBytecode::new(self.clone()).to_json()
    }

    pub fn from_versioned_json(json: &str) -> Result<Self, BytecodeJsonError> {
        VersionedBytecode::from_json(json)?
            .into_bytecode()
            .map_err(BytecodeJsonError::Version)
    }
}

impl VersionedBytecode {
    pub fn new(bytecode: Bytecode) -> Self {
        Self {
            monocurl_version: MONOCURL_VERSION.to_string(),
            bytecode,
        }
    }

    pub fn check_version(&self) -> Result<(), BytecodeVersionError> {
        if self.monocurl_version == MONOCURL_VERSION {
            Ok(())
        } else {
            Err(BytecodeVersionError {
                expected: MONOCURL_VERSION,
                found: self.monocurl_version.clone(),
            })
        }
    }

    pub fn into_bytecode(self) -> Result<Bytecode, BytecodeVersionError> {
        self.check_version()?;
        Ok(self.bytecode)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bytecode() -> Bytecode {
        let mut section = SectionBytecode::new(SectionFlags {
            is_stdlib: false,
            is_library: false,
            is_init: true,
            is_root_module: true,
        });
        section.name = Some("init".to_string());
        section.instructions = vec![
            Instruction::PushInt { index: 0 },
            Instruction::Observe,
            Instruction::EndOfExecutionHead,
        ];
        section.annotations = vec![
            InstructionAnnotation { source_loc: 0..1 },
            InstructionAnnotation { source_loc: 1..2 },
            InstructionAnnotation { source_loc: 2..3 },
        ];
        section.int_pool = vec![42];
        Bytecode::new(vec![Arc::new(section)])
    }

    #[test]
    fn versioned_json_roundtrips_bytecode() {
        let bytecode = sample_bytecode();
        let json = bytecode.to_versioned_json().unwrap();
        let decoded = Bytecode::from_versioned_json(&json).unwrap();

        assert!(decoded == bytecode);
    }

    #[test]
    fn versioned_json_rejects_mismatched_version() {
        let versioned = VersionedBytecode {
            monocurl_version: "other-version".to_string(),
            bytecode: sample_bytecode(),
        };
        let json = versioned.to_json().unwrap();
        let error = Bytecode::from_versioned_json(&json).unwrap_err();

        assert!(matches!(
            error,
            BytecodeJsonError::Version(BytecodeVersionError { found, .. })
                if found == "other-version"
        ));
    }
}
