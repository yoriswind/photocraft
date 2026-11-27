//! #13300: Polygon tool History names and single-property edits of live polygons.

use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = crate::session();
    s.execute("file.new", json!({"width": 200, "height": 160, "background": "white"})).unwrap();
    s
}

fn polygon(sides: u32) -> Value {
    json!({"kind": "polygon", "rect": [20, 20, 100, 80], "sides": sides})
}

fn create(s: &mut Session, params: Value) -> u64 {
    s.execute("shape.create", params).unwrap()["layer"].as_u64().unwrap()
}

fn shape(s: &Session, id: u64) -> &ShapeLayer {
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(LayerId(id)).unwrap().content else { panic!("expected a shape layer") };
    sh
}

fn steps(s: &Session) -> usize {
    s.active().unwrap().history.past_len()
}

fn last_step(s: &Session) -> Option<&str> {
    s.active().unwrap().history.undo_label()
}

#[test]
fn polygon_tool_creation_is_one_step_named_polygon_tool() {
    let mut s = session();
    let mut params = polygon(5);
    params["historyName"] = json!("Polygon Tool");
    let n = steps(&s);
    let id = create(&mut s, params);
    assert_eq!(steps(&s), n + 1);
    assert_eq!(last_step(&s), Some("Polygon Tool"));
    assert_eq!(s.active().unwrap().doc.layer(LayerId(id)).unwrap().name, "Polygon 1");
    assert!(matches!(shape(&s, id).live, Some(LiveShape::Polygon { sides: 5, star_ratio, corner_radius, .. }) if star_ratio == 1.0 && corner_radius == 0.0));
    assert_eq!(shape(&s, id).path.subpaths[0].knots.len(), 5);
}

#[test]
fn each_polygon_path_drag_is_one_named_step_and_adds_a_component() {
    let mut s = session();
    let layers = s.active().unwrap().doc.walk().len();
    for (i, x) in [20, 80].into_iter().enumerate() {
        let mut params = polygon(5);
        params["rect"] = json!([x, 20, 60, 60]);
        // Match finish_drag: assemble the complete Work Path before calling path.set, without op.
        let mut path = s.active().unwrap().doc.work_path.clone().unwrap_or_default();
        path.subpaths.extend(shape_path(&params).unwrap().subpaths);
        let n = steps(&s);
        s.execute("path.set", json!({"name": "work", "path": path_json(&path), "historyName": "Polygon Tool"})).unwrap();
        assert_eq!(steps(&s), n + 1);
        assert_eq!(last_step(&s), Some("Polygon Tool"));
        let work = s.active().unwrap().doc.work_path.as_ref().unwrap();
        assert_eq!(work.subpaths.len(), i + 1);
        assert_eq!(work.subpaths.iter().map(|sp| sp.knots.len()).sum::<usize>(), (i + 1) * 5);
        assert_eq!(s.active().unwrap().doc.walk().len(), layers, "Path mode adds no shape layer");
    }
}

#[test]
fn changing_only_polygon_sides_names_one_step_and_undo_redo_restore_it() {
    let mut s = session();
    let id = create(&mut s, polygon(5));
    let before = shape(&s, id).clone();
    let n = steps(&s);
    s.execute("shape.edit", json!({"layer": id, "sides": 8})).unwrap();
    assert_eq!(steps(&s), n + 1);
    assert_eq!(last_step(&s), Some("Change Polygon Sides"));
    assert!(matches!(shape(&s, id).live, Some(LiveShape::Polygon { sides: 8, corner_radius, .. }) if corner_radius == 0.0));
    assert_eq!(shape(&s, id).path.subpaths[0].knots.len(), 8);
    let after = shape(&s, id).clone();
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(shape(&s, id).live, before.live);
    assert_eq!(shape(&s, id).path, before.path);
    assert_eq!(s.active().unwrap().history.redo_label(), Some("Change Polygon Sides"));
    s.execute("edit.redo", json!({})).unwrap();
    assert_eq!(shape(&s, id).live, after.live);
    assert_eq!(shape(&s, id).path, after.path);
    assert_eq!(last_step(&s), Some("Change Polygon Sides"));
}

#[test]
fn rounding_three_and_five_sided_polygons_names_one_step() {
    for sides in [3, 5] {
        let mut s = session();
        let id = create(&mut s, polygon(sides));
        let n = steps(&s);
        s.execute("shape.edit", json!({"layer": id, "cornerRadius": 15})).unwrap();
        assert_eq!(steps(&s), n + 1);
        assert_eq!(last_step(&s), Some("Change Rounded Polygon"));
        assert!(matches!(shape(&s, id).live, Some(LiveShape::Polygon { corner_radius, .. }) if corner_radius == 15.0));
        assert_eq!(shape(&s, id).path.subpaths[0].knots.len(), (sides * 2) as usize);
    }
}

#[test]
fn polygon_field_drags_keep_one_step_and_ignore_dispatch_metadata() {
    for (key, values, name) in [("sides", [6, 7, 8], "Change Polygon Sides"), ("cornerRadius", [5, 10, 15], "Change Rounded Polygon")] {
        let mut s = session();
        let id = create(&mut s, polygon(5));
        let n = steps(&s);
        for v in values {
            let mut params = json!({"layer": id, "coalesce": "polygon-field-drag", "__source": "Properties"});
            params[key] = json!(v);
            s.execute("shape.edit", params).unwrap();
        }
        assert_eq!(steps(&s), n + 1);
        assert_eq!(last_step(&s), Some(name));
    }
}

#[test]
fn unchanged_or_normalized_polygon_values_keep_existing_edit_shape_behavior() {
    for params in [json!({"sides": 5}), json!({"sides": 5.2}), json!({"cornerRadius": 0}), json!({"cornerRadius": -5})] {
        let mut s = session();
        let id = create(&mut s, polygon(5));
        let before = shape(&s, id).clone();
        let n = steps(&s);
        s.execute("shape.edit", params.clone()).unwrap();
        // shape.edit already records these re-entered values; this fix does not change that policy.
        assert_eq!(steps(&s), n + 1, "{params}");
        assert_eq!(last_step(&s), Some("Edit Shape"), "{params}");
        assert_eq!(shape(&s, id).live, before.live);
        assert_eq!(shape(&s, id).path, before.path);
    }
}

#[test]
fn combined_and_other_polygon_edits_keep_edit_shape() {
    for params in [
        json!({"sides": 8, "cornerRadius": 15}),
        json!({"sides": 8, "fill": "#ff0000"}),
        json!({"cornerRadius": 15, "name": "Renamed"}),
        json!({"kind": "polygon", "sides": 8}),
        json!({"rect": [30, 20, 100, 80]}),
        json!({"starRatio": 0.5}),
    ] {
        let mut s = session();
        create(&mut s, polygon(5));
        let n = steps(&s);
        s.execute("shape.edit", params.clone()).unwrap();
        assert_eq!(steps(&s), n + 1, "{params}");
        assert_eq!(last_step(&s), Some("Edit Shape"), "{params}");
    }
}

#[test]
fn star_sides_and_radius_keep_edit_shape() {
    for params in [json!({"sides": 8}), json!({"cornerRadius": 15})] {
        let mut s = session();
        let id = create(&mut s, json!({"kind": "star", "rect": [20, 20, 100, 80], "sides": 5, "starRatio": 0.5}));
        let n = steps(&s);
        s.execute("shape.edit", params).unwrap();
        assert_eq!(steps(&s), n + 1);
        assert_eq!(last_step(&s), Some("Edit Shape"));
        assert!(matches!(shape(&s, id).live, Some(LiveShape::Polygon { star_ratio, .. }) if star_ratio == 0.5));
    }
}

#[test]
fn explicit_polygon_target_is_used_when_a_star_is_active() {
    let mut s = session();
    let polygon = create(&mut s, polygon(5));
    let star = create(&mut s, json!({"kind": "star", "rect": [20, 20, 100, 80]}));
    let before = shape(&s, star).clone();
    s.execute("shape.edit", json!({"layer": polygon, "sides": 8})).unwrap();
    assert_eq!(last_step(&s), Some("Change Polygon Sides"));
    assert!(matches!(shape(&s, polygon).live, Some(LiveShape::Polygon { sides: 8, .. })));
    assert_eq!(shape(&s, star).live, before.live);
    assert_eq!(shape(&s, star).path, before.path);
}

#[test]
fn plain_paths_and_refused_polygon_edits_do_not_gain_special_history() {
    let mut s = session();
    let outline = shape_path(&polygon(5)).unwrap();
    let id = create(&mut s, json!({"kind": "path", "path": path_json(&outline)}));
    let before = shape(&s, id).clone();
    let n = steps(&s);
    assert!(s.execute("shape.edit", json!({"layer": id, "sides": 8})).is_err());
    assert_eq!(steps(&s), n);
    assert_eq!(shape(&s, id).live, before.live);
    assert_eq!(shape(&s, id).path, before.path);
    let id = create(&mut s, polygon(5));
    let before = shape(&s, id).clone();
    let n = steps(&s);
    assert!(s.execute("shape.edit", json!({"layer": id, "cornerRadius": 15, "rect": [1e100, 0, 100, 80]})).is_err());
    assert_eq!(steps(&s), n);
    assert_eq!(shape(&s, id).live, before.live);
    assert_eq!(shape(&s, id).path, before.path);
}

#[test]
fn command_defaults_additions_and_unknown_history_names_keep_their_contract() {
    let mut s = session();
    let id = create(&mut s, polygon(5));
    assert_eq!(last_step(&s), Some("New Shape Layer"));
    let path = path_json(&shape_path(&polygon(5)).unwrap());
    s.execute("path.set", json!({"name": "work", "path": path})).unwrap();
    assert_eq!(last_step(&s), Some("Work Path"));
    let mut addition = polygon(5);
    addition["historyName"] = json!("Polygon Tool");
    addition["addTo"] = json!(id);
    s.execute("shape.create", addition).unwrap();
    assert_eq!(last_step(&s), Some("Add Shape"));
    assert!(shape(&s, id).live.is_none());
    let n = steps(&s);
    let mut invalid = polygon(5);
    invalid["historyName"] = json!("Unrecognized Polygon History");
    assert!(s.execute("shape.create", invalid).is_err());
    assert!(s.execute("path.set", json!({"name": "work", "path": path, "historyName": 7})).is_err());
    assert_eq!(steps(&s), n);
}

#[test]
fn an_explicit_canvas_label_leaves_headless_polygon_labels_available() {
    let mut s = session();
    let id = create(&mut s, polygon(5));
    let n = steps(&s);
    s.execute("shape.edit", json!({"layer": id, "cornerRadius": 15, "historyName": "Edit Shape"})).unwrap();
    assert_eq!(steps(&s), n + 1);
    assert_eq!(last_step(&s), Some("Edit Shape"));
    assert!(matches!(shape(&s, id).live, Some(LiveShape::Polygon { corner_radius, .. }) if corner_radius == 15.0));
    s.execute("shape.edit", json!({"layer": id, "cornerRadius": 20, "historyName": null})).unwrap();
    assert_eq!(last_step(&s), Some("Change Rounded Polygon"));
    s.execute("shape.edit", json!({"layer": id, "sides": 8})).unwrap();
    assert_eq!(last_step(&s), Some("Change Polygon Sides"));
}

#[test]
fn invalid_edit_history_names_leave_polygon_and_history_unchanged() {
    let mut s = session();
    let id = create(&mut s, polygon(5));
    let before = s.active().unwrap().doc.clone();
    let n = steps(&s);
    for history in [json!("Polygon Tool"), json!("Arbitrary label"), json!(7)] {
        assert!(s.execute("shape.edit", json!({"layer": id, "cornerRadius": 15, "historyName": history})).is_err());
        assert_eq!(steps(&s), n);
        assert_eq!(s.active().unwrap().doc, before);
    }
}
