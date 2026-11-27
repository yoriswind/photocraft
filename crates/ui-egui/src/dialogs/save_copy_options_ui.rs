//! Copy-only content/profile choices, after the platform picker. The original document stays untouched.

use std::sync::Arc;

use craft::ui::contract::Bind;
use craft::ui::dialog::DialogId;
use craft::ui::patterns::form_dialogs::{DialogFields, Param, ParamDialog, ParamKind, ParamResult};
use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{DocId, Document};
use photocraft_engine::file_cmds::CopyOptions;
use serde_json::{Value, json};

use super::params::{self, ParamFamily};
use crate::{ExportSettings, PhotocraftApp};

pub(crate) const KEY: &str = "file.saveACopy.options";

/// Only the three content choices are remembered, in this app instance, across formats and opens.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Remembered {
    layers: Option<bool>,
    alpha: Option<bool>,
    spot: Option<bool>,
}

/// The reference RGB/8 control states. PDF retention remains a separately disclosed writer gap.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Capabilities {
    pub layers: bool,
    pub alpha: bool,
    pub spot: bool,
    pub profile: bool,
    pub content_default: bool,
}

pub(crate) fn capabilities(path: &str) -> Capabilities {
    let ext = photocraft_engine::file_cmds::extension(path).unwrap_or_default().to_ascii_lowercase();
    let (layers, alpha, spot, content_default) = match ext.as_str() {
        "psd" | "psb" | "tif" | "tiff" => (true, true, true, true),
        "pdf" => (true, true, true, false),
        "jp2" | "j2k" => (false, true, true, false),
        "raw" => (false, true, true, true),
        "bmp" => (false, true, false, false),
        "tga" | "icb" | "vda" | "vst" | "pxr" => (false, true, false, true),
        "dcm" | "dicom" => (true, false, false, true),
        _ => (false, false, false, false),
    };
    let profile = !matches!(ext.as_str(), "bmp" | "dcm" | "dicom" | "gif" | "iff" | "jp2" | "j2k" | "pcx" | "raw" | "pxr" | "pbm" | "pgm" | "ppm" | "sct" | "tga" | "icb" | "vda" | "vst");
    Capabilities { layers, alpha, spot, profile, content_default }
}

/// A prepared, immutable copy and its original export context travel through every format answer.
#[derive(Clone)]
pub(crate) struct CapturedCopy {
    pub doc: Arc<Document>,
    pub options: CopyOptions,
    pub settings: ExportSettings,
}

pub(crate) struct Pending {
    dialog: DialogId,
    original: Arc<Document>,
    path: String,
    settings: ExportSettings,
    initial: CopyOptions,
    caps: Capabilities,
    content: bool,
}

pub(crate) const FAMILY: ParamFamily = ParamFamily {
    check: Some(check),
    cancel: Some(cancel),
    document: Some(document),
    ..ParamFamily::new(|key| key == KEY, run)
};

fn document(value: &Value) -> Option<DocId> {
    value.get("doc").and_then(Value::as_u64).map(DocId)
}

fn initial(path: &str, caps: Capabilities, remembered: Remembered, content: bool) -> CopyOptions {
    CopyOptions {
        layers: content.then_some(caps.layers && remembered.layers.unwrap_or(caps.content_default)),
        alpha_channels: content.then_some(caps.alpha && remembered.alpha.unwrap_or(caps.content_default)),
        spot_colors: content.then_some(caps.spot && remembered.spot.unwrap_or(caps.content_default)),
        embed_profile: Some(caps.profile && CopyOptions::profile_default(path)),
    }
}

fn checkbox(key: &str, label: impl Into<craft::ui::runtime::env::Label>, value: bool, enabled: bool) -> Param {
    let row = Param::new(key, label, ParamKind::bool(value));
    if enabled { row } else { row.disabled() }
}

/// Opens the follow-up prompt. A second copy cannot replace a live pending copy or format answer.
pub(crate) fn open(app: &mut PhotocraftApp, original: Arc<Document>, path: String) -> Result<Value, String> {
    if app.save_options_open() {
        return Err(crate::SAVE_OPTIONS_OPEN.into());
    }
    let caps = capabilities(&path);
    // Other content modes were not measured; retain their existing content behavior.
    let content = original.mode == ColorMode::Rgb && original.depth == SampleType::U8;
    let initial = initial(&path, caps, app.copy_remembered, content);
    let profile = original.icc_profile.as_deref().and_then(|p| craft::cms::Profile::parse(p).ok()).map(|p| p.description).unwrap_or_else(|| tl!("Untagged").into());
    let profile = if profile.chars().count() > 32 { format!("{}...", profile.chars().take(29).collect::<String>()) } else { profile };
    let profile_label = crate::i18n::fmt(tl!("ICC Profile: {profile}"), &[("profile", &profile)]);
    let doc = original.id;
    let settings = app.with_document(doc, |app| Ok(app.save_settings()))?;
    let dialog = params::open_param_unsaved(app, KEY, DialogFields::new(), json!({"doc": doc.0}), move |fields| {
        let mut rows = Vec::new();
        if content {
            rows.extend([
                Param::section(tl!("Save:")),
                checkbox("alphaChannels", tl!("Alpha Channels"), initial.alpha_channels.unwrap_or(false), caps.alpha),
                checkbox("spotColors", tl!("Spot Colors"), initial.spot_colors.unwrap_or(false), caps.spot),
                checkbox("layers", tl!("Layers"), initial.layers.unwrap_or(false), caps.layers),
            ]);
        }
        rows.push(Param::section(tl!("Color:")));
        rows.push(checkbox("embedProfile", profile_label.clone(), initial.embed_profile.unwrap_or(false), caps.profile));
        ParamDialog::command(KEY, tl!("Save a Copy"), rows).values(Bind::bound(fields))
    });
    app.pending_copy = Some(Pending { dialog, original, path, settings, initial, caps, content });
    Ok(json!({"dialog": dialog.value().to_string(), "pending": true}))
}

fn check(app: &mut PhotocraftApp, result: &ParamResult) -> Result<(), craft::ui::runtime::env::Label> {
    if app.pending_copy.as_ref().is_none_or(|pending| pending.dialog != result.id) {
        return Err(tl!("This Save a Copy dialog is no longer active").into());
    }
    for key in ["layers", "alphaChannels", "spotColors", "embedProfile"] {
        if result.fields.contains(key) && result.fields.bool(key).is_none() {
            return Err(tl!("Save a Copy options must be true or false").into());
        }
    }
    Ok(())
}

fn cancel(app: &mut PhotocraftApp, _ctx: &egui::Context, id: DialogId) {
    if app.pending_copy.as_ref().is_some_and(|pending| pending.dialog == id) {
        app.pending_copy = None;
        app.ui.status = tl!("Save canceled").into();
        app.ui.status_error = false;
    }
}

/// OK captures all four decisions before a format prompt can take over; no option is reread from live UI state.
fn run(app: &mut PhotocraftApp, _ctx: &egui::Context, result: ParamResult) {
    if app.pending_copy.as_ref().is_none_or(|pending| pending.dialog != result.id) {
        return;
    }
    let Some(pending) = app.pending_copy.take() else { return };
    let value = |key: &str, default: Option<bool>| result.fields.bool(key).or(default);
    let options = CopyOptions {
        layers: pending.content.then(|| pending.caps.layers && value("layers", pending.initial.layers).unwrap_or(false)),
        alpha_channels: pending.content.then(|| pending.caps.alpha && value("alphaChannels", pending.initial.alpha_channels).unwrap_or(false)),
        spot_colors: pending.content.then(|| pending.caps.spot && value("spotColors", pending.initial.spot_colors).unwrap_or(false)),
        embed_profile: Some(pending.caps.profile && value("embedProfile", pending.initial.embed_profile).unwrap_or(false)),
    };
    if pending.content {
        // A disabled format never clears a user's last enabled choice.
        if pending.caps.layers { app.copy_remembered.layers = options.layers; }
        if pending.caps.alpha { app.copy_remembered.alpha = options.alpha_channels; }
        if pending.caps.spot { app.copy_remembered.spot = options.spot_colors; }
    }
    let outcome = (|| {
        let doc = Arc::new(options.prepare(&pending.original).map_err(|e| e.to_string())?);
        let mut settings = pending.settings;
        settings.embed_icc = options.embed_profile.unwrap_or_else(|| CopyOptions::profile_default(&pending.path));
        if let Some(layers) = options.layers { settings.tiff_layers = layers; }
        let copy = CapturedCopy { doc, options, settings };
        let parked = app.with_document(copy.doc.id, |app| {
            if crate::targa_options_ui::wants_prompt(app, &pending.path) {
                crate::targa_options_ui::park_copy(app, pending.path.clone(), copy.clone())?;
                return Ok(true);
            }
            if crate::dds_options_ui::wants_prompt(&pending.path) {
                crate::dds_options_ui::park_copy(app, pending.path.clone(), copy.clone())?;
                return Ok(true);
            }
            if crate::jpeg_options_ui::wants_prompt(app, &pending.path) {
                crate::jpeg_options_ui::park_copy(app, pending.path.clone(), copy.clone())?;
                return Ok(true);
            }
            if crate::tiff_options_ui::wants_prompt(app, &pending.path) {
                crate::tiff_options_ui::park_copy(app, pending.path.clone(), copy.clone())?;
                return Ok(true);
            }
            Ok(false)
        })?;
        if parked { return Ok(Value::Null); }
        let reply = crate::view_cmds::write_copy_settings(app, &copy.doc, &pending.path, &copy.settings)?;
        let mut step = json!({"path": pending.path});
        options.record(&mut step);
        photocraft_engine::actions_cmds::note_automation_step(&mut app.session, "file.saveACopy", &step);
        app.session.journal.push(("file.saveACopy".into(), step));
        Ok(reply)
    })();
    super::alerts::report(app, outcome);
}
