use std::path::PathBuf;

// pub struct ImportResolver {}
//

pub struct Document {
    location: PathBuf,

    content_rope: String,
    content_stack: Vec<String>,

    lexing_attribute_rope: String,
    compiler_attribute_rope: String,

    parameter_state: Vec<String>,
    error_state: Vec<String>,
    autocompletion_state: Vec<String>,

    dirty: bool,

    // lexer: StatefulLexer,
    // parser: Parser,
    // compiler: Compiler,
    // executor: Executor,
}

// pub struct Document {

//     // each one of these components acts as a state machine, and uses
//     // channels to communicate information to other components
// }

// pub struct Executor {
//     import_resolver: ImportResolver,
//     state_cache: int,

//     runtime_error: String,
//     playing_state: u32,
//     timestamp: u32,
//     viewport_state: u32,
// }
