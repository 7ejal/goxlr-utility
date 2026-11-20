use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};

// Original P:D24..D36 with boot coefficient banks Y:40 / Y:D0.
// Each original stage doubles its five-term MAC; preserve that scale here.
fn response_db(hz: f64, rate: u32) -> Option<f64> {
    if !matches!(rate, 44_100 | 48_000) || !hz.is_finite() || hz <= 0. || hz >= f64::from(rate) / 2.
    {
        return None;
    }
    let words: [u32; 10] = match rate {
        48_000 => [
            14308108, 0, 2469082, 14355633, 0, 8388651, 4194261, 4194304, 10965418, 2516582,
        ],
        44_100 => [
            14424263, 0, 2352929, 14483425, 0, 8388651, 4194261, 4194304, 11214961, 2412068,
        ],
        _ => return None,
    };
    let c = words.map(|word| {
        f64::from(if word & 0x800000 == 0 {
            word as i32
        } else {
            word as i32 - 0x1000000
        }) / 8388608.
    });
    let w = 2. * std::f64::consts::PI * hz / f64::from(rate);
    let (sin, cos) = w.sin_cos();
    let (sin2, cos2) = (2. * w).sin_cos();
    let magnitude = |a: f64, b: f64, d: f64| (a + b * cos + d * cos2).hypot(-b * sin - d * sin2);
    let gain = 4. * magnitude(c[2], c[0], c[1]) * magnitude(c[7], c[5], c[6])
        / (magnitude(1., 2. * c[3], 2. * c[4]) * magnitude(1., 2. * c[8], 2. * c[9]));
    let db = 20. * gain.log10();
    db.is_finite().then_some(db)
}

pub fn show(ui: &mut egui::Ui, rate: &mut u32) {
    ui.label("Recovered detector model — not an acoustic measurement or output EQ.");
    egui::ComboBox::from_id_salt("detector-bank")
        .selected_text(format!("{rate} Hz coefficient bank"))
        .show_ui(ui, |ui| {
            ui.selectable_value(rate, 48_000, "48000 Hz coefficient bank");
            ui.selectable_value(rate, 44_100, "44100 Hz coefficient bank");
        });
    ui.weak("Model selection changes this graph only. It does not measure or change the hardware clock.");
    if !matches!(*rate, 44_100 | 48_000) {
        ui.label("Coefficient bank unavailable");
        return;
    }
    let (response, painter) = ui.allocate_painter(
        Vec2::new(ui.available_width().max(160.), 210.),
        Sense::hover(),
    );
    let plot = response.rect.shrink2(Vec2::new(38., 18.));
    let x = |hz: f64| plot.left() + ((hz / 20.).log10() / 1000_f64.log10()) as f32 * plot.width();
    let y = |db: f64| plot.bottom() - ((db.clamp(-144., 0.) + 144.) / 144.) as f32 * plot.height();
    let grid = Stroke::new(1., ui.visuals().widgets.noninteractive.bg_stroke.color);
    let label = ui.visuals().weak_text_color();
    for db in [0, -24, -48, -72, -96, -120, -144] {
        let py = y(f64::from(db));
        painter.line_segment(
            [Pos2::new(plot.left(), py), Pos2::new(plot.right(), py)],
            grid,
        );
        painter.text(
            Pos2::new(plot.left() - 5., py),
            Align2::RIGHT_CENTER,
            format!("{db}"),
            FontId::proportional(10.),
            label,
        );
    }
    for (hz, text) in [
        (20., "20 Hz"),
        (100., "100"),
        (1000., "1k"),
        (4000., "4k"),
        (10000., "10k"),
        (20000., "20k"),
    ] {
        let px = x(hz);
        painter.line_segment(
            [Pos2::new(px, plot.top()), Pos2::new(px, plot.bottom())],
            grid,
        );
        painter.text(
            Pos2::new(px, plot.bottom() + 5.),
            Align2::CENTER_TOP,
            text,
            FontId::proportional(10.),
            label,
        );
    }
    let points = (0..=160)
        .map(|index| {
            let hz = 20. * 1000_f64.powf(f64::from(index) / 160.);
            Pos2::new(
                x(hz),
                y(response_db(hz, *rate).expect("Supported graph domain")),
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(2., Color32::from_rgb(92, 180, 235)),
    ));
    if let Some(pointer) = response.hover_pos().filter(|pos| plot.contains(*pos)) {
        let hz = 20. * 1000_f64.powf(f64::from((pointer.x - plot.left()) / plot.width()));
        let db = response_db(hz, *rate).expect("Supported graph domain");
        painter.line_segment(
            [
                Pos2::new(pointer.x, plot.top()),
                Pos2::new(pointer.x, plot.bottom()),
            ],
            Stroke::new(1., label),
        );
        response.on_hover_text(format!("{hz:.1} Hz: {db:.2} dB detector-model magnitude"));
    }
    ui.weak("Analytic magnitude; fixed-point noise floor and physical acoustic response are not modeled.");
    ui.label("Fixed 4 kHz, third-order Butterworth high-pass detector. Its gain scales unfiltered audio: wideband de-essing. No frequency/bandwidth control is exposed.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coefficient_graph_matches_original_serialized_detector_pcm() {
        // Actual P:D24..D36 execution, integer-cycle 0.5-amplitude input windows;
        // tools/dynamics_kernel.rs reads the original boot banks and logs these vectors.
        let frequencies = [500., 1000., 2000., 4000., 8000., 12000., 18000.];
        for (rate, measured) in [
            (
                48_000,
                [
                    -54.781065, -36.692253, -18.578069, -3.010303, -0.043183, -0.001607, -0.000009,
                ],
            ),
            (
                44_100,
                [
                    -54.893280, -36.799357, -18.663082, -3.010299, -0.039434, -0.001190, -0.000003,
                ],
            ),
        ] {
            for (hz, native_db) in frequencies.into_iter().zip(measured) {
                assert!((response_db(hz, rate).unwrap() - native_db).abs() < 0.001);
            }
        }
    }

    #[test]
    fn recovered_detector_model_has_expected_cutoff_and_supported_domains() {
        for rate in [44_100, 48_000] {
            assert!((response_db(4000., rate).unwrap() + 3.01029995664).abs() < 0.001);
            assert!(response_db(1000., rate).unwrap() < -36.);
            assert!(response_db(18000., rate).unwrap().abs() < 0.001);
            assert!(response_db(0., rate).is_none());
            assert!(response_db(f64::from(rate) / 2., rate).is_none());
        }
        assert!(response_db(f64::NAN, 48_000).is_none());
        assert!(response_db(4000., 96_000).is_none());
    }
}
