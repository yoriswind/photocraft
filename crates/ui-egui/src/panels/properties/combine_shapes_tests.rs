//! Properties exposes the menu's existing operations without changing their selection or history semantics.
use super::*;
use egui::vec2;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

const OPERATIONS: [(&str, &str); 5] = [
    ("layer.combineShapes.unite", "Unite Shapes"),
    ("layer.combineShapes.subtractFrontShape", "Subtract Front Shape"),
    ("layer.combineShapes.intersectShapeAreas", "Intersect Shape Areas"),
    ("layer.combineShapes.excludeOverlappingShapes", "Exclude Overlapping Shapes"),
    ("layer.combineShapes.mergeShapeComponents", "Merge Shape Components"),
];

fn fixture() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 96, "height": 64})).unwrap();
    let a = app.run("shape.create", json!({"kind": "rect", "rect": [8, 8, 40, 32], "fill": "#cc4422"})).unwrap()["layer"].as_u64().unwrap();
    app.run("shape.create", json!({"kind": "rect", "rect": [28, 20, 40, 32], "fill": "#2266cc"})).unwrap();
    app.run("layer.select", json!({"layer": a, "mode": "add"})).unwrap();
    app
}

fn harness(mut app: PhotocraftApp, width: f32, scale: f32) -> Harness<'static, PhotocraftApp> {
    // The shape tool's options can offer similarly named actions; this witness addresses Properties.
    app.ui.tool = crate::state::Tool::Brush;
    let mut h = Harness::builder()
        .with_context(app.ctx().clone())
        .with_size(vec2(width, 900.0))
        .with_pixels_per_point(scale)
        .with_max_steps(64)
        .build_eframe(move |_cc| app);
    crate::panel_docking::show_panel(h.state_mut(), "properties", Some(true)).unwrap();
    h.run_steps(8);
    h
}

#[test]
fn properties_combine_buttons_match_menu_and_whole_document_undo_redo() {
    for (command, label) in OPERATIONS {
        let app = fixture();
        let state = app.session.active().unwrap();
        assert_eq!(state.selected_layers().len(), 2);
        assert!(crate::menus::is_enabled(&app, command), "menu control: {command}");
        let before = state.doc.clone();
        let history = state.history.past_len();
        let mut reference = photocraft_engine::Session::new();
        reference.add_document((*before).clone(), None);
        reference.restore_layer_selection(state.active_layer, &state.selected_layers(), state.layer_anchor).unwrap();
        let mut reference = PhotocraftApp::new(reference, crate::Services::default());
        let mut h = harness(app, 1200.0, 1.0);
        assert!(std::sync::Arc::ptr_eq(&before, &h.state().session.active().unwrap().doc), "drawing must not edit");
        h.get_by_label(label).click();
        h.run_steps(4);
        let ctx = reference.ctx().clone();
        crate::menus::invoke(&mut reference, &ctx, command, json!({})).unwrap();
        let after = h.state().session.active().unwrap().doc.clone();
        assert_eq!(after, reference.session.active().unwrap().doc, "{command}: full menu result");
        assert_ne!(after, before);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1, "one edit");
        assert_eq!(h.state().session.active().unwrap().selected_layers(), reference.session.active().unwrap().selected_layers());
        h.state_mut().run("edit.undo", json!({})).unwrap();
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().doc, before, "{command}: whole document undo");
        h.state_mut().run("edit.redo", json!({})).unwrap();
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().doc, after, "{command}: whole document redo");
    }
}

#[test]
fn properties_combine_is_hidden_without_multiple_selected_layers() {
    for selection in ["empty", "none", "single"] {
        let mut app = if selection == "empty" { PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default()) } else { fixture() };
        if selection == "none" {
            app.run("select.deselectLayers", json!({})).unwrap();
        } else if selection == "single" {
            let active = app.session.active().unwrap().active_layer.unwrap();
            app.run("layer.select", json!({"layer": active.0})).unwrap();
        }
        let h = harness(app, 1200.0, 1.0);
        for (_, label) in OPERATIONS {
            assert!(h.query_by_label(label).is_none());
        }
    }
}

#[test]
fn properties_combine_mixed_selection_matches_menu_availability() {
    for active_shape in [false, true] {
        let mut app = fixture();
        let mut selected = app.session.active().unwrap().selected_layers();
        let shape = app.session.active().unwrap().active_layer.unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        let raster = app.session.active().unwrap().active_layer.unwrap();
        let active = if active_shape { shape } else { raster };
        selected.push(raster);
        app.session.restore_layer_selection(Some(active), &selected, Some(active)).unwrap();
        let before = app.session.active().unwrap().doc.clone();
        let history = app.session.active().unwrap().history.past_len();
        let mut h = harness(app, 1200.0, 1.0);
        for (command, label) in OPERATIONS {
            let enabled = crate::menus::is_enabled(h.state(), command);
            assert_eq!(enabled, active_shape);
            assert_eq!(!h.get_by_label(label).accesskit_node().is_disabled(), enabled, "{command}: menu availability");
            if !enabled {
                h.get_by_label(label).click();
                h.run_steps(2);
            }
        }
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    }
}

#[test]
fn properties_combine_keeps_unsupported_background_and_mask_contexts_safe() {
    let mut app = fixture();
    let background = app.session.active().unwrap().doc.layers[0].id;
    app.run("layer.select", json!({"layer": background.0, "mode": "add"})).unwrap();
    let before = app.session.active().unwrap().doc.clone();
    let mut h = harness(app, 1200.0, 1.0);
    for (command, label) in OPERATIONS {
        assert!(!crate::menus::is_enabled(h.state(), command));
        assert!(h.get_by_label(label).accesskit_node().is_disabled());
        h.get_by_label(label).click();
        h.run_steps(2);
    }
    assert_eq!(h.state().session.active().unwrap().doc, before);
    drop(h);
    for vector in [false, true] {
        let mut app = fixture();
        if vector {
            // A shape already uses its path as a vector mask, so target a separate raster mask.
            let mut selected = app.session.active().unwrap().selected_layers();
            app.run("layer.new.layer", json!({"name": "Mask target"})).unwrap();
            let active = app.session.active().unwrap().active_layer;
            selected.push(active.unwrap());
            app.session.restore_layer_selection(active, &selected, active).unwrap();
        }
        app.run(if vector { "layer.vectorMask.revealAll" } else { "layer.layerMask.revealAll" }, json!({})).unwrap();
        app.ui.mask_target = !vector;
        app.ui.vector_mask_target = vector;
        let state = app.session.active().unwrap();
        let layer = state.doc.layer(state.active_layer.unwrap()).unwrap();
        assert!(state.selected_layers().len() > 1);
        assert!(if vector { crate::mask_props_ui::vector_targeted(&app, layer) } else { crate::mask_props_ui::targeted(&app, layer) });
        let before = state.doc.clone();
        let h = harness(app, 1200.0, 1.0);
        for (_, label) in OPERATIONS {
            assert!(h.query_by_label(label).is_none(), "mask Properties retain their own context");
        }
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
}

#[test]
fn properties_combine_rejects_locked_shapes_without_an_edit() {
    let mut app = fixture();
    app.run("layer.lockLayers", json!({"all": true})).unwrap();
    let before = app.session.active().unwrap().doc.clone();
    let history = app.session.active().unwrap().history.past_len();
    let mut h = harness(app, 1200.0, 1.0);
    for (_, label) in OPERATIONS {
        h.get_by_label(label).click();
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    }
}

#[test]
fn properties_combine_buttons_fit_and_keyboard_activates_the_menu_operation() {
    for width in [800.0, 1440.0] {
        for scale in [1.0, 2.0] {
            let mut h = harness(fixture(), width, scale);
            let mut previous: Option<egui::Rect> = None;
            for (_, label) in OPERATIONS {
                let button = h.get_by_label(label);
                let rect = button.rect();
                assert!(rect.left() >= 0.0 && rect.right() <= width && rect.height() >= 24.0, "{width} {scale}: {rect:?}");
                if let Some(last) = previous {
                    assert!(!last.intersects(rect), "overlapping controls: {last:?} {rect:?}");
                }
                previous = Some(rect);
            }
            let before = h.state().session.active().unwrap().doc.clone();
            h.get_by_label("Unite Shapes").focus();
            h.key_press(egui::Key::Space);
            h.run_steps(4);
            assert_ne!(h.state().session.active().unwrap().doc, before, "keyboard must activate Unite Shapes");
        }
    }
}
