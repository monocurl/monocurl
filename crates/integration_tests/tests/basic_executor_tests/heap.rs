use super::*;

fn assert_no_live_heap_growth(src: &str) {
    let baseline = with_heap(|heap| heap.live_slot_count());

    {
        let result = run(src);
        result.assert_ok();
    }

    assert_eq!(
        with_heap(|heap| heap.live_slot_count()),
        baseline,
        "live virtual heap slots should return to baseline after executor/result drop"
    );
}

#[test]
fn temporary_collection_slots_are_released_after_execution() {
    assert_no_live_heap_growth(
        r#"
        var xs = []
        var m = [->]
        var i = 0
        while (i < 40) {
            xs .= [i, [i + 1, i + 2]]
            m[[i, i + 1]] = xs[i][1][0]
            i = i + 1
        }
        let result = 0
    "#,
    );
}

#[test]
fn closure_allocated_slots_are_released_after_execution() {
    assert_no_live_heap_growth(
        r#"
        let make_pair = |base| [base, [base + 1, base + 2]]
        var acc = []
        var i = 0
        while (i < 40) {
            acc .= make_pair(i)
            i = i + 1
        }
        let result = 0
    "#,
    );
}

#[test]
fn retained_lvalue_reference_vectors_are_released_after_execution() {
    assert_no_live_heap_growth(
        r#"
        param p = 0
        mesh grid = [0, 1]

        let accept = |&refs| {
            return []
        }

        var i = 0
        while (i < 30) {
            accept([&p, &grid])
            grid = [i, i + 10]
            p = i
            i = i + 1
        }

        let result = 0
    "#,
    );
}

#[test]
fn freed_slots_are_reused_between_executor_runs() {
    let src = r#"
        var xs = []
        var i = 0
        while (i < 80) {
            xs .= [i, [i + 1, i + 2, i + 3]]
            i = i + 1
        }
        let result = 0
    "#;

    let baseline_live = with_heap(|heap| heap.live_slot_count());

    {
        let result = run(src);
        result.assert_ok();
    }

    assert_eq!(with_heap(|heap| heap.live_slot_count()), baseline_live);
    let high_water_mark = with_heap(|heap| heap.slot_count());

    {
        let result = run(src);
        result.assert_ok();
    }

    assert_eq!(with_heap(|heap| heap.live_slot_count()), baseline_live);
    assert_eq!(with_heap(|heap| heap.slot_count()), high_water_mark);
}
