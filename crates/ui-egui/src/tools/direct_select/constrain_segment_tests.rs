//! #13214: the shared overlay's constrained receipt must reach the same real engine operation
//! that the preview showed. The option remains a saved, inspected, resettable Direct Selection setting.

use egui::Modifiers;
use photocraft_doc::Path;
use serde_json::{Value, json};

use super::convert_point_history_tests::{added, drag};
use super::{PathRef, path_of};
use crate::PhotocraftApp;
use crate::canvas::testing::{ToolEvent, tool_event};
use crate::state::Tool;

fn curve() -> Value {
    json!({"subpaths": [{"closed": false, "knots": [
        {"anchor": [400,450], "in": [300,550], "out": [500,350], "smooth": true},
        {"anchor": [700,450], "in": [600,550], "out": [800,350], "smooth": true}
    ]}]})
}

fn circle() -> Value {
    json!({"subpaths": [{"closed": true, "knots": [
        {"anchor": [430,70], "in": [385.82,70], "out": [474.18,70], "smooth": true},
        {"anchor": [510,150], "in": [510,105.82], "out": [510,194.18], "smooth": true},
        {"anchor": [430,230], "in": [474.18,230], "out": [385.82,230], "smooth": true},
        {"anchor": [350,150], "in": [350,194.18], "out": [350,105.82], "smooth": true}
    ]}]})
}

fn app(path: Value) -> Box<PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 1200, "height": 800, "background": "white"})).unwrap();
    app.run("path.set", json!({"name": "Path 1", "path": path})).unwrap();
    app.ui.selected_path = Some("Path 1".into());
    app.ui.tool = Tool::DirectSelection;
    app.ui.extras.snap = false;
    app.sync_views();
    app
}

fn path(app: &PhotocraftApp) -> Path {
    path_of(app, &PathRef::Saved(0)).unwrap().into_owned()
}

#[test]
fn constrained_shown_geometry_commits_through_the_engine_as_one_drag_segment() {
    for (fixture, from, delta, out, into) in [
        (curve(), [550.0, 450.0], [0.0, -50.0], [527.5, 322.5], [620.9, 529.1]),
        (circle(), [486.5675, 93.4325], [20.0, -20.0], [527.5, 70.0], [510.0, 52.5]),
    ] {
        let mut app = app(fixture);
        app.ui.tool_options.constrain_path_dragging = true;
        let before = path(&app);
        let state = app.session.active().unwrap();
        let history = state.history.entries();
        let revision = state.revision;
        tool_event(&mut app, ToolEvent::Down { x: from[0], y: from[1], pressure: 1.0 }, Modifiers::NONE);
        tool_event(&mut app, ToolEvent::Move { x: from[0] + delta[0], y: from[1] + delta[1], pressure: 1.0 }, Modifiers::NONE);
        let shown = super::edits::test_preview(&app).expect("the actual shown overlay value");
        let preview = shown.iter().find(|p| p.key == PathRef::Saved(0).key()).unwrap().clone();
        assert_eq!(path(&app), before, "preview has not committed the document");
        assert_eq!(app.session.active().unwrap().revision, revision);
        assert_eq!(app.session.active().unwrap().history.entries(), history);
        let knots = &preview.subpaths[0].knots;
        assert!(knots[0].out_ctrl.distance(craft::behavior::kurbo::Point::new(out[0], out[1])) <= 1.0);
        assert!(knots[1].in_ctrl.distance(craft::behavior::kurbo::Point::new(into[0], into[1])) <= 1.0);
        let old = crate::vector_ui::edit_path(&before, PathRef::Saved(0).key());
        for (i, (a, b)) in old.subpaths[0].knots.iter().zip(knots).enumerate() {
            assert_eq!(a.anchor, b.anchor);
            assert_eq!(a.smooth, b.smooth);
            if i != 0 { assert_eq!(a.out_ctrl, b.out_ctrl); }
            if i != 1 { assert_eq!(a.in_ctrl, b.in_ctrl); }
        }
        let names = added(&mut app, |app| {
            tool_event(app, ToolEvent::Up { x: from[0] + delta[0], y: from[1] + delta[1] }, Modifiers::NONE);
        });
        assert_eq!(names, ["Drag Segment"]);
        let after = path(&app);
        assert_eq!(crate::vector_ui::edit_path(&after, PathRef::Saved(0).key()).subpaths, preview.subpaths);
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(path(&app), before);
        app.run("edit.redo", json!({})).unwrap();
        assert_eq!(path(&app), after);
    }
}

#[test]
fn modified_segment_gestures_keep_the_same_geometry_and_history_with_the_option_on_or_off() {
    for mods in [Modifiers::SHIFT, Modifiers::ALT, Modifiers::COMMAND] {
        let run = |enabled| {
            let mut app = app(curve());
            app.ui.tool_options.constrain_path_dragging = enabled;
            let names = added(&mut app, |app| drag(app, [550.0, 450.0], [570.0, 400.0], mods));
            (path(&app), names)
        };
        assert_eq!(run(true), run(false), "{mods:?}");
    }
}

#[test]
fn constrain_checkbox_is_saved_inspected_and_reset_without_changing_other_options() {
    use egui::accesskit::Role;
    use egui_kittest::{Harness, kittest::{NodeT, Queryable}};
    use crate::control::{ControlRequest, Reply, handle};
    use crate::state::ToolOptions;
    use crate::app::framework::BuildApp as _;

    let app = app(curve());
    let ctx = app.ctx().clone();
    let mut h = Harness::builder().with_size(egui::vec2(1800.0, 250.0)).with_step_dt(0.25)
        .with_context(ctx.clone()).build_app(
            |ui, app| {
                crate::panels::options_bar(app, ui);
                crate::dialogs::run(app, ui);
            }, app);
    h.run_steps(4);
    assert!(!h.state().ui.tool_options.constrain_path_dragging);
    h.get_by_role_and_label(Role::CheckBox, "Constrain Path Dragging").click();
    h.run_steps(2);
    assert!(h.state().ui.tool_options.constrain_path_dragging);
    let back: ToolOptions = serde_json::from_value(serde_json::to_value(&h.state().ui.tool_options).unwrap()).unwrap();
    assert!(back.constrain_path_dragging);
    let legacy: ToolOptions = serde_json::from_value(json!({})).unwrap();
    assert!(!legacy.constrain_path_dragging);
    let inspect = |app: &mut PhotocraftApp| {
        let (req, _rx) = ControlRequest::new("ui.inspect", json!({}));
        match handle(app, &ctx, &req) {
            Reply::Done(v) => v["result"]["toolOptions"]["constrain_path_dragging"].clone(),
            _ => panic!("ui.inspect: expected an immediate reply"),
        }
    };
    assert_eq!(inspect(h.state_mut()), json!(true));
    h.state_mut().ui.tool_options.feather = 12.0;
    crate::menus::invoke(h.state_mut(), &ctx, "tool.reset", json!({})).unwrap();
    h.run_steps(2);
    assert!(!h.state().ui.tool_options.constrain_path_dragging);
    assert_eq!(h.state().ui.tool_options.feather, 12.0);
    assert_eq!(inspect(h.state_mut()), json!(false));
    h.state_mut().ui.tool = Tool::PathSelection;
    h.run_steps(2);
    assert_eq!(h.query_all_by_label("Constrain Path Dragging").count(), 0, "only Direct Selection offers this setting");
}


#[test]
fn engine_constrain_flag_is_optional_and_bad_types_change_no_document_or_history() {
    let run = |flag: Option<Value>| {
        let mut app = app(curve());
        let mut params = json!({"name": "Path 1", "subpath": 0, "knot": 0, "t": 0.5, "move": [0.0, -50.0]});
        if let Some(flag) = flag { params["constrain"] = flag; }
        let names = added(&mut app, |app| { app.run("path.bendSegment", params).unwrap(); });
        (path(&app), names)
    };
    let baseline = run(None);
    assert_eq!(baseline, run(Some(json!(false))));
    assert_eq!(baseline, run(Some(Value::Null)));
    assert_eq!(baseline.1, ["Drag Segment"]);
    assert_ne!(baseline.0, run(Some(json!(true))).0);
    let mut app = app(curve());
    let before = path(&app);
    let state = app.session.active().unwrap();
    let revision = state.revision;
    let history = state.history.entries();
    let journal = app.session.journal.len();
    for flag in [json!("true"), json!(1), json!([]), json!({})] {
        assert!(app.run("path.bendSegment", json!({"name": "Path 1", "subpath": 0, "knot": 0, "t": 0.5, "move": [0.0, -50.0], "constrain": flag})).is_err());
        assert_eq!(path(&app), before);
        assert_eq!(app.session.active().unwrap().revision, revision);
        assert_eq!(app.session.active().unwrap().history.entries(), history);
        assert_eq!(app.session.journal.len(), journal);
    }
}


#[test]
fn shared_receipts_and_real_commands_match_across_measured_and_fallback_geometry() {
    use craft::behavior::kurbo::Vec2;
    use craft::draw::ui::pen::{KnotRef, PathEdit};

    let mut retract_start = curve();
    retract_start["subpaths"][0]["knots"][0]["out"] = json!([400, 450]);
    let mut retract_end = curve();
    retract_end["subpaths"][0]["knots"][1]["in"] = json!([700, 450]);
    let line = json!({"subpaths": [{"closed": false, "knots": [[400,300], [700,300]]}]});
    let square = json!({"subpaths": [{"closed": true, "knots": [[100,300], [300,300], [300,500], [100,500]]}]});
    let mut cases = vec![
        (curve(), 0, 0.5, [0.0, -50.0], true, true),
        (circle(), 0, 0.5, [20.0, -20.0], true, true),
        (circle(), 3, 0.5, [-20.0, -20.0], true, true),
        (line, 0, 0.5, [20.0, -50.0], true, true),
        (square, 3, 0.5, [20.0, -50.0], true, true),
        (retract_start, 0, 0.5, [0.0, -50.0], true, true),
        (retract_end, 0, 0.5, [0.0, -50.0], true, true),
        (curve(), 0, 0.5, [0.0, 0.0], true, true),
        (circle(), 0, 0.5, [0.0, 0.0], true, true),
        (curve(), 0, 0.5, [0.0, -50.0], false, true),
        (circle(), 0, 0.5, [20.0, -20.0], false, true),
        (circle(), 0, 0.5, [-80.0, 80.0], true, true), // facing lengths clamp to zero
        (curve(), 0, 0.5, [f64::MAX, f64::MAX], true, true), // rejected finite overflow leaves both models unchanged
        (circle(), 0, 0.5, [f64::MAX, f64::MAX], true, true),
        (curve(), 1, 0.5, [0.0, -50.0], true, false),
        (curve(), usize::MAX, 0.5, [0.0, -50.0], true, false),
    ];
    for t in [0.0, 0.01, 0.99, 1.0] {
        cases.push((curve(), 0, t, [0.0, -50.0], true, true));
    }
    for (fixture, knot, t, by, constrain, valid) in cases {
        let mut app = app(fixture);
        let before = path(&app);
        let mut shown = crate::vector_ui::edit_path(&before, PathRef::Saved(0).key());
        let segment = KnotRef::new(0, knot);
        let edit = if constrain {
            PathEdit::bend_segment_constrained(shown.key, segment, t, Vec2::new(by[0], by[1]))
        } else {
            PathEdit::bend_segment(shown.key, segment, t, Vec2::new(by[0], by[1]))
        };
        let changed = edit.apply(&mut shown);
        let result = app.run("path.bendSegment", json!({"name": "Path 1", "subpath": 0, "knot": knot, "t": t, "move": by, "constrain": constrain}));
        assert_eq!(result.is_ok(), valid, "knot={knot}, t={t}, by={by:?}, constrain={constrain}: {result:?}");
        assert_eq!(crate::vector_ui::edit_path(&path(&app), shown.key).subpaths, shown.subpaths,
            "the actual shared receipt and real command must agree: knot={knot}, t={t}, by={by:?}, constrain={constrain}");
        if !valid || by == [0.0, 0.0] {
            assert!(!changed);
            assert_eq!(path(&app), before);
        }
    }
}
