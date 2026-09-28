//! the kernel tier against the interpreter: every scene here runs with the
//! tier off and on, and the two transcripts must match exactly. the scenes
//! that the tier is expected to take over also assert that it did, so a
//! translator regression that silently sends everything back to the
//! interpreter fails loudly instead of only showing up in benchmarks

use std::path::Path;

use executor::{executor::SeekOptions, kernel::KernelMode};
use scene_harness::{run_scene_with_kernels, use_repo_assets};

const PRELUDE: &str = "
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene
";

struct Outcome {
    transcript: Vec<String>,
    errors: Vec<String>,
    kernel_calls: usize,
    single_calls: usize,
    faults: usize,
}

fn run(body: &str, mode: KernelMode) -> Outcome {
    use_repo_assets();
    let source = format!("{PRELUDE}\n{body}");
    let run = run_scene_with_kernels(
        &source,
        Path::new("kernel_tier_test.mcs"),
        SeekOptions::strict(),
        Some(mode),
    )
    .unwrap_or_else(|error| panic!("scene failed to prepare: {error}\n{source}"));
    Outcome {
        transcript: run.transcript,
        errors: run.runtime_errors,
        kernel_calls: run.kernel_stats.calls,
        single_calls: run.kernel_stats.single_calls,
        faults: run.kernel_stats.faults,
    }
}

/// what the tier is expected to do with a scene
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// at least one batch runs as a kernel and none faults
    Kernels,
    /// at least one interpreted call ran as a kernel on its own
    SingleCalls,
    /// the tier runs and then hands the batch back
    Fault,
    /// no opinion; only agreement is checked
    Any,
}

fn check(body: &str, expect: Expect) {
    let off = run(body, KernelMode::Off);
    let on = run(body, KernelMode::On);
    assert_eq!(
        off.transcript, on.transcript,
        "kernel tier changes the transcript of:\n{body}"
    );
    assert_eq!(
        off.errors, on.errors,
        "kernel tier changes the runtime errors of:\n{body}"
    );
    assert_eq!(off.kernel_calls, 0, "kernels ran with the tier off");
    match expect {
        Expect::Kernels => {
            assert!(on.kernel_calls > 0, "no kernel ran for:\n{body}");
            assert_eq!(on.faults, 0, "a kernel faulted for:\n{body}");
        }
        Expect::SingleCalls => {
            assert!(
                on.single_calls > 0,
                "no single call ran as a kernel for:\n{body}"
            );
            assert_eq!(on.faults, 0, "a kernel faulted for:\n{body}");
        }
        Expect::Fault => assert!(on.faults > 0, "no kernel faulted for:\n{body}"),
        Expect::Any => {}
    }
    // verify mode cross-checks call by call and panics on any disagreement
    let verified = run(body, KernelMode::Verify);
    assert_eq!(verified.transcript, off.transcript);
}

#[test]
fn sampled_scalar_function_with_loop() {
    check(
        "
        let n = 30
        let ws = |a, b, x| {
            var sum = 0
            for (i in range(0, n)) {
                sum = sum + (a ^ i) * cos((b ^ i) * x * PI)
            }
            return sum
        }
        mesh w = ExplicitFunc(|x| ws(0.5, 3, x), [-2, 2, 257])
        print w
        ",
        Expect::Kernels,
    );
}

#[test]
fn sampled_surface_with_colour_callback() {
    check(
        "
        let escape = |cx, cy| {
            var zx = 0.0
            var zy = 0.0
            var i = 0
            while (i < 10 and zx * zx + zy * zy < 4) {
                let nx = zx * zx - zy * zy + cx
                zy = 2 * zx * zy + cy
                zx = nx
                i = i + 1
            }
            return i / 10
        }
        mesh surface = ExplicitFunc2d(
            |x, y| 0.3 * escape(x - 0.5, y),
            [-2, 1, 40],
            [-1, 1, 30],
            |x, y, z| [z, 1 - z, 0.5 + 0.5 * sin(z * 9), 1]
        )
        print surface
        ",
        Expect::Kernels,
    );
}

#[test]
fn int_and_float_results_keep_their_types() {
    check(
        "
        let f = |x| [x * 2, x * 2.0, x / 2, x // 2, -x // 2, -7 // 2, 7 // -2, x // -2, sign(x * 0.0), sign(-x * 0.0), floor(x * 1.5), round(x / 3),
                     min(x, 3), max(x, 2.5), abs(-x), sign(x - 2), mod(x, 3), mod(-x, 3),
                     mod(x * 0.5, 2), x ^ 2, 2 ^ x, x < 2, x <= 2, x == 2, x != 2.0,
                     not x, to_int(x * 1.7), to_float(x), trunc(-x * 0.7), ceil(x * 0.3)]
        let samples = [0, 1, 2, 3, 0.5, 1.5, 2.5]
        mesh field = Field(|pos, idx| block { print f(idx[0]) }, [0, 1, 7], [0, 1, 1])
        print f(3)
        print f(2.5)
        ",
        Expect::Kernels,
    );
}

#[test]
fn list_arithmetic_and_indexing() {
    check(
        "
        let f = |p| {
            let a = p - [0.5, 0.5, 0]
            let b = 2 * a + a * 0.5 - [1, 2, 3] / 2
            let n = norm(b)
            let c = normalize(b + [1, 0, 0])
            let m = [[1, 2], [3, 4.5]]
            return [dot(a, b), n, c[0], c[1], c[2], m[1][0], m[0][1], len(m), len(p), -a[1],
                    cross(a, [0, 0, 1])[0], 2 in p, p[0] in [p[0]], [a[0], a[1]] == [a[0], a[1]]]
        }
        mesh field = Field(|pos, idx| block { print f(pos) }, [-1, 1, 5], [-1, 1, 3])
        ",
        Expect::Kernels,
    );
}

#[test]
fn recursion_closures_and_defaults() {
    check(
        "
        let fib = |self, n| {
            if (n < 2) { return n }
            return self(self, n - 1) + self(self, n - 2)
        }
        let scale = |k| |x| x * k
        let triple = scale(3)
        let with_default = |x, y = 10, z = 0.5| x + y * z
        let f = |x| [fib(fib, 8), triple(x), with_default(x), with_default(x, 2), with_default(x, 2, 0.25),
                     with_default(x) == x + 5, [with_default(x)] * 2, abs(with_default(x))]
        mesh w = ExplicitFunc(|x| f(x)[1] + f(x)[0], [0, 1, 65])
        mesh field = Field(|pos, idx| block { print f(pos[0]) }, [0, 1, 4], [0, 1, 1])
        print w
        ",
        Expect::Kernels,
    );
}

#[test]
fn list_building_in_loops() {
    check(
        "
        let squares = |n| {
            var out = []
            var k = 0
            while (k < n) {
                out .= k * k
                k = k + 1
            }
            return out
        }
        let total = |v| {
            var s = 0
            for (x in v) { s = s + x }
            return s
        }
        let f = |x| total(squares(to_int(x))) + sum(squares(4)) + len(squares(3))
        mesh field = Field(|pos, idx| block { print f(idx[0] + idx[1]) }, [0, 1, 6], [0, 1, 3])
        ",
        Expect::Kernels,
    );
}

#[test]
fn short_circuit_and_branches() {
    check(
        "
        let classify = |x| {
            if (x > 0.5 and x < 2 or not (x == 3)) {
                if (x <= 0) { return -1 }
                return 1
            } else if (x == 3) {
                return 3
            }
            return 0
        }
        mesh field = Field(|pos, idx| block { print [classify(idx[0]), classify(pos[0])] }, [-1, 4, 6], [0, 1, 1])
        ",
        Expect::Kernels,
    );
}

#[test]
fn division_by_zero_inside_a_sampled_lambda_reports_the_same_error() {
    check(
        "
        mesh w = ExplicitFunc(|x| 1 / (x - 0.5), [0, 1, 3])
        print w
        ",
        Expect::Fault,
    );
}

#[test]
fn out_of_range_index_inside_a_sampled_lambda_reports_the_same_error() {
    check(
        "
        let table = [1, 2, 3]
        mesh w = ExplicitFunc(|x| table[to_int(x * 10)], [0, 1, 5])
        print w
        ",
        Expect::Fault,
    );
}

#[test]
fn type_error_inside_a_sampled_lambda_reports_the_same_error() {
    check(
        "
        let text = \"hello\"
        mesh w = ExplicitFunc(|x| x + text, [0, 1, 5])
        print w
        ",
        Expect::Fault,
    );
}

#[test]
fn unused_opaque_captures_do_not_stop_the_kernel() {
    check(
        "
        let text = \"hello\"
        let palette = [0 -> RED, 1 -> BLUE]
        let unused = |x| [text, palette, x]
        let f = |x| {
            if (x > 100) { return len(text) + len(unused(x)) }
            return x * 2
        }
        mesh w = ExplicitFunc(|x| f(x), [0, 1, 33])
        print w
        ",
        Expect::Kernels,
    );
}

#[test]
fn mesh_building_callbacks_stay_in_the_interpreter() {
    check(
        "
        mesh field = Field(|pos, idx| center{pos} Circle(0.1), [-1, 1, 4], [-1, 1, 3])
        print field
        ",
        Expect::Any,
    );
}

#[test]
fn point_and_colour_maps() {
    check(
        "
        mesh grid = LineGrid([-1, 1, 5], [-1, 1, 5], 1)
        mesh lifted = point_map{|p| [p[0], p[1], p[0] * p[1]]} grid
        mesh shaded = color_map{|c| [c[0] * 0.5, c[1], c[2], 1]} lifted
        print [grid, lifted, shaded]
        ",
        Expect::Kernels,
    );
}

#[test]
fn deep_recursion_falls_back_without_changing_the_answer() {
    check(
        "
        let count = |self, n| {
            if (n <= 0) { return 0 }
            return 1 + self(self, n - 1)
        }
        mesh w = ExplicitFunc(|x| count(count, 700) + x, [0, 1, 3])
        print w
        ",
        Expect::Fault,
    );
}

#[test]
fn stateful_parameters_feeding_a_sampled_lambda() {
    check(
        "
        let Wave = |amp, freq|
            ExplicitFunc(|x| amp * sin(freq * x), [-3, 3, 129])
        mesh w = Wave(amp: 1, freq: 2)
        slide \"a\"
            w.amp = 0.25
            w.freq = 5
            play Lerp(0.5)
            print w
        ",
        Expect::Kernels,
    );
}

#[test]
fn higher_order_std_helpers_run_as_single_calls() {
    check(
        "
        let base = range(0, 500)
        print sum(map(base, |x| x * 2))
        print len(filter(base, |x| mod(x, 3) == 0))
        print reduce(base, 0, |acc, x| acc + x)
        print [product([1, 2, 3, 4]), any([0, 0, 1]), all([1, 1, 0]), count(base, |x| x > 400)]
        var nested = []
        for (i in range(0, 20)) {
            var row = []
            for (j in range(0, 5)) {
                row .= i * j
            }
            nested .= row
        }
        print sum(map(nested, sum))
        print map([1, 2.5, 3], |x| [x, x * 2])
        ",
        Expect::SingleCalls,
    );
}

#[test]
fn recursive_top_level_call_runs_as_a_single_call() {
    check(
        "
        let fib = |self, n| {
            if (n < 2) {
                return n
            }
            return self(self, n - 1) + self(self, n - 2)
        }
        print fib(fib, 18)
        ",
        Expect::SingleCalls,
    );
}

#[test]
fn single_call_faults_leave_the_interpreter_result_unchanged() {
    check(
        "
        let text = \"x\"
        let walk = |v| {
            var s = 0
            for (x in v) { s = s + x }
            return s + text
        }
        print walk([1, 2, 3])
        ",
        Expect::Fault,
    );
}

#[test]
fn large_captured_lists_are_not_converted_per_call() {
    check(
        "
        let table = range(0, 200)
        let lookup = |i| {
            var s = 0
            for (k in range(0, 3)) { s = s + table[i + k] }
            return s
        }
        var total = 0
        for (i in range(0, 100)) { total = total + lookup(i) }
        print total
        ",
        Expect::Any,
    );
}

#[test]
fn returning_a_capture_or_argument_survives_repeated_calls() {
    check(
        "
        let white = [1, 1, 1, 1]
        mesh grid = LineGrid([-1, 1, 3], [-1, 1, 3], 1)
        mesh a = color_map{|c| white} grid
        mesh b = color_map{|c| c} grid
        mesh c = point_map{|p| p} grid
        print [a, b, c]
        ",
        Expect::Kernels,
    );
}

#[test]
fn shader_pixels_come_from_the_kernel_tier() {
    check(
        "
        let escape = |cx, cy| {
            var zx = 0.0
            var zy = 0.0
            var i = 0
            while (i < 8 and zx * zx + zy * zy < 4) {
                let nx = zx * zx - zy * zy + cx
                zy = 2 * zx * zy + cy
                zx = nx
                i = i + 1
            }
            return i / 8
        }
        mesh plasma = Shader(|x, y| [escape(x, y), 0.5 + 0.5 * sin(3 * x), 0.5 + 0.5 * cos(2 * y), 1], [-2, 1], [-1, 1], 48)
        mesh flat = Shader(|x, y| [1, 0, 0, 1])
        print [plasma, flat]
        ",
        Expect::Kernels,
    );
}

#[test]
fn closures_created_inside_a_sampled_lambda_fall_back() {
    check(
        "
        let scale = |k| |x| x * k
        mesh w = ExplicitFunc(|x| scale(2)(x) + scale(x)(3), [0, 1, 17])
        print w
        ",
        Expect::Fault,
    );
}

#[test]
fn block_expressions_are_translated_inline() {
    check(
        "
        let classify = |x, y| block {
            let z = x * 2 + y
            if (z > 3) { return [z, 1] }
            let w = block {
                var acc = 0
                for (i in range(0, 4)) { acc = acc + i * z }
                return acc
            }
            return [z, w]
        }
        let accumulate = |n| {
            var out = []
            let pushed = block {
                out .= n
                out .= n * 2
                return len(out)
            }
            return [out, pushed]
        }
        let nested = |x| block {
            let inner = block {
                let deeper = block { return x + 1 }
                return deeper * 2
            }
            return inner - x
        }
        mesh field = Field(|pos, idx| block { print [classify(pos[0], pos[1]), accumulate(idx[0]), nested(idx[1])] }, [-1, 1, 4], [-1, 1, 3])
        mesh w = ExplicitFunc(|x| block { let s = sin(x); return s * s }, [0, 1, 33])
        print w
        ",
        Expect::Kernels,
    );
}
