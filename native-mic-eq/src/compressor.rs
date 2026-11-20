use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};

const RATIOS: [f64; 15] = [
    1., 1.1, 1.2, 1.4, 1.6, 1.8, 2., 2.5, 3.2, 4., 5.6, 8., 16., 32., 64.,
];

struct ManualModel {
    threshold: i32,
    ratio: f64,
    makeup: i32,
}

impl ManualModel {
    fn from_raw(values: &[i32; 11]) -> Option<Self> {
        let ratio = usize::try_from(values[6])
            .ok()
            .and_then(|raw| RATIOS.get(raw))?;
        ((-50..=0).contains(&values[7]) && (0..=24).contains(&values[10])).then_some(Self {
            threshold: values[7],
            ratio: if values[7] == 0 { 1. } else { *ratio },
            makeup: values[10],
        })
    }

    fn output_db(&self, input_db: f64) -> Option<f64> {
        if !input_db.is_finite() || !(-120.0..=0.).contains(&input_db) {
            return None;
        }
        // Original P:0343DD coefficients and P:00441D manual-mode kernel.
        let scale = 0x550a96 as f64 / 8388608. / 256.;
        let manual_offset = 0x780a3e as f64 / 8388608. * 32.;
        let input_log = input_db / (20. * 2_f64.log10()) / 64.;
        let threshold = f64::from(self.threshold - 6) * scale;
        let slope = 1. / self.ratio - 1.;
        let makeup = (f64::from(self.makeup) - manual_offset) * scale;
        let gain_log = ((input_log - threshold).max(0.) * slope + makeup).min(0.);
        Some(input_db + 20. * 32_f64.log10() + 64. * gain_log * 20. * 2_f64.log10())
    }
}

pub fn show(ui: &mut egui::Ui, values: &[i32; 11]) {
    ui.label("Recovered manual-mode compressor model — not a microphone measurement.");
    ui.weak("Uses the last DSP threshold, ratio and makeup snapshot. The private automatic-makeup flag is not read back or assumed.");
    let Some(model) = ManualModel::from_raw(values) else {
        ui.label("Compressor model unavailable: invalid raw settings.");
        return;
    };
    ui.label(format!(
        "Model ratio {:.2}:1; threshold {} dB; manual makeup {} dB. Internal knee: {} dB.",
        model.ratio,
        model.threshold,
        model.makeup,
        model.threshold - 6
    ));
    if model.threshold == 0 {
        ui.weak("The original threshold-0 special case forces a 1:1 ratio.");
    }
    let (response, painter) = ui.allocate_painter(
        Vec2::new(ui.available_width().max(160.), 220.),
        Sense::hover(),
    );
    let plot = response.rect.shrink2(Vec2::new(38., 25.));
    let x = |db: f64| plot.left() + ((db + 60.) / 60.) as f32 * plot.width();
    let y = |db: f64| plot.bottom() - ((db.clamp(-60., 30.) + 60.) / 90.) as f32 * plot.height();
    let grid = Stroke::new(1., ui.visuals().widgets.noninteractive.bg_stroke.color);
    let label = ui.visuals().weak_text_color();
    for db in [-60, -40, -20, 0, 20, 30] {
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
    for db in [-60, -40, -20, 0] {
        let px = x(f64::from(db));
        painter.line_segment(
            [Pos2::new(px, plot.top()), Pos2::new(px, plot.bottom())],
            grid,
        );
        painter.text(
            Pos2::new(px, plot.bottom() + 5.),
            Align2::CENTER_TOP,
            format!("{db}"),
            FontId::proportional(10.),
            label,
        );
    }
    painter.line_segment(
        [Pos2::new(x(-60.), y(-60.)), Pos2::new(x(0.), y(0.))],
        Stroke::new(1., label),
    );
    let points = (0..=160)
        .map(|i| {
            let input = -60. + f64::from(i) * 60. / 160.;
            Pos2::new(
                x(input),
                y(model.output_db(input).expect("Supported graph domain")),
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(2., Color32::from_rgb(92, 180, 235)),
    ));
    if let Some(pointer) = response.hover_pos().filter(|pos| plot.contains(*pos)) {
        let input = -60. + f64::from((pointer.x - plot.left()) / plot.width()) * 60.;
        let output = model.output_db(input).expect("Supported graph domain");
        painter.line_segment(
            [
                Pos2::new(pointer.x, plot.top()),
                Pos2::new(pointer.x, plot.bottom()),
            ],
            Stroke::new(1., label),
        );
        response.on_hover_text(format!(
            "Internal input {input:.2} dB → model output {output:.2} dB\nStatic gain {:+.2} dB",
            output - input
        ));
    }
    ui.weak("Axes: internal normalized input/output dB. Gray: unity reference. Blue: manual-mode coefficient model.");
    ui.weak("Fixed-point ramp quantization, saturation, attack/release, automatic mode and physical acoustic response are not modeled. This graph does not change hardware.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(ratio: i32, threshold: i32, makeup: i32) -> ManualModel {
        let mut values = [0; 11];
        values[6] = ratio;
        values[7] = threshold;
        values[10] = makeup;
        ManualModel::from_raw(&values).unwrap()
    }

    #[test]
    fn model_matches_original_manual_kernel_vectors() {
        // Native serialized P:00441D, original generated LUTs and manual flag 0.
        let curve = model(9, -20, 0);
        for (input, native_output) in [
            (-30., -29.9067),
            (-24., -25.4060),
            (-20., -24.4065),
            (-10., -21.9067),
        ] {
            assert!((curve.output_db(input).unwrap() - native_output).abs() < 0.01);
        }
        assert!(
            (curve.output_db(-10.).unwrap() - curve.output_db(-20.).unwrap() - 2.5).abs() < 0.001
        );
        assert!(
            (model(0, -20, 12).output_db(-40.).unwrap()
                - model(0, -20, 0).output_db(-40.).unwrap()
                - 12.)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn threshold_zero_special_case_and_invalid_domains_are_explicit() {
        let high_ratio = model(14, 0, 0);
        for input in [-60., -30., 0.] {
            assert_eq!(high_ratio.output_db(input), model(0, 0, 0).output_db(input));
        }
        for (index, invalid) in [(6, -1), (6, 15), (7, -51), (7, 1), (10, -1), (10, 25)] {
            let mut values = [0; 11];
            values[index] = invalid;
            assert!(ManualModel::from_raw(&values).is_none());
        }
        assert!(high_ratio.output_db(f64::NAN).is_none());
        assert!(high_ratio.output_db(1.).is_none());
    }
}
