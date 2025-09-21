use std::path::PathBuf;

pub enum TimestampState {
    Paused,
    Play,
}

pub struct ImportResolver {}

pub struct Document {
    location: PathBuf,

    // each one of these components acts as a state machine, and uses
    // channels to communicate information to other components
    lexer: Lexer,
    parser: Parser,
    compiler: Compiler,
    autocompletor: AutoCompletor,
    executor: Executor,
}

pub struct Executor {
    import_resolver: ImportResolver,
    state_cache: int,

    runtime_error: String,
    playing_state: u32,
    timestamp: u32,
    viewport_state: u32,
}
