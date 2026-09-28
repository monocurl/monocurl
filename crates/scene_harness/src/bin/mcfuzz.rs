//! differential fuzzer: generated programs run through the real pipeline and the
//! reference evaluator must print the same transcript and fail the same way.
//!
//! usage: mcfuzz [--seed N] [--count N] [--print] [--stats]
//!
//! runs `count` consecutive seeds starting at `seed` (default: seed 0, 1000
//! cases; `--seed` alone runs just that seed) and exits nonzero on the first
//! mismatch. `--print` shows each program and its transcript, `--stats`
//! summarises why candidate programs were discarded before one was accepted.

use std::{collections::BTreeMap, process::ExitCode};

use scene_harness::{fuzz::check_seed, use_repo_assets};

const USAGE: &str = "usage: mcfuzz [--seed N] [--count N] [--print] [--stats]";

fn main() -> ExitCode {
    let mut seed = 0u64;
    let mut count = None;
    let mut print = false;
    let mut stats = false;

    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        let mut number = || -> u64 {
            argv.next()
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(|| panic!("{arg} expects a number; {USAGE}"))
        };
        match arg.as_str() {
            "--seed" => {
                seed = number();
                count = count.or(Some(1));
            }
            "--count" => count = Some(number()),
            "--print" => print = true,
            "--stats" => stats = true,
            other => panic!("unknown argument {other}; {USAGE}"),
        }
    }
    let count = count.unwrap_or(1000);

    use_repo_assets();

    let mut errors = 0;
    let mut discarded: BTreeMap<&str, usize> = BTreeMap::new();
    for seed in seed..seed + count {
        let case = check_seed(seed);
        if print {
            println!(
                "=== seed {seed} ===\n{}--- transcript ---\n{}\n",
                case.source, case.actual
            );
        }
        if !case.matches() {
            eprintln!("{}", case.report());
            return ExitCode::FAILURE;
        }
        errors += usize::from(case.expected.error.is_some());
        for reason in &case.discarded {
            *discarded.entry(reason).or_default() += 1;
        }
    }

    println!("{count} cases agree ({errors} ending in a runtime error)");
    if stats {
        let total: usize = discarded.values().sum();
        println!("{total} candidates discarded along the way:");
        for (reason, times) in discarded {
            println!("  {times:>7}  {reason}");
        }
    }
    ExitCode::SUCCESS
}
