//! #17310: own synthetic strokes exercise the measured short-axis spacing law.

use photocraft_color::PixelFormat;
use photocraft_raster::Surface;

use crate::{BrushSettings, Stroke, StrokePoint, dabs, render_stroke};

#[test]
fn flattened_tip_spacing_uses_the_short_axis_at_both_angles_and_directions() {
    for angle in [0.0, 90.0] {
        for vertical in [false, true] {
            let brush = BrushSettings { size: 40.0, roundness: 0.2, angle, spacing: 1.5, seed: 17310, ..BrushSettings::default() };
            let points = if vertical {
                vec![StrokePoint::new(100.0, 100.0, 1.0), StrokePoint::new(100.0, 400.0, 1.0)]
            } else {
                vec![StrokePoint::new(100.0, 100.0, 1.0), StrokePoint::new(400.0, 100.0, 1.0)]
            };
            let steps = dabs(&Stroke { brush, points });
            assert_eq!(steps.len(), 26, "{angle} degrees, vertical={vertical}");
            for (index, dab) in steps.iter().enumerate() {
                let coordinate = if vertical { dab.center.y } else { dab.center.x };
                assert!((coordinate - (100.0 + 12.0 * index as f64)).abs() < 1e-4);
            }
        }
    }
}

#[test]
fn half_roundness_stays_continuous_and_round_tips_keep_their_last_dab() {
    let points = [StrokePoint::new(100.0, 100.0, 1.0), StrokePoint::new(400.0, 100.0, 1.0)];
    let brush = BrushSettings { size: 40.0, roundness: 0.5, spacing: 1.5, seed: 17310, ..BrushSettings::default() };
    let mut surface = Surface::new(PixelFormat::RGBA32F);
    render_stroke(&mut surface, &brush, &points, None, false, 1.0);
    for x in 100..=400 {
        assert!(surface.rgba(x, 100)[3] > 0.5, "gap at {x}");
    }
    let round = BrushSettings { size: 20.0, roundness: 1.0, spacing: 2.0, ..brush };
    let steps = dabs(&Stroke { brush: round, points: points.to_vec() });
    assert_eq!(steps.len(), 8);
    assert!((steps.last().unwrap().center.x - 380.0).abs() < 1e-4);
}
