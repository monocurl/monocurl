//! bytecode disassembler: prints every section's instructions with their
//! source spans, for inspecting what the compiler emits for a scene.
//!
//! usage: mcdump scene.mcs [--section N]

use std::path::PathBuf;

use scene_harness::{prepare, use_repo_assets};

fn main() {
    use_repo_assets();

    let mut only_section = None;
    let mut scene = None;
    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--section" => {
                only_section = argv.next().and_then(|value| value.parse::<usize>().ok());
            }
            other => scene = Some(PathBuf::from(other)),
        }
    }
    let scene = scene.expect("usage: mcdump scene.mcs [--section N]");
    let source = std::fs::read_to_string(&scene).expect("scene should be readable");
    let (executor, _) = match prepare(&source, &scene) {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    for (index, section) in executor.sections().iter().enumerate() {
        if only_section.is_some_and(|only| only != index) {
            continue;
        }
        if section.flags.is_stdlib && only_section.is_none() {
            continue;
        }
        println!(
            "== section {index} {:?} (library={}, init={}, root={}) ==",
            section.name, section.flags.is_library, section.flags.is_init, section.flags.is_root_module
        );
        for (offset, instr) in section.instructions.iter().enumerate() {
            let span = &section.annotations[offset].source_loc;
            println!("{offset:5}  {instr:?}    ; {}..{}", span.start, span.end);
        }
        if !section.lambda_prototypes.is_empty() {
            println!("-- lambda prototypes");
            for (index, proto) in section.lambda_prototypes.iter().enumerate() {
                println!("{index:5}  {proto:?}");
            }
        }
        if !section.float_pool.is_empty() {
            println!("-- floats {:?}", section.float_pool);
        }
        if !section.int_pool.is_empty() {
            println!("-- ints {:?}", section.int_pool);
        }
        if !section.string_pool.is_empty() {
            println!("-- strings {:?}", section.string_pool);
        }
    }
}
