use super::*;
use eframe::egui::{
    epaint::{Mesh, Vertex},
    Color32, Painter, Response, Shape,
};

pub(super) fn gradient(p: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    for (pos, color) in [
        (rect.left_top(), top),
        (rect.right_top(), top),
        (rect.right_bottom(), bottom),
        (rect.left_bottom(), bottom),
    ] {
        mesh.vertices.push(Vertex {
            pos,
            uv: egui::epaint::WHITE_UV,
            color,
        });
    }
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    p.add(Shape::mesh(mesh));
}

pub(super) fn plate(p: &Painter, rect: Rect, fill: Color32, pressed: bool) {
    gradient(
        p,
        rect,
        fill.gamma_multiply(if pressed { 0.8 } else { 1.12 }),
        fill.gamma_multiply(if pressed { 1.0 } else { 0.86 }),
    );
    p.rect_stroke(rect, 2, Stroke::new(1.0_f32, SHADOW), StrokeKind::Inside);
    p.line_segment(
        [
            rect.left_top() + vec2(1.0, 1.0),
            rect.right_top() + vec2(-1.0, 1.0),
        ],
        Stroke::new(1.0_f32, if pressed { SHADOW } else { HILITE }),
    );
    p.line_segment(
        [
            rect.left_bottom() + vec2(1.0, -1.0),
            rect.right_bottom() + vec2(-1.0, -1.0),
        ],
        Stroke::new(1.0_f32, SHADOW),
    );
}

pub(super) fn button(ui: &mut egui::Ui, label: &str, width: f32, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 25.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    plate(
        ui.painter(),
        rect,
        if active {
            ACCENT
        } else if response.hovered() {
            CONTROL_HOVER
        } else {
            CONTROL
        },
        response.is_pointer_button_down_on(),
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.5),
        if active { Color32::BLACK } else { TEXT },
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            2,
            Stroke::new(1.0_f32, ACCENT),
            StrokeKind::Inside,
        );
    }
    response
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Play,
    Pause,
    Stop,
    Rewind,
    Folder,
    Wave,
    Loop,
}

pub(super) fn icon(p: &Painter, center: egui::Pos2, kind: Icon, size: f32, color: Color32) {
    let s = size * 0.5;
    match kind {
        Icon::Play => {
            p.add(Shape::convex_polygon(
                vec![
                    center + vec2(-s * 0.6, -s),
                    center + vec2(s, 0.0),
                    center + vec2(-s * 0.6, s),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for x in [-s * 0.6, s * 0.3] {
                p.rect_filled(
                    Rect::from_min_size(center + vec2(x, -s), vec2(s * 0.45, size)),
                    0,
                    color,
                );
            }
        }
        Icon::Stop => {
            p.rect_filled(
                Rect::from_center_size(center, vec2(size * 0.8, size * 0.8)),
                0,
                color,
            );
        }
        Icon::Rewind => {
            for x in [-s * 0.6, s * 0.6] {
                p.add(Shape::convex_polygon(
                    vec![
                        center + vec2(x + s * 0.5, -s),
                        center + vec2(x - s * 0.6, 0.0),
                        center + vec2(x + s * 0.5, s),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
        }
        Icon::Folder => {
            p.rect_stroke(
                Rect::from_center_size(center + vec2(0.0, 1.0), vec2(size, size * 0.65)),
                1,
                Stroke::new(1.0_f32, color),
                StrokeKind::Inside,
            );
            p.line_segment(
                [
                    center + vec2(-s, -s * 0.6),
                    center + vec2(-s * 0.2, -s * 0.6),
                ],
                Stroke::new(2.0_f32, color),
            );
        }
        Icon::Wave => {
            for (i, h) in [0.25, 0.7, 1.0, 0.5, 0.8, 0.3].iter().enumerate() {
                let x = center.x - s + i as f32 / 5.0 * size;
                p.line_segment(
                    [pos2(x, center.y - s * h), pos2(x, center.y + s * h)],
                    Stroke::new(1.0_f32, color),
                );
            }
        }
        Icon::Loop => {
            let points = (0..28)
                .map(|i| {
                    let a = i as f32 / 27.0 * 5.2 - 2.3;
                    center + vec2(a.cos() * s, a.sin() * s * 0.75)
                })
                .collect();
            p.add(Shape::line(points, Stroke::new(1.5_f32, color)));
            p.line_segment(
                [
                    center + vec2(-s * 0.65, -s * 0.55),
                    center + vec2(-s, -s * 0.8),
                ],
                Stroke::new(2.0_f32, color),
            );
        }
    }
}

pub(super) fn icon_button(ui: &mut egui::Ui, kind: Icon, label: &str, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 29.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    plate(
        ui.painter(),
        rect,
        if active {
            if matches!(kind, Icon::Play | Icon::Pause) {
                METER_GREEN
            } else {
                ACCENT
            }
        } else if response.hovered() {
            CONTROL_HOVER
        } else {
            CONTROL
        },
        response.is_pointer_button_down_on(),
    );
    icon(
        ui.painter(),
        rect.center(),
        kind,
        12.0,
        if active { Color32::BLACK } else { TEXT },
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            2,
            Stroke::new(1.0_f32, ACCENT),
            StrokeKind::Inside,
        );
    }
    response.on_hover_text(label)
}

pub(super) fn meter(p: &Painter, rect: Rect, peak: f32, horizontal: bool) {
    p.rect_filled(rect, 1, WELL);
    let fill = if peak > 0.0001 {
        ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    for i in 0..28 {
        let t = i as f32 / 28.0;
        let color = if t > 0.96 {
            METER_RED
        } else if t > 0.8 {
            METER_YELLOW
        } else {
            METER_GREEN
        };
        let color = if t < fill {
            color
        } else {
            color.gamma_multiply(0.16)
        };
        let segment = if horizontal {
            Rect::from_min_size(
                pos2(rect.left() + rect.width() * t + 1.0, rect.top() + 1.0),
                vec2((rect.width() / 28.0 - 1.0).max(1.0), rect.height() - 2.0),
            )
        } else {
            Rect::from_min_size(
                pos2(
                    rect.left() + 1.0,
                    rect.bottom() - rect.height() * (i + 1) as f32 / 28.0,
                ),
                vec2(rect.width() - 2.0, (rect.height() / 28.0 - 1.0).max(1.0)),
            )
        };
        p.rect_filled(segment, 0, color);
    }
}

pub(super) fn knob(
    ui: &mut egui::Ui,
    value: &mut f32,
    label: &str,
    display: &str,
    color: Color32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(94.0, 119.0), Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Slider,
            true,
            format!("{label}: {display}"),
        )
    });
    let old = *value;
    if response.dragged() {
        *value = (*value - ui.input(|i| i.pointer.delta().y) * 0.006).clamp(0.0, 1.0);
    }
    if response.has_focus() {
        ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowRight) {
                *value = (*value + 0.01).min(1.0);
            }
            if i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowLeft) {
                *value = (*value - 0.01).max(0.0);
            }
        });
    }
    let p = ui.painter();
    let center = rect.min + vec2(47.0, 48.0);
    for i in 0..=12 {
        let angle = 2.3561945 + i as f32 / 12.0 * 4.712389;
        let direction = vec2(angle.cos(), angle.sin());
        p.line_segment(
            [center + direction * 33.0, center + direction * 36.0],
            Stroke::new(
                1.0_f32,
                if i as f32 / 12.0 <= *value {
                    color
                } else {
                    EDGE
                },
            ),
        );
    }
    p.circle_filled(center + vec2(0.0, 3.0), 29.0, Color32::BLACK);
    metal_disc(p, center, 28.0, 166, 62);
    p.circle_filled(center + vec2(0.0, 0.5), 26.0, WELL);
    metal_disc(p, center, 24.0, 130, 43);
    p.circle_stroke(center, 23.5, Stroke::new(1.0_f32, Color32::from_gray(99)));
    let angle = 2.3561945 + *value * 4.712389;
    p.line_segment(
        [
            center + vec2(angle.cos(), angle.sin()) * 13.0,
            center + vec2(angle.cos(), angle.sin()) * 21.0,
        ],
        Stroke::new(2.0_f32, TEXT),
    );
    p.text(
        pos2(center.x, rect.top() + 92.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        MUTED,
    );
    p.text(
        pos2(center.x, rect.top() + 109.0),
        Align2::CENTER_CENTER,
        display,
        FontId::monospace(12.0),
        TEXT,
    );
    if response.has_focus() {
        p.circle_stroke(center, 30.0, Stroke::new(1.0_f32, color));
    }
    old != *value
}

fn metal_disc(p: &Painter, center: egui::Pos2, radius: f32, light: u8, dark: u8) {
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: center - vec2(radius * 0.15, radius * 0.15),
        uv: egui::epaint::WHITE_UV,
        color: Color32::from_gray(light),
    });
    for i in 0..=48 {
        let angle = i as f32 / 48.0 * std::f32::consts::TAU;
        let direction = vec2(angle.cos(), angle.sin());
        let illumination = ((-direction.x * 0.6 - direction.y * 0.8 + 1.0) * 0.5).clamp(0.0, 1.0);
        let grey = dark as f32 + (light - dark) as f32 * illumination;
        mesh.vertices.push(Vertex {
            pos: center + direction * radius,
            uv: egui::epaint::WHITE_UV,
            color: Color32::from_gray(grey as u8),
        });
        if i > 0 {
            mesh.indices.extend_from_slice(&[0, i, i + 1]);
        }
    }
    p.add(Shape::mesh(mesh));
}

pub(super) fn wood(p: &Painter, rect: Rect) {
    gradient(
        p,
        rect,
        Color32::from_rgb(108, 70, 43),
        Color32::from_rgb(61, 39, 27),
    );
    for line in 0..6 {
        let points = (0..24)
            .map(|i| {
                let y = rect.top() + i as f32 / 23.0 * rect.height();
                let x =
                    rect.left() + line as f32 * 1.3 + (i as f32 * 0.7 + line as f32).sin() * 0.6;
                pos2(x, y)
            })
            .collect();
        p.add(Shape::line(
            points,
            Stroke::new(0.5_f32, Color32::from_black_alpha(66)),
        ));
    }
    p.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0_f32, SHADOW),
    );
    p.line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0_f32, Color32::from_rgb(141, 96, 61)),
    );
    for y in [rect.top() + 9.0, rect.bottom() - 9.0] {
        let center = pos2(rect.center().x, y);
        p.circle_filled(center, 2.1, SHADOW);
        p.circle_stroke(center, 2.1, Stroke::new(0.5_f32, HILITE));
        p.line_segment(
            [center + vec2(-1.0, 0.0), center + vec2(1.0, 0.0)],
            Stroke::new(0.6_f32, MUTED),
        );
    }
}
