//! differential fuzzer: generated programs run through the real pipeline and the
//! reference evaluator must print the same transcript and fail the same way.
//!
//! usage: mcfuzz [--batches] [--seed N] [--count N] [--print] [--stats] [--keep-going]
//!
//! runs `count` consecutive seeds starting at `seed` (default: seed 0, 1000
//! cases; `--seed` alone runs just that seed) and exits nonzero on the first
//! mismatch. `--print` shows each program and its transcript, `--stats`
//! summarises why candidate programs were discarded before one was accepted.
//!
//! `--batches` also samples each program's lambdas through batch
//! constructors and runs it with the kernel tier off, on and verifying; any
//! verify panic or off/on difference is a mismatch. `--keep-going` reports
//! every mismatching seed instead of stopping at the first.

use std::{collections::BTreeMap, process::ExitCode};

use executor::kernel::KernelStats;
use scene_harness::{
    fuzz::{check_batch_seed, check_seed, use_serial_kernels},
    use_repo_assets,
};

const USAGE: &str =
    "usage: mcfuzz [--batches] [--seed N] [--count N] [--print] [--stats] [--keep-going]";

fn main() -> ExitCode {
    let mut seed = 0u64;
    let mut count = None;
    let mut print = false;
    let mut stats = false;
    let mut batches = false;
    let mut keep_going = false;

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
            "--batches" => batches = true,
            "--keep-going" => keep_going = true,
            other => panic!("unknown argument {other}; {USAGE}"),
        }
    }
    let count = count.unwrap_or(1000);

    use_repo_assets();
    if batches {
        use_serial_kernels();
        return run_batches(seed, count, print, stats, keep_going);
    }

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

fn run_batches(first: u64, count: u64, print: bool, stats: bool, keep_going: bool) -> ExitCode {
    let mut mismatches = Vec::new();
    let mut errors = 0;
    let mut totals = KernelStats::default();
    let mut typed_cases = 0;
    let mut discarded: BTreeMap<&str, usize> = BTreeMap::new();
    for seed in first..first + count {
        let case = check_batch_seed(seed);
        if print {
            println!(
                "=== seed {seed} ===\n{}--- transcript ---\n{}\n",
                case.source, case.off.outcome
            );
        }
        if !case.matches() {
            eprintln!("{}", case.report());
            mismatches.push(seed);
            if !keep_going {
                return ExitCode::FAILURE;
            }
        }
        errors += usize::from(case.off.outcome.error.is_some());
        let on = case.on.stats;
        typed_cases += usize::from(on.typed_calls > 0);
        totals.batches += on.batches;
        totals.calls += on.calls;
        totals.single_calls += on.single_calls;
        totals.typed_calls += on.typed_calls;
        totals.lane_calls += on.lane_calls;
        totals.jit_calls += on.jit_calls;
        totals.typed_declined += on.typed_declined;
        totals.faults += on.faults;
        totals.rejected_bodies += on.rejected_bodies;
        for reason in &case.discarded {
            *discarded.entry(reason).or_default() += 1;
        }
    }

    let agreeing = count as usize - mismatches.len();
    println!("{agreeing} of {count} batch cases agree ({errors} ending in a runtime error)");
    if stats {
        println!(
            "kernels on: {} batches, {} calls ({} typed, {} on lanes, {} native), {} single calls, \
             {} typed declines, {} faults, {} rejected bodies; {typed_cases} cases ran typed",
            totals.batches,
            totals.calls,
            totals.typed_calls,
            totals.lane_calls,
            totals.jit_calls,
            totals.single_calls,
            totals.typed_declined,
            totals.faults,
            totals.rejected_bodies,
        );
        let total: usize = discarded.values().sum();
        println!("{total} candidates discarded along the way:");
        for (reason, times) in discarded {
            println!("  {times:>7}  {reason}");
        }
    }
    if mismatches.is_empty() {
        ExitCode::SUCCESS
    } else {
        println!("mismatching seeds: {mismatches:?}");
        ExitCode::FAILURE
    }
}
