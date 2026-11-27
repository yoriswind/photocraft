//! Save a Copy goes through the real option host and encoder; no exporter stub discards settings.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use craft::ui::dialog::DialogId;
use craft::ui::patterns::form_dialogs::HostedParamDialog;
use photocraft_doc::Document;
use serde_json::{Value, json};

use crate::{PhotocraftApp, Services, save_copy_options_ui};

type Written = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

fn new_app(files: Option<Arc<dyn craft::host::Files>>) -> (Box<PhotocraftApp>, Written) {
    let written = Written::default();
    let output = written.clone();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services {
        files,
        export: Some(Box::new(|doc, path, settings| {
            photocraft_io::export(doc, path, &settings.io_options()).map(|result| (result.bytes, result.warnings)).map_err(|error| error.to_string())
        })),
        write: Some(Box::new(move |path, bytes| { output.borrow_mut().push((path.into(), bytes.to_vec())); Ok(()) })),
        ..Default::default()
    });
    app.run("file.new", json!({"width": 2, "height": 2, "name": "Original"})).unwrap();
    app.run("layer.new.layer", json!({"name": "Paint"})).unwrap();
    app.run("edit.fill", json!({"color": "#4675a4"})).unwrap();
    app.run("edit.assignProfile", json!({"profile": "display-p3"})).unwrap();
    (app, written)
}

fn identity(app: &PhotocraftApp) -> (Arc<Document>, Option<String>, u64, u64) {
    let state = app.session.active().unwrap();
    (state.doc.clone(), state.path.clone(), state.revision, state.saved_revision)
}

fn open(app: &mut PhotocraftApp, path: &str) -> DialogId {
    let original = app.session.active().unwrap().doc.clone();
    save_copy_options_ui::open(app, original, path.into()).unwrap();
    crate::dialogs::testing::show(app);
    app.framework.dialogs.stack().top().unwrap()
}

fn field(app: &PhotocraftApp, id: DialogId, key: &str) -> Option<bool> {
    app.framework.dialogs.get_by_id::<HostedParamDialog>(id).unwrap().fields().bool(key)
}

fn set(app: &mut PhotocraftApp, id: DialogId, key: &str, value: bool) {
    let ctx = app.ctx().clone();
    app.framework.dialogs.set_json_field(&ctx, id, key, json!(value)).unwrap();
}

fn answer(app: &mut PhotocraftApp, id: DialogId) {
    crate::dialogs::testing::confirm(app, id).unwrap();
    assert!(!app.ui.status_error, "{}", app.ui.status);
}

fn reopened(written: &Written) -> Document {
    let output = written.borrow();
    let (path, bytes) = output.last().unwrap();
    photocraft_io::import(path, bytes).unwrap().document
}

#[test]
fn picker_then_copy_form_uses_png_profile_default_and_preserves_the_open_document() {
    let files = craft::host::ScriptedFiles::answering([craft::host::Picked::Paths(vec!["copy.png".into()])]);
    let (mut app, written) = new_app(Some(Arc::new(files)));
    let before = identity(&app);
    let ctx = app.ctx().clone();
    crate::menus::invoke(&mut app, &ctx, "file.saveACopy", json!({})).unwrap();
    app.poll_file_dialog();
    crate::dialogs::testing::show(&mut app);
    assert!(written.borrow().is_empty());
    let id = app.framework.dialogs.stack().top().unwrap();
    assert_eq!(field(&app, id, "embedProfile"), Some(false));
    answer(&mut app, id);
    assert_eq!(written.borrow().len(), 1);
    assert!(reopened(&written).icc_profile.is_none());
    assert_eq!(identity(&app), before);
    assert!(app.parked_save.is_none());
    let (_, params) = app.session.journal.last().unwrap();
    assert_eq!(params["embedProfile"], false);
}

#[test]
fn explicit_profile_and_content_choices_survive_recorded_desktop_replay() {
    let (mut app, written) = new_app(None);
    let before = identity(&app);
    let id = open(&mut app, "copy.psd");
    set(&mut app, id, "layers", false);
    set(&mut app, id, "alphaChannels", false);
    set(&mut app, id, "spotColors", true);
    set(&mut app, id, "embedProfile", true);
    answer(&mut app, id);
    assert!(app.parked_save.is_none(), "a flat PSD needs no PSD format prompt");
    let params = app.session.journal.last().unwrap().1.clone();
    assert_eq!(params, json!({"path": "copy.psd", "layers": false, "alphaChannels": false, "spotColors": true, "embedProfile": true}));
    let first = reopened(&written);
    assert!(first.background().is_some());
    assert_eq!(first.layers.len(), 1);
    assert_eq!(first.icc_profile, before.0.icc_profile);
    app.replay_save("file.saveACopy", &params).unwrap();
    let replay = reopened(&written);
    assert_eq!(replay.layers.len(), first.layers.len());
    assert_eq!(replay.icc_profile, first.icc_profile);
    assert_eq!(identity(&app), before);
}

#[test]
fn content_choices_are_shared_across_formats_and_disabled_formats_do_not_clear_them() {
    let (mut app, written) = new_app(None);
    let id = open(&mut app, "first.psd");
    set(&mut app, id, "layers", false);
    set(&mut app, id, "alphaChannels", false);
    set(&mut app, id, "spotColors", true);
    answer(&mut app, id);
    let id = open(&mut app, "flat.png");
    assert_eq!(field(&app, id, "layers"), Some(false));
    assert_eq!(field(&app, id, "spotColors"), Some(false));
    answer(&mut app, id);
    let id = open(&mut app, "again.tiff");
    assert_eq!(field(&app, id, "layers"), Some(false));
    assert_eq!(field(&app, id, "alphaChannels"), Some(false));
    assert_eq!(field(&app, id, "spotColors"), Some(true));
    crate::dialogs::testing::cancel(&mut app, id).unwrap();
    assert_eq!(written.borrow().len(), 2);
    assert!(app.pending_copy.is_none());
    // Preferences are per app run, not a persisted setting and not an ICC state cache.
    let (mut fresh, _) = new_app(None);
    let id = open(&mut fresh, "fresh.psd");
    assert_eq!(field(&fresh, id, "layers"), Some(true));
}

#[test]
fn all_four_format_handoffs_write_the_captured_copy_and_record_all_four_choices() {
    for path in ["copy.tga", "copy.dds", "copy.jpg", "copy.tiff"] {
        let (mut app, written) = new_app(None);
        let id = open(&mut app, path);
        if save_copy_options_ui::capabilities(path).profile { set(&mut app, id, "embedProfile", false); }
        if path.ends_with("tiff") { set(&mut app, id, "layers", false); }
        answer(&mut app, id);
        assert!(written.borrow().is_empty(), "format answer must precede write: {path}");
        let copy = app.parked_save.as_ref().unwrap().copy_source.as_ref().unwrap().clone();
        let captured = copy.doc.clone();
        app.run("edit.fill", json!({"color": "#ffff00"})).unwrap();
        let current = identity(&app);
        let ctx = app.ctx().clone();
        crate::format_prompts::confirm(&mut app, &ctx);
        assert!(!app.ui.status_error, "{path}: {}", app.ui.status);
        assert_eq!(written.borrow().len(), 1);
        assert_eq!(identity(&app), current);
        let (_, params) = app.session.journal.last().unwrap();
        for key in ["layers", "alphaChannels", "spotColors", "embedProfile"] { assert!(params[key].is_boolean(), "{path}: {key}"); }
        assert_eq!(params["embedProfile"], false);
        let actual = reopened(&written);
        assert!(actual.icc_profile.is_none(), "{path}");
        let expected = photocraft_compose::flatten_to_surface(&captured, captured.pixel_format(), None).read_region(captured.bounds());
        let pixels = photocraft_compose::flatten_to_surface(&actual, actual.pixel_format(), None).read_region(actual.bounds());
        assert_eq!(pixels.len(), expected.len());
        let tolerance = if path.ends_with("jpg") { 0.08 } else { 1.0 / 255.0 };
        assert!(pixels.iter().zip(expected).all(|(actual, expected)| (actual - expected).abs() <= tolerance), "the parked snapshot, not a later edit: {path}");
        assert!(app.parked_save.is_none());
    }
}

#[test]
fn cancel_or_bad_desktop_copy_flags_never_write_or_change_save_identity() {
    let (mut app, written) = new_app(None);
    let before = identity(&app);
    let id = open(&mut app, "cancel.psd");
    assert!(save_copy_options_ui::open(&mut app, before.0.clone(), "replace.psd".into()).is_err());
    assert!(crate::jpeg_options_ui::park(&mut app, "replace.jpg".into(), false, "file.saveAs").is_err());
    crate::dialogs::testing::cancel(&mut app, id).unwrap();
    assert!(app.pending_copy.is_none());
    for key in ["layers", "alphaChannels", "spotColors", "embedProfile"] {
        let mut params = json!({"path": "bad.psd"});
        params[key] = Value::Null;
        assert!(app.replay_save("file.saveACopy", &params).is_err());
    }
    assert!(written.borrow().is_empty());
    assert_eq!(identity(&app), before);
}

#[test]
fn copy_profile_off_does_not_change_save_as_or_export_as_profile_policy() {
    let (mut app, written) = new_app(None);
    let id = open(&mut app, "copy.png");
    answer(&mut app, id);
    assert!(reopened(&written).icc_profile.is_none());
    app.save_as(Some("original.png".into())).unwrap();
    assert!(reopened(&written).icc_profile.is_some());
    crate::export_dialog::replay(&mut app, &json!({"format": "png", "scale": 100, "profile": "document", "embedProfile": true, "path": "export.png"})).unwrap();
    assert!(reopened(&written).icc_profile.is_some());
}

#[test]
fn keyboard_profile_choice_is_exposed_and_reaches_the_written_copy() {
    use egui::accesskit::{Role, Toggled};
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    let (mut app, written) = new_app(None);
    let before = identity(&app);
    open(&mut app, "keyboard.png");
    let mut h = egui_kittest::Harness::builder().with_context(app.ctx().clone()).with_size(egui::vec2(800.0, 600.0)).build_ui_state(
        |ui, app| crate::dialogs::run(app, ui), app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::AppTheme::pro());
    craft::ui::testing::windows::set_os(&h.ctx, craft::host::target::Os::Windows);
    h.run_steps(2);
    let label = format!("ICC Profile: {}", craft::cms::Builtin::DisplayP3.profile().description);
    assert_eq!(h.get_by_role_and_label(Role::CheckBox, &label).accesskit_node().toggled(), Some(Toggled::False));
    h.get_by_role_and_label(Role::CheckBox, &label).focus();
    h.run_steps(2);
    h.key_press(egui::Key::Space);
    h.run_steps(2);
    assert_eq!(h.get_by_role_and_label(Role::CheckBox, &label).accesskit_node().toggled(), Some(Toggled::True));
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
    assert_eq!(written.borrow().len(), 1);
    assert!(reopened(&written).icc_profile.is_some());
    assert_eq!(identity(h.state()), before);
    assert_eq!(h.state().session.journal.last().unwrap().1["embedProfile"], true);
}
