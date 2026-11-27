//! #13213: Ctrl/Cmd alone with Direct Selection drags the pressed subpath as a whole.
//! Shift, Alt, handles and the empty-canvas marquee retain their existing behavior.

use egui::Modifiers;
use photocraft_doc::Path;
use serde_json::json;

use super::convert_point_history_tests::{added, drag};
use super::{PathRef, path_of};
use crate::PhotocraftApp;
use crate::canvas::testing::{ToolEvent, tool_event};
use crate::state::Tool;

const A: [[f64; 2]; 4] = [[100.0, 300.0], [300.0, 300.0], [300.0, 500.0], [100.0, 500.0]];
const B: [[f64; 2]; 4] = [[400.0, 100.0], [600.0, 100.0], [600.0, 200.0], [400.0, 200.0]];

fn setup(kind: &str) -> (Box<PhotocraftApp>, PathRef) {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 800, "height": 600, "background": "white"})).unwrap();
    let path = json!({"subpaths": [{"closed": true, "knots": A}, {"closed": true, "knots": B}]});
    let target = match kind {
        "saved" => {
            app.run("path.set", json!({"name": "Path 1", "path": path})).unwrap();
            app.ui.selected_path = Some("Path 1".into());
            PathRef::Saved(0)
        }
        "work" => {
            app.run("path.set", json!({"name": "work", "path": path})).unwrap();
            PathRef::Work
        }
        "shape" => PathRef::Layer(app.run("shape.create", json!({"kind": "path", "path": path, "fill": "#3070c0"})).unwrap()["layer"].as_u64().unwrap()),
        "mask" => {
            app.run("layer.new.layer", json!({})).unwrap();
            let id = app.session.active().unwrap().active_layer.unwrap().0;
            app.run("layer.vectorMask.add", json!({"layer": id, "path": path})).unwrap();
            app.ui.vector_mask_target = true;
            PathRef::Layer(id)
        }
        _ => panic!("unknown target fixture: {kind}"),
    };
    app.ui.tool = Tool::DirectSelection;
    app.ui.extras.snap = false;
    app.sync_views();
    (app, target)
}

fn path(app: &PhotocraftApp, target: PathRef) -> Path {
    path_of(app, &target).unwrap().into_owned()
}

fn anchors(path: &Path, subpath: usize) -> Vec<[f64; 2]> {
    path.subpaths[subpath].knots.iter().map(|k| [k.anchor.x, k.anchor.y]).collect()
}

fn translated(points: &[[f64; 2]], [dx, dy]: [f64; 2]) -> Vec<[f64; 2]> {
    points.iter().map(|[x, y]| [x + dx, y + dy]).collect()
}

#[test]
fn ctrl_drag_on_an_anchor_moves_every_anchor_of_only_its_subpath_in_one_drag_paths_step() {
    let (mut app, target) = setup("saved");
    let before = path(&app, target);
    let names = added(&mut app, |app| drag(app, [300.0, 300.0], [320.0, 280.0], Modifiers::COMMAND));
    let after = path(&app, target);
    assert_eq!(anchors(&after, 0), translated(&A, [20.0, -20.0]));
    assert_eq!(after.subpaths[1], before.subpaths[1], "the other subpath stays put");
    assert_eq!(names, ["Drag Paths"]);
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(path(&app, target), before, "one undo restores the complete path");
    app.run("edit.redo", json!({})).unwrap();
    assert_eq!(path(&app, target), after);
}

#[test]
fn ctrl_drag_on_the_left_edge_moves_the_whole_subpath() {
    let (mut app, target) = setup("saved");
    let names = added(&mut app, |app| drag(app, [100.0, 400.0], [150.0, 430.0], Modifiers::COMMAND));
    let after = path(&app, target);
    assert_eq!(anchors(&after, 0), translated(&A, [50.0, 30.0]));
    assert_eq!(anchors(&after, 1), B);
    assert_eq!(names, ["Drag Paths"]);
}

#[test]
fn windows_ctrl_and_mac_command_move_the_subpath_on_each_document_target() {
    let windows = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
    let mac = Modifiers { mac_cmd: true, command: true, ..Modifiers::NONE };
    for kind in ["saved", "work", "shape", "mask"] {
        for mods in [windows, mac] {
            let (mut app, target) = setup(kind);
            drag(&mut app, [300.0, 300.0], [320.0, 280.0], mods);
            let after = path(&app, target);
            assert_eq!(anchors(&after, 0), translated(&A, [20.0, -20.0]), "{kind}: {mods:?}");
            assert_eq!(anchors(&after, 1), B, "{kind}: {mods:?}");
            assert_eq!(app.ui.tool, Tool::DirectSelection, "the selected tool stays Direct Selection");
        }
    }
}

#[test]
fn command_subpath_drag_previews_without_committing_before_release() {
    let (mut app, target) = setup("saved");
    let before = path(&app, target);
    tool_event(&mut app, ToolEvent::Down { x: 300.0, y: 300.0, pressure: 1.0 }, Modifiers::COMMAND);
    tool_event(&mut app, ToolEvent::Move { x: 320.0, y: 280.0, pressure: 1.0 }, Modifiers::COMMAND);
    let shown = super::edits::test_preview(&app).expect("an actual overlay shown-value preview");
    let preview = shown.iter().find(|path| path.key == target.key()).expect("the target's shown path");
    let preview_anchors: Vec<_> = preview.subpaths[0].knots.iter().map(|k| [k.anchor.x, k.anchor.y]).collect();
    assert_eq!(preview_anchors, translated(&A, [20.0, -20.0]));
    assert_eq!(preview.subpaths[1], crate::vector_ui::edit_path(&before, target.key()).subpaths[1]);
    assert_eq!(path(&app, target), before);
    tool_event(&mut app, ToolEvent::Up { x: 320.0, y: 280.0 }, Modifiers::COMMAND);
    assert_eq!(anchors(&path(&app, target), 0), translated(&A, [20.0, -20.0]));
}

#[test]
fn releasing_command_mid_drag_finishes_the_subpath_then_plain_drag_selects_one_anchor() {
    let (mut app, target) = setup("saved");
    tool_event(&mut app, ToolEvent::Down { x: 300.0, y: 300.0, pressure: 1.0 }, Modifiers::COMMAND);
    tool_event(&mut app, ToolEvent::Move { x: 320.0, y: 280.0, pressure: 1.0 }, Modifiers::NONE);
    tool_event(&mut app, ToolEvent::Up { x: 320.0, y: 280.0 }, Modifiers::NONE);
    assert_eq!(anchors(&path(&app, target), 0), translated(&A, [20.0, -20.0]));
    drag(&mut app, B[0], [410.0, 110.0], Modifiers::NONE);
    assert_eq!(anchors(&path(&app, target), 1), vec![[410.0, 110.0], B[1], B[2], B[3]]);
    assert_eq!(app.ui.direct_selection.anchors, vec![[1, 0]]);
}

#[test]
fn command_click_selects_the_subpath_without_a_history_step() {
    let (mut app, target) = setup("saved");
    let before = path(&app, target);
    assert!(added(&mut app, |app| drag(app, A[1], A[1], Modifiers::COMMAND)).is_empty());
    assert_eq!(app.ui.direct_selection.anchors, vec![[0, 0], [0, 1], [0, 2], [0, 3]]);
    assert_eq!(path(&app, target), before);
}

#[test]
fn command_shift_still_drags_only_the_pressed_anchor() {
    let (mut app, target) = setup("saved");
    let mods = Modifiers { shift: true, ..Modifiers::COMMAND };
    drag(&mut app, A[1], [320.0, 280.0], mods);
    assert_eq!(anchors(&path(&app, target), 0), vec![A[0], [320.0, 280.0], A[2], A[3]]);
}

#[test]
fn command_alt_still_moves_without_duplicating() {
    let (mut app, target) = setup("saved");
    let mods = Modifiers { alt: true, ..Modifiers::COMMAND };
    drag(&mut app, [100.0, 400.0], [150.0, 430.0], mods);
    let after = path(&app, target);
    assert_eq!(anchors(&after, 0), translated(&A, [50.0, 30.0]));
    assert_eq!(after.subpaths.len(), 2, "the unmeasured combination keeps today's behavior");
}

#[test]
fn command_empty_canvas_still_marquee_selects_anchors() {
    let (mut app, target) = setup("saved");
    let before = path(&app, target);
    assert!(added(&mut app, |app| drag(app, [70.0, 270.0], [130.0, 330.0], Modifiers::COMMAND)).is_empty());
    assert_eq!(app.ui.direct_selection.anchors, vec![[0, 0]]);
    assert_eq!(path(&app, target), before);
}

#[test]
fn command_drag_on_a_shown_handle_keeps_handle_editing() {
    let (mut app, target) = setup("saved");
    let path = json!({"subpaths": [{"closed": true, "knots": [
        [100, 300],
        {"anchor": [300, 300], "in": [280, 300], "out": [320, 300], "smooth": true},
        [300, 500], [100, 500]
    ]}]});
    app.run("path.set", json!({"name": "Path 1", "path": path})).unwrap();
    drag(&mut app, A[1], A[1], Modifiers::NONE);
    drag(&mut app, [320.0, 300.0], [320.0, 320.0], Modifiers::COMMAND);
    let after = super::path_of(&app, &target).unwrap();
    assert_eq!(anchors(&after, 0), A, "the command press on a handle does not move the subpath");
    assert_eq!(after.subpaths[0].knots[1].out_ctrl, photocraft_geom::Point::new(320.0, 320.0));
    assert_eq!(app.session.active().unwrap().history.undo_label(), Some("Drag Control Point"));
}

#[test]
fn command_on_a_hidden_active_layer_keeps_single_anchor_selection() {
    for kind in ["shape", "mask"] {
        let (mut app, target) = setup(kind);
        let PathRef::Layer(id) = target else { panic!("a layer target") };
        app.run("layer.setProps", json!({"layer": id, "visible": false})).unwrap();
        let before = path(&app, target);
        drag(&mut app, A[1], A[1], Modifiers::COMMAND);
        assert_eq!(app.ui.direct_selection.anchors, vec![[0, 1]], "{kind}: the unmeasured hidden-layer case is unchanged");
        assert_eq!(path(&app, target), before);
    }
}
