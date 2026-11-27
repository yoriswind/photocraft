//! Constrain Path Dragging (#13214): measured curve and circle results, with the existing
//! straight, retracted-handle and near-anchor behavior kept.

use super::*;

fn curve() -> Path {
    Path::new(vec![Subpath {
        closed: false,
        knots: vec![
            Knot::smooth(Point::new(400.0, 450.0), Point::new(300.0, 550.0), Point::new(500.0, 350.0)),
            Knot::smooth(Point::new(700.0, 450.0), Point::new(600.0, 550.0), Point::new(800.0, 350.0)),
        ],
        op: PathOp::Combine,
    }])
}

fn circle() -> Path {
    let mut sp = Subpath::polygon(&[(430.0, 70.0), (510.0, 150.0), (430.0, 230.0), (350.0, 150.0)]);
    let controls = [
        ((385.82, 70.0), (474.18, 70.0)),
        ((510.0, 105.82), (510.0, 194.18)),
        ((474.18, 230.0), (385.82, 230.0)),
        ((350.0, 194.18), (350.0, 105.82)),
    ];
    for (k, (i, o)) in sp.knots.iter_mut().zip(controls) {
        *k = Knot::smooth(k.anchor, Point::new(i.0, i.1), Point::new(o.0, o.1));
    }
    Path::new(vec![sp])
}

fn near(p: Point, expected: [f64; 2]) {
    assert!((p.x - expected[0]).abs() <= 1.0 && (p.y - expected[1]).abs() <= 1.0, "{p:?} != {expected:?}");
}

/// Only the two facing handles may change, even when the knots are smooth.
fn only_segment_handles_changed(before: &Path, after: &Path, knot: [usize; 2]) {
    let end = segment_end(before, knot).unwrap();
    let mut restored = after.clone();
    restored.subpaths[knot[0]].knots[knot[1]].out_ctrl = before.subpaths[knot[0]].knots[knot[1]].out_ctrl;
    restored.subpaths[end[0]].knots[end[1]].in_ctrl = before.subpaths[end[0]].knots[end[1]].in_ctrl;
    assert_eq!(&restored, before, "anchors, opposite handles, flags and other subpaths stay unchanged");
    for (r, h) in [(knot, Handle::Out), (end, Handle::In)] {
        let old = before.subpaths[r[0]].knots[r[1]];
        let new = after.subpaths[r[0]].knots[r[1]];
        let v = [handle(&old, h).x - old.anchor.x, handle(&old, h).y - old.anchor.y];
        let w = [handle(&new, h).x - new.anchor.x, handle(&new, h).y - new.anchor.y];
        assert!((v[0] * w[1] - v[1] * w[0]).abs() < 1e-8, "handle stays on its original line");
        assert!(v[0] * w[0] + v[1] * w[1] >= 0.0, "handle stays on its original ray");
    }
}

#[test]
fn the_measured_s_curve_keeps_handle_directions_without_moving_anchors_or_opposite_handles() {
    let before = curve();
    let mut p = before.clone();
    bend_segment_constrained(&mut p, [0, 0], 0.5, [0.0, -50.0]).unwrap();
    near(p.subpaths[0].knots[0].out_ctrl, [527.5, 322.5]);
    near(p.subpaths[0].knots[1].in_ctrl, [620.9, 529.1]);
    only_segment_handles_changed(&before, &p, [0, 0]);
}

#[test]
fn the_measured_circle_keeps_the_opposite_handles_and_follows_the_pointer() {
    let before = circle();
    let mut p = before.clone();
    bend_segment_constrained(&mut p, [0, 0], 0.5, [20.0, -20.0]).unwrap();
    near(p.subpaths[0].knots[0].out_ctrl, [527.5, 70.0]);
    near(p.subpaths[0].knots[1].in_ctrl, [510.0, 52.5]);
    only_segment_handles_changed(&before, &p, [0, 0]);
    let a = eval(&before.subpaths[0].segments()[0], 0.5);
    let b = eval(&p.subpaths[0].segments()[0], 0.5);
    assert!(dist(b, add(a, [20.0, -20.0])) < 1e-9);
}

#[test]
fn a_closed_wrapping_curve_changes_only_its_facing_handles() {
    let before = circle();
    let mut p = before.clone();
    bend_segment_constrained(&mut p, [0, 3], 0.5, [-20.0, -20.0]).unwrap();
    near(p.subpaths[0].knots[3].out_ctrl, [350.0, 52.5]);
    near(p.subpaths[0].knots[0].in_ctrl, [332.5, 70.0]);
    only_segment_handles_changed(&before, &p, [0, 3]);
}

#[test]
fn straight_open_and_closed_segments_match_the_unconstrained_drag_exactly() {
    for (before, knot) in [
        (Path::new(vec![Subpath::polyline(&[(400.0, 300.0), (700.0, 300.0)])]), [0, 0]),
        (Path::new(vec![Subpath::polygon(&[(100.0, 300.0), (300.0, 300.0), (300.0, 500.0), (100.0, 500.0)])]), [0, 3]),
    ] {
        let mut free = before.clone();
        let mut constrained = before.clone();
        bend_segment(&mut free, knot, 0.5, [20.0, -50.0]).unwrap();
        bend_segment_constrained(&mut constrained, knot, 0.5, [20.0, -50.0]).unwrap();
        assert_eq!(constrained, free);
        let end = segment_end(&before, knot).unwrap();
        for r in [knot, end] {
            assert_eq!(constrained.subpaths[r[0]].knots[r[1]].anchor, add(before.subpaths[r[0]].knots[r[1]].anchor, [20.0, -50.0]));
        }
    }
}

#[test]
fn a_retracted_handle_keeps_the_existing_unconstrained_behavior() {
    for retract_start in [false, true] {
        let mut before = curve();
        if retract_start {
            before.subpaths[0].knots[0].out_ctrl = before.subpaths[0].knots[0].anchor;
        } else {
            before.subpaths[0].knots[1].in_ctrl = before.subpaths[0].knots[1].anchor;
        }
        let mut free = before.clone();
        let mut constrained = before.clone();
        bend_segment(&mut free, [0, 0], 0.5, [0.0, -50.0]).unwrap();
        bend_segment_constrained(&mut constrained, [0, 0], 0.5, [0.0, -50.0]).unwrap();
        assert_eq!(constrained, free);
    }
}

#[test]
fn near_anchor_and_nonfinite_parameters_keep_the_existing_clamped_drag() {
    for t in [0.0, 0.01, 0.99, 1.0, f64::NAN, f64::INFINITY] {
        let mut free = curve();
        let mut constrained = free.clone();
        bend_segment(&mut free, [0, 0], t, [0.0, -50.0]).unwrap();
        bend_segment_constrained(&mut constrained, [0, 0], t, [0.0, -50.0]).unwrap();
        assert_eq!(constrained, free, "t={t}");
    }
}

#[test]
fn invalid_segments_are_errors_and_leave_the_path_unchanged() {
    let before = curve();
    for knot in [[0, 1], [0, 2], [1, 0], [usize::MAX, usize::MAX]] {
        let mut p = before.clone();
        assert!(bend_segment_constrained(&mut p, knot, 0.5, [0.0, -50.0]).is_err());
        assert_eq!(p, before);
    }
}

#[test]
fn a_zero_drag_does_not_change_the_path() {
    for before in [curve(), circle()] {
        let mut p = before.clone();
        bend_segment_constrained(&mut p, [0, 0], 0.5, [0.0, 0.0]).unwrap();
        assert_eq!(p, before);
    }
}

#[test]
fn the_option_off_geometry_keeps_the_measured_free_bend() {
    // The issue's off-mode baseline allows 2 px, independently of the constrained 1 px checks.
    let near_free = |actual: Point, x: f64, y: f64| assert!((actual.x - x).abs() < 2.0 && (actual.y - y).abs() < 2.0, "{actual:?}");
    let mut c = curve();
    bend_segment(&mut c, [0, 0], 0.5, [0.0, -50.0]).unwrap();
    near_free(c.subpaths[0].knots[0].out_ctrl, 500.1, 282.3);
    near_free(c.subpaths[0].knots[1].in_ctrl, 600.1, 483.8);
    let mut p = circle();
    bend_segment(&mut p, [0, 0], 0.5, [20.0, -20.0]).unwrap();
    near_free(p.subpaths[0].knots[0].out_ctrl, 502.3, 41.9);
    near_free(p.subpaths[0].knots[1].in_ctrl, 536.4, 79.4);
    near_free(p.subpaths[0].knots[0].in_ctrl, 388.8, 86.0);
    near_free(p.subpaths[0].knots[1].out_ctrl, 494.5, 191.4);
}
