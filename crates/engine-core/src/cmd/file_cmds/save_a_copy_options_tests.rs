//! Synthetic documents and native ATN fixtures for the copy-only content/profile contract.

use super::{CopyOptions, SaveOpts, encode};
use crate::actions_cmds::{Action, ActionStep};
use photocraft_doc::{AlphaChannel, Document};
use serde_json::{Value, json};

fn document() -> Document {
    let mut s = crate::session();
    s.execute("file.new", json!({"width": 2, "height": 2, "name": "Original", "background": "transparent"})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Paint"})).unwrap();
    s.execute("select.rect", json!({"x": 0, "y": 0, "width": 1, "height": 2})).unwrap();
    s.execute("edit.fill", json!({"color": "#4675a4"})).unwrap();
    s.execute("select.saveSelection", json!({"name": "Mask"})).unwrap();
    let mut doc = (*s.active().unwrap().doc).clone();
    let mut spot = AlphaChannel::new("Ink", doc.channels[0].surface.clone());
    spot.spot = Some((photocraft_color::Color::rgb(0.8, 0.1, 0.2), 0.5));
    doc.channels.push(spot);
    doc.icc_profile = Some(craft_cms::Builtin::Srgb.profile().to_bytes());
    doc
}

#[test]
fn flattened_copies_keep_alpha_and_spot_channels_independent_and_leave_the_source_intact() {
    let original = document();
    let original_layers = original.layers.clone();
    let original_channels = original.channels.clone();
    let expected = photocraft_compose::flatten_to_surface(&original, original.pixel_format(), Some([1.0, 1.0, 1.0]));
    for (alpha, spot) in [(false, false), (true, false), (false, true), (true, true)] {
        let options = CopyOptions { layers: Some(false), alpha_channels: Some(alpha), spot_colors: Some(spot), embed_profile: Some(true) };
        let prepared = options.prepare(&original).unwrap();
        let (bytes, _) = encode(&prepared, "copy.psd", options.apply("copy.psd", SaveOpts::default())).unwrap();
        let reopened = photocraft_io::import("copy.psd", &bytes).unwrap().document;
        assert_eq!(reopened.layers.len(), 1);
        assert!(reopened.background().is_some(), "a real Background, not only a layer named Background");
        assert_eq!(reopened.channels.iter().filter(|channel| channel.spot.is_none()).count(), usize::from(alpha));
        assert_eq!(reopened.channels.iter().filter(|channel| channel.spot.is_some()).count(), usize::from(spot));
        let actual = photocraft_compose::flatten_to_surface(&reopened, reopened.pixel_format(), None);
        assert_eq!(actual.read_region(reopened.bounds()), expected.read_region(original.bounds()));
    }
    assert_eq!(original.layers, original_layers);
    assert_eq!(original.channels, original_channels);
    assert!(original.icc_profile.is_some());
}

#[test]
fn keeping_layers_and_old_absent_content_choices_preserves_the_layered_document() {
    let original = document();
    for options in [CopyOptions::default(), CopyOptions { layers: Some(true), ..Default::default() }] {
        let prepared = options.prepare(&original).unwrap();
        assert_eq!(prepared.layers, original.layers);
        assert_eq!(prepared.channels, original.channels);
        let (bytes, _) = encode(&prepared, "copy.psb", options.apply("copy.psb", SaveOpts::default())).unwrap();
        let reopened = photocraft_io::import("copy.psb", &bytes).unwrap().document;
        assert_eq!(reopened.layers.len(), original.layers.len());
        assert_eq!(reopened.channels.len(), original.channels.len());
    }
}

#[test]
fn png_profile_defaults_off_but_an_explicit_choice_survives_encoding_without_changing_source() {
    let original = document();
    for (choice, embedded) in [(None, false), (Some(false), false), (Some(true), true)] {
        let options = CopyOptions { embed_profile: choice, ..Default::default() };
        let (bytes, _) = encode(&original, "copy.png", options.apply("copy.png", SaveOpts::default())).unwrap();
        let reopened = photocraft_io::import("copy.png", &bytes).unwrap().document;
        assert_eq!(reopened.icc_profile.is_some(), embedded);
        if let Some(profile) = reopened.icc_profile {
            assert_eq!(profile, original.icc_profile.clone().unwrap());
        }
    }
    assert!(original.icc_profile.is_some());
}

#[test]
fn native_copies_omit_only_the_emitted_profile_and_keep_layers_channels_and_pixels() {
    let original = document();
    for path in ["copy.psd", "copy.psb", "copy.tiff"] {
        let mut outputs = Vec::new();
        for embed in [false, true] {
            let options = CopyOptions { layers: Some(true), embed_profile: Some(embed), ..Default::default() };
            let (bytes, _) = encode(&original, path, options.apply(path, SaveOpts::default())).unwrap();
            let reopened = photocraft_io::import(path, &bytes).unwrap().document;
            assert_eq!(reopened.icc_profile.is_some(), embed, "{path}");
            assert_eq!(reopened.layers.len(), original.layers.len(), "{path}");
            assert_eq!(reopened.channels.len(), original.channels.len(), "{path}");
            outputs.push(photocraft_compose::flatten_to_surface(&reopened, reopened.pixel_format(), None).read_region(reopened.bounds()));
        }
        assert_eq!(outputs[0], outputs[1], "embedding must not reinterpret source colours: {path}");
    }
    assert!(original.icc_profile.is_some());
}

#[test]
fn bad_copy_flags_are_refused_before_any_writer_or_document_effect() {
    let mut s = crate::session();
    s.execute("file.new", json!({"width": 2, "height": 2})).unwrap();
    let doc = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    for key in ["layers", "alphaChannels", "spotColors", "embedProfile"] {
        for value in [Value::Null, json!("false"), json!(0), json!([]), json!({})] {
            let mut params = json!({"path": "must-not-be-written.psd"});
            params[key] = value;
            let error = s.execute("file.saveACopy", params).unwrap_err().to_string();
            assert!(error.contains(key), "{error}");
            assert_eq!(s.active().unwrap().doc, doc);
            assert_eq!(s.active().unwrap().revision, revision);
        }
    }
}

#[test]
fn copy_flags_roundtrip_through_native_atn_without_changing_save_as() {
    let params = json!({"path": "copy.psd", "layers": false, "alphaChannels": true, "spotColors": false, "embedProfile": true});
    let action = Action::new("Copy", vec![ActionStep::new("file.saveACopy", params.clone())]);
    let (bytes, skipped) = crate::atn_cmds::export("Copies", &[&action], &[], false).unwrap();
    assert!(skipped.is_empty());
    let file = craft_psd::atn::parse(&bytes).unwrap();
    let native = file.actions[0].items[0].descriptor.as_ref().unwrap();
    use craft_psd::descriptor::Value as D;
    assert_eq!(native.get("spot"), Some(&D::Boolean(false)));
    assert_eq!(native.get("embedProfiles"), Some(&D::Boolean(true)));
    let imported = crate::atn_cmds::import(&bytes).unwrap();
    assert!(imported.unsupported.is_empty());
    assert_eq!(imported.actions[0].steps[0].params, params);
    let mut aliased = file.clone();
    aliased.actions[0].items[0].descriptor = Some(native.clone().with("spotColor", D::Boolean(false)));
    let imported = crate::atn_cmds::import(&craft_psd::atn::write(&aliased).unwrap()).unwrap();
    assert_eq!(imported.actions[0].steps[0].params, params, "the accepted native alias has the same meaning");
    aliased.actions[0].items[0].descriptor = Some(native.clone().with("spotColor", D::Boolean(true)));
    assert_eq!(crate::atn_cmds::import(&craft_psd::atn::write(&aliased).unwrap()).unwrap().unsupported.len(), 1);
    aliased.actions[0].items[0].descriptor = Some(native.clone().with("embedProfiles", D::Integer(1)));
    assert_eq!(crate::atn_cmds::import(&craft_psd::atn::write(&aliased).unwrap()).unwrap().unsupported.len(), 1);
    let save_as = Action::new("Save As", vec![ActionStep::new("file.saveAs", json!({"path": "original.psd"}))]);
    let (bytes, _) = crate::atn_cmds::export("Saves", &[&save_as], &[], false).unwrap();
    let imported = crate::atn_cmds::import(&bytes).unwrap();
    assert_eq!(imported.actions[0].steps[0].command, "file.saveAs");
    assert_eq!(imported.actions[0].steps[0].params, json!({"path": "original.psd"}));
}

#[test]
fn an_unmeasured_sixteen_bit_copy_keeps_the_existing_headless_flatten_behavior() {
    let mut original = Document::new("16-bit", photocraft_doc::Size::new(2, 2), photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U16);
    original.layers.push(photocraft_doc::Layer::raster("Transparent", original.pixel_format()));
    let expected = super::flattened_checked(&original, original.pixel_format(), "file.saveACopy").unwrap();
    let copy = CopyOptions { layers: Some(false), ..Default::default() }.prepare(&original).unwrap();
    assert_eq!(copy.layers.len(), 1);
    assert_eq!(copy.layers[0].surface().unwrap(), &expected);
    assert!(copy.background().is_none(), "the unmeasured mode keeps the old unlocked composite layer");
}
