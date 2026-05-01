use std::ffi::OsString;

use command::CliAction;

mod command;
mod execute;
mod help;
mod parse;
mod progress;
mod style_trusted_html;

pub fn run(args: Vec<OsString>) -> i32 {
    clean_latex_file_cache();

    match parse::parse_cli(args) {
        Ok(CliAction::Help(topic)) => {
            println!("{}", help::help_text(topic));
            0
        }
        Ok(CliAction::Run(command)) => {
            if let Err(error) = execute::run_command(command) {
                eprintln!("error: {error:#}");
                1
            } else {
                0
            }
        }
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!();
            eprintln!("Use `monocurl help` for usage.");
            2
        }
    }
}

fn clean_latex_file_cache() {
    if let Err(error) = latex::clean_stale_file_cache() {
        log::warn!("unable to clean stale LaTeX SVG cache: {error:#}");
    }
}
