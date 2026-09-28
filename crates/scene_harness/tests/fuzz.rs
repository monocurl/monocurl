//! differential fuzzing of the executor against the reference evaluator in
//! `scene_harness::fuzz`. runs `MONOCURL_FUZZ_CASES` consecutive seeds (default
//! 300) from a fixed base, plus pinned regression seeds. reproduce a failure
//! with `mcfuzz --seed N --print`. batch mode runs `MONOCURL_FUZZ_BATCH_CASES`
//! seeds (default 600, about 12s in release) with the kernel tier off, on and
//! verifying; reproduce with `mcfuzz --batches --seed N --print`.

use scene_harness::{
    fuzz::{check_batch_seed, check_seed, use_serial_kernels},
    use_repo_assets,
};

const BASE_SEED: u64 = 0x5eed_0000;

/// seeds that once exposed a problem (in the executor or the reference) or
/// that cover a shape worth keeping an eye on. 16 and 127 hit FUZZ_FINDINGS.md
/// #3 and #1 before the reference learned to steer around them; 195920 needs
/// `lerp` of two calls to one lambda to interpolate arguments; 198044 needs call
/// arguments evaluated before the callee; 4 and 9 end in
/// deliberate runtime errors
const PINNED_SEEDS: &[u64] = &[4, 9, 16, 127, 195_920, 198_044];

fn check(seeds: impl IntoIterator<Item = u64>) {
    use_repo_assets();
    let failures: Vec<String> = seeds
        .into_iter()
        .map(check_seed)
        .filter(|case| !case.matches())
        .map(|case| case.report())
        .take(3)
        .collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn generated_programs_agree_with_reference() {
    let cases = std::env::var("MONOCURL_FUZZ_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(300u64);
    check(BASE_SEED..BASE_SEED + cases);
}

#[test]
fn pinned_seeds_agree_with_reference() {
    check(PINNED_SEEDS.iter().copied());
}

const BATCH_BASE_SEED: u64 = 0xba7c_0000;

#[test]
fn batch_programs_agree_across_kernel_modes() {
    use_repo_assets();
    use_serial_kernels();
    let cases = std::env::var("MONOCURL_FUZZ_BATCH_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(600u64);
    let failures: Vec<String> = (BATCH_BASE_SEED..BATCH_BASE_SEED + cases)
        .map(check_batch_seed)
        .filter(|case| !case.matches())
        .map(|case| case.report())
        .take(3)
        .collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
