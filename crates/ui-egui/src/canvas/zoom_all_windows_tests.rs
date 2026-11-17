//! Zoom All Windows follows only real selected Zoom clicks. Each shown view keeps its own camera and ladder.

use craft::behavior::dock::{Arrangement, DocumentGroup};
use craft::behavior::{CanvasView, ZoomLadder};
use craft::ui::canvas::{ToolHost, ViewCommands};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

use super::document_display::CanvasHost;
use crate::PhotocraftApp;
use crate::state::{Tool, ToolOptions};

const DT: f32 = 1.0 / 60.0;

fn app(ctx: egui::Context) -> PhotocraftApp {
    PhotocraftApp::setup_context(&ctx, Default::default());
    let mut app = PhotocraftApp::with_context(ctx, photocraft_engine::Session::new(), crate::Services::default());
    app.background_jobs = false;
    app.session.edit_prefs(|prefs| prefs.general.animated_zoom = false);
    // The two measured documents are blank; the third small document is a hidden-tab control.
    for (width, height) in [(2000, 1500), (3000, 2000), (32, 32)] {
        app.run("file.new", json!({"width": width, "height": height})).unwrap();
    }
    app.sync_views();
    let ids: Vec<_> = app.session.documents().iter().map(|state| state.doc.id).collect();
    let mut first = DocumentGroup::new(0, vec![ids[0]]);
    first.shown = Some(ids[0]);
    let mut second = DocumentGroup::new(1, vec![ids[1], ids[2]]);
    second.shown = Some(ids[1]);
    app.ui.document_windows.docked = vec![first, second];
    app.ui.document_windows.arrangement = Some(Arrangement::TwoUpVertical);
    app.session.set_active(0);
    app.ui.tool = Tool::Zoom;
    app.ui.tool_options.zoom_scrubby = false;
    app.ui.extras.rulers = false;
    app
}

fn harness() -> Harness<'static, PhotocraftApp> {
    let mut h = Harness::builder().with_size(vec2(1400.0, 900.0)).with_step_dt(DT).with_max_steps(64).build_eframe(|cc| app(cc.egui_ctx.clone()));
    h.run_steps(6);
    h.ctx.options_mut(|options| options.max_passes = std::num::NonZeroUsize::MIN);
    reset_views(&mut h, 0);
    h
}

/// Set the measured 50% / 33.33% cameras after the real initial fit and layout have run.
fn reset_views(h: &mut Harness<'_, PhotocraftApp>, source: usize) {
    let app = h.state_mut();
    app.session.set_active(source);
    for (index, zoom, center) in [(0, 0.5, [1000.0, 750.0]), (1, 1.0 / 3.0, [1092.0, 1000.5]), (2, 1.0, [16.0, 16.0])] {
        let view = &mut app.ui.views[index];
        view.zoom = zoom;
        view.center = center;
        view.rotation = 0.0;
        view.clear_pending();
    }
    if source == 1 {
        app.ui.views[1].zoom = 0.5;
        app.ui.views[0].zoom = 1.0 / 3.0;
    }
    h.run_steps(2);
}

fn shown(h: &Harness<'_, PhotocraftApp>, index: usize) -> CanvasView {
    ToolHost::shown_space(&h.ctx, CanvasHost::main(index).canvas_id()).expect("the actual tiled canvas was shown").view
}

fn button(h: &mut Harness<'_, PhotocraftApp>, at: Pos2, pressed: bool, modifiers: Modifiers) {
    h.input_mut().events.push(Event::ModifiersChanged(modifiers));
    h.input_mut().events.push(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers });
    h.run_steps(1);
}

fn point(h: &mut Harness<'_, PhotocraftApp>, at: Pos2, modifiers: Modifiers) {
    h.input_mut().events.extend([Event::ModifiersChanged(modifiers), Event::PointerMoved(at)]);
    h.run_steps(1);
}

fn click(h: &mut Harness<'_, PhotocraftApp>, source: usize, offset: egui::Vec2, modifiers: Modifiers) -> Option<bool> {
    let at = shown(h, source).rect.center() + offset;
    point(h, at, modifiers);
    button(h, at, true, modifiers);
    button(h, at, false, modifiers);
    let step = h.state().canvas_tools.last_response.as_ref().and_then(|response| response.zoom_step.as_ref()).map(|step| step.out);
    h.run_steps(2);
    step
}

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

fn center_near(actual: [f32; 2], expected: [f32; 2]) {
    // The canvas aligns its displayed camera to device pixels; at 33.33% one pixel spans three document units.
    assert!((actual[0] - expected[0]).abs() <= 3.0 && (actual[1] - expected[1]).abs() <= 3.0, "{actual:?} != {expected:?}");
}

#[test]
fn measured_tiled_clicks_step_each_own_ladder_and_alt_steps_back() {
    for source in [0, 1] {
        let mut h = harness();
        reset_views(&mut h, source);
        let peer = 1 - source;
        let hidden = h.state().ui.views[2].clone();
        let docs: Vec<_> = h.state().session.documents().iter().map(|state| (state.doc.clone(), state.history.past_len())).collect();
        assert!(!h.state().ui.tool_options.zoom_all_windows);
        assert_eq!(click(&mut h, source, egui::Vec2::ZERO, Modifiers::NONE), Some(false));
        near(h.state().ui.views[source].zoom, 2.0 / 3.0);
        near(h.state().ui.views[peer].zoom, 1.0 / 3.0);
        let peer_center = h.state().ui.views[peer].center;
        h.state_mut().ui.tool_options.zoom_all_windows = true;
        assert_eq!(click(&mut h, source, egui::Vec2::ZERO, Modifiers::NONE), Some(false));
        near(h.state().ui.views[source].zoom, 1.0);
        near(h.state().ui.views[peer].zoom, 0.5);
        center_near(h.state().ui.views[peer].center, peer_center);
        assert_eq!(click(&mut h, source, egui::Vec2::ZERO, Modifiers::ALT), Some(true));
        near(h.state().ui.views[source].zoom, 2.0 / 3.0);
        near(h.state().ui.views[peer].zoom, 1.0 / 3.0);
        center_near(h.state().ui.views[peer].center, peer_center);
        assert_eq!(h.state().ui.views[2], hidden, "a tab that is not shown is not a follower");
        assert_eq!(h.state().session.active_index(), Some(source));
        for (state, (doc, history)) in h.state().session.documents().iter().zip(docs) {
            assert!(std::sync::Arc::ptr_eq(&state.doc, &doc), "zoom never edits document pixels");
            assert_eq!(state.history.past_len(), history);
        }
    }
}

#[test]
fn an_off_center_click_keeps_its_anchor_while_the_peer_keeps_its_own_center() {
    let mut h = harness();
    h.state_mut().ui.tool_options.zoom_all_windows = true;
    let before = shown(&h, 0);
    let offset = vec2(53.0, 37.0);
    let at = before.rect.center() + offset;
    let anchor = before.to_doc(at);
    let source_center = h.state().ui.views[0].center;
    let peer_center = h.state().ui.views[1].center;
    assert_eq!(click(&mut h, 0, offset, Modifiers::NONE), Some(false));
    near(h.state().ui.views[0].zoom, 2.0 / 3.0);
    near(h.state().ui.views[1].zoom, 0.5);
    let after = shown(&h, 0).to_doc(at);
    assert!((after.x - anchor.x).abs() <= 3.0 && (after.y - anchor.y).abs() <= 3.0, "the clicked document point stays under the pointer");
    assert_ne!(h.state().ui.views[0].center, source_center, "an off-center click really moves its own camera");
    center_near(h.state().ui.views[1].center, peer_center);
}

#[test]
fn off_and_shift_off_leave_the_other_shown_view_alone() {
    let mut h = harness();
    h.state_mut().ui.tool_options.hand_scroll_all = true;
    for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
        reset_views(&mut h, 0);
        let peer = h.state().ui.views[1].clone();
        assert_eq!(click(&mut h, 0, egui::Vec2::ZERO, modifiers), Some(false));
        near(h.state().ui.views[0].zoom, 2.0 / 3.0);
        assert_eq!(h.state().ui.views[1], peer, "Shift does not borrow the Hand tool's Scroll All Windows meaning");
    }
}

#[test]
fn the_real_checkbox_is_default_off_and_persists_without_history() {
    assert!(!ToolOptions::default().zoom_all_windows);
    assert!(!serde_json::from_value::<ToolOptions>(json!({})).unwrap().zoom_all_windows);
    let mut h = harness();
    let history: Vec<_> = h.state().session.documents().iter().map(|state| state.history.past_len()).collect();
    let resize = h.get_by_label("Resize Windows to Fit").rect();
    let all = h.get_by_label("Zoom All Windows").rect();
    let scrubby = h.get_by_label("Scrubby Zoom").rect();
    assert!(resize.center().x < all.center().x && all.center().x < scrubby.center().x);
    h.get_by_label("Zoom All Windows").hover();
    h.run_steps(40);
    assert!(h.query_by_label("Zoom all open document windows when using the Zoom Tool").is_some());
    h.get_by_label("Zoom All Windows").click();
    h.run_steps(2);
    assert!(h.state().ui.tool_options.zoom_all_windows, "the real bar edits the Photo-owned option");
    let ctx = h.ctx.clone();
    crate::tool_memory::persist(h.state_mut(), &ctx);
    let saved = h.state().session.prefs().tool_options.clone();
    assert_eq!(saved["zoom_all_windows"], true);
    crate::tool_memory::select(h.state_mut(), Tool::Hand);
    crate::tool_memory::select(h.state_mut(), Tool::Zoom);
    assert!(h.state().ui.tool_options.zoom_all_windows);
    let mut restored = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    restored.session.prefs.edit(|prefs| prefs.tool_options = saved);
    crate::tool_memory::restore(&mut restored);
    assert!(restored.ui.tool_options.zoom_all_windows);
    let params = crate::preset_panels::tool_presets::new_params(h.state(), Tool::Zoom);
    let created = h.state_mut().run("tool.presets.new", params).unwrap();
    let name = created["name"].as_str().unwrap().to_owned();
    let stored = &h.state().session.presets.tool_presets.iter().find(|preset| preset.name == name).unwrap().options["toolOptions"];
    assert_eq!(stored["zoom_all_windows"], true, "a real saved Zoom preset owns the new option");
    assert!(stored.get("hand_scroll_all").is_none());
    h.state_mut().ui.tool_options.zoom_in = false;
    h.state_mut().ui.tool_options.zoom_scrubby = false;
    h.state_mut().ui.tool_options.hand_scroll_all = true;
    crate::menus::invoke(h.state_mut(), &ctx, "tool.reset", json!({})).unwrap();
    assert!(!h.state().ui.tool_options.zoom_all_windows);
    assert!(!h.state().ui.tool_options.zoom_in, "Reset Tool still keeps Zoom Out");
    assert!(h.state().ui.tool_options.zoom_scrubby, "Reset Tool still restores Scrubby Zoom");
    assert!(h.state().ui.tool_options.hand_scroll_all, "the Hand tool's setting is not reset");
    crate::preset_panels::select_tool_preset(h.state_mut(), &name);
    assert!(h.state().ui.tool_options.zoom_all_windows, "the saved Zoom preset restores the option");
    assert!(h.state().ui.tool_options.hand_scroll_all);
    assert_eq!(h.state().session.documents().iter().map(|state| state.history.past_len()).collect::<Vec<_>>(), history);
}

fn key(h: &mut Harness<'_, PhotocraftApp>, key: Key, pressed: bool, modifiers: Modifiers) {
    h.input_mut().events.extend([
        Event::ModifiersChanged(modifiers),
        Event::Key { key, physical_key: None, pressed, repeat: false, modifiers },
    ]);
    h.run_steps(2);
}

#[test]
fn temporary_zoom_keys_released_before_the_mouse_do_not_step_peers() {
    for spring in [false, true] {
        let mut h = harness();
        h.state_mut().ui.tool_options.zoom_all_windows = true;
        let peer = h.state().ui.views[1].clone();
        let before = h.state().ui.views[0].zoom;
        let (key_code, modifiers) = if spring {
            h.state_mut().ui.tool = Tool::Brush;
            (Key::Z, Modifiers::NONE)
        } else {
            (Key::Space, Modifiers { ctrl: !cfg!(target_os = "macos"), mac_cmd: cfg!(target_os = "macos"), command: true, ..Modifiers::NONE })
        };
        let at = shown(&h, 0).rect.center();
        point(&mut h, at, modifiers);
        key(&mut h, key_code, true, modifiers);
        assert_eq!(h.state().ui.tool, Tool::Zoom);
        button(&mut h, at, true, modifiers);
        key(&mut h, key_code, false, Modifiers::NONE);
        if spring {
            assert!(crate::spring_tools::holds(h.state(), Tool::Zoom), "the semantic spring owns the press through key-up");
        }
        button(&mut h, at, false, Modifiers::NONE);
        assert!(h.state().canvas_tools.last_response.as_ref().unwrap().zoom_step.is_none());
        h.run_steps(2);
        assert!(h.state().ui.views[0].zoom > before, "the actual temporary source click zoomed");
        assert_eq!(h.state().ui.views[1], peer, "temporary zoom preserves the peer");
        if spring {
            assert_eq!(h.state().ui.tool, Tool::Brush);
            assert!(h.state().ui.spring.is_none());
        }
    }
}

#[test]
fn scrub_rectangle_and_wheel_zoom_do_not_step_peers() {
    let mut h = harness();
    h.state_mut().ui.tool_options.zoom_all_windows = true;
    for scrubby in [true, false] {
        reset_views(&mut h, 0);
        h.state_mut().ui.tool_options.zoom_scrubby = scrubby;
        let peer = h.state().ui.views[1].clone();
        let before = h.state().ui.views[0].zoom;
        let at = shown(&h, 0).rect.center() - vec2(50.0, 35.0);
        point(&mut h, at, Modifiers::NONE);
        button(&mut h, at, true, Modifiers::NONE);
        point(&mut h, at + vec2(50.0, 35.0), Modifiers::NONE);
        point(&mut h, at + vec2(100.0, 70.0), Modifiers::NONE);
        button(&mut h, at + vec2(100.0, 70.0), false, Modifiers::NONE);
        h.run_steps(2);
        assert_ne!(h.state().ui.views[0].zoom, before, "the real drag zoomed (Scrubby Zoom: {scrubby})");
        assert_eq!(h.state().ui.views[1], peer);
    }
    reset_views(&mut h, 0);
    h.state_mut().session.edit_prefs(|prefs| prefs.general.zoom_with_scroll_wheel = true);
    let peer = h.state().ui.views[1].clone();
    let before = h.state().ui.views[0].zoom;
    let at = shown(&h, 0).rect.center();
    point(&mut h, at, Modifiers::NONE);
    h.event(craft::ui::testing::pointer::wheel(120.0));
    h.run_steps(2);
    assert_ne!(h.state().ui.views[0].zoom, before, "the actual canvas wheel path zoomed");
    assert_eq!(h.state().ui.views[1], peer);
}

#[test]
fn view_buttons_and_zoom_commands_stay_source_only() {
    let mut h = harness();
    h.state_mut().ui.tool_options.zoom_all_windows = true;
    for label in ["100%", "Fit Screen", "Fill Screen"] {
        reset_views(&mut h, 0);
        let peer = h.state().ui.views[1].clone();
        let before = h.state().ui.views[0].zoom;
        h.get_by_label(label).click();
        h.run_steps(3);
        assert_ne!(h.state().ui.views[0].zoom, before, "the actual {label} button ran");
        assert_eq!(h.state().ui.views[1], peer, "{label} is not part of Zoom All Windows");
    }
    for command in ["view.zoomIn", "view.zoomOut"] {
        reset_views(&mut h, 0);
        let peer = h.state().ui.views[1].clone();
        let before = h.state().ui.views[0].zoom;
        let ctx = h.ctx.clone();
        crate::menus::invoke(h.state_mut(), &ctx, command, json!({})).unwrap();
        h.run_steps(3);
        assert_ne!(h.state().ui.views[0].zoom, before, "the actual {command} menu route ran");
        assert_eq!(h.state().ui.views[1], peer);
    }
}

#[test]
fn a_source_click_at_its_limit_does_not_step_peers() {
    let mut h = harness();
    h.state_mut().ui.tool_options.zoom_all_windows = true;
    for (zoom, modifiers) in [(ZoomLadder::MAX as f32, Modifiers::NONE), (ZoomLadder::min(craft::behavior::kurbo::Size::new(2000.0, 1500.0)) as f32, Modifiers::ALT)] {
        reset_views(&mut h, 0);
        h.state_mut().ui.views[0].zoom = zoom;
        h.run_steps(2);
        let before = h.state().ui.views[0].zoom;
        let peer = h.state().ui.views[1].clone();
        let receipt = click(&mut h, 0, egui::Vec2::ZERO, modifiers);
        if zoom == ZoomLadder::MAX as f32 {
            assert_eq!(receipt, None);
        }
        assert_eq!(h.state().ui.views[0].zoom, before, "the actual stored source zoom is unchanged, including its f32 minimum");
        assert_eq!(h.state().ui.views[1], peer, "a source no-op must not trigger peers");
        // The next genuine rung away from the minimum must still follow; the guard is not a minimum-zoom veto.
        if modifiers.alt {
            assert_eq!(click(&mut h, 0, egui::Vec2::ZERO, Modifiers::NONE), Some(false));
            assert!(h.state().ui.views[0].zoom > before);
            near(h.state().ui.views[1].zoom, 0.5);
        }
    }
}

#[test]
fn a_real_source_click_preserves_peer_requests_and_bounded_cameras() {
    let peer_min = ZoomLadder::min(craft::behavior::kurbo::Size::new(3000.0, 2000.0)) as f32;
    for (out, peer_zoom, expected_peer_zoom) in [
        (false, 1.0 / 3.0, 0.5),
        (false, ZoomLadder::MAX as f32, ZoomLadder::MAX as f32),
        (true, peer_min, peer_min),
        (true, ZoomLadder::MAX as f32, 64.0),
        (false, peer_min, 0.002),
    ] {
        let mut app = app(egui::Context::default());
        super::testing::shown(&mut app);
        app.ui.tool_options.zoom_all_windows = true;
        app.ui.tool_options.zoom_in = !out;
        app.ui.views[0].clear_pending();
        app.ui.views[0].zoom = 0.5;
        let peer = &mut app.ui.views[1];
        peer.clear_pending();
        // A prior zoom has requested an ease, but the peer has not yet run the pass that consumes it.
        peer.request_zoom_animation(true);
        peer.zoom = peer_zoom;
        peer.request(ViewCommands::ActualPixels);
        peer.place(ViewCommands::FitOnScreen);
        peer.request(ViewCommands::ZoomOut);
        let before_peer = peer.clone();
        let ctx = app.ctx().clone();
        let rect = ToolHost::shown_space(&ctx, CanvasHost::main(0).canvas_id()).unwrap().view.rect;
        let at = rect.center();
        let mut frames = app.canvas_tools.frames.take().expect("the real warmup supplies the existing frame clock");
        // Show the real source canvas only. The peer's prior shown-space is genuine, but its next pass has not yet
        // legitimately executed its pending commands, so this observes only the follower's camera bookkeeping.
        for events in [
            vec![Event::PointerMoved(at)],
            vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }],
            vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }],
        ] {
            app.framework_before_input(&ctx);
            frames.frame_without_input(events, |ui| {
                let view = app.ui.views[0].clone();
                super::canvas_view(&mut app, ui, CanvasHost::main(0), rect, view, true);
            });
        }
        assert_eq!(app.canvas_tools.last_response.as_ref().unwrap().zoom_step.as_ref().map(|step| step.out), Some(out));
        near(app.ui.views[0].zoom, if out { 1.0 / 3.0 } else { 2.0 / 3.0 });
        if expected_peer_zoom == peer_zoom {
            assert_eq!(app.ui.views[1], before_peer, "a peer storage no-op preserves every camera, animation and queued-request field");
        } else {
            near(app.ui.views[1].zoom, expected_peer_zoom);
            assert_eq!(app.ui.views[1].pending, before_peer.pending, "preserve every command/place tag, value and position for the next peer pass");
        }
        assert!(app.ui.views[0].pending.is_empty());
    }
}
