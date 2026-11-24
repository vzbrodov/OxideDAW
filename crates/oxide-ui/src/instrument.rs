use super::skin::Skin;
use super::widgets as w;
use super::*;
use egui::Color32;
use oxide_core::project::{NamedPattern, MAX_NAMED_PATTERNS, MAX_PATTERN_NAME_BYTES};

pub(super) struct NewPattern {
    name: String,
    focus_name: bool,
}

const INK: Color32 = Color32::from_rgb(43, 45, 44);
const FACE: Color32 = Color32::from_rgb(182, 181, 172);
const SILK: Color32 = Color32::from_rgb(97, 99, 93);

fn text(
    p: &egui::Painter,
    pos: egui::Pos2,
    align: Align2,
    value: impl ToString,
    size: f32,
    color: Color32,
) {
    p.text(pos, align, value, FontId::proportional(size), color);
}
fn mono(p: &egui::Painter, pos: egui::Pos2, value: impl ToString, size: f32, color: Color32) {
    p.text(
        pos,
        Align2::LEFT_CENTER,
        value,
        FontId::monospace(size),
        color,
    );
}
fn screw(p: &egui::Painter, center: egui::Pos2) {
    p.circle_filled(center + vec2(0.0, 1.0), 3.4, SHADOW);
    p.circle_filled(center, 2.8, Color32::from_gray(104));
    p.circle_stroke(center, 2.7, Stroke::new(0.6_f32, Color32::from_gray(198)));
    p.line_segment(
        [center - vec2(1.6, -0.6), center + vec2(1.6, -0.6)],
        Stroke::new(1.0_f32, SHADOW),
    );
}
fn faceplate(ui: &mut egui::Ui, height: f32, light: bool) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    p.rect_filled(
        rect.translate(vec2(0.0, 3.0)),
        5,
        Color32::from_black_alpha(100),
    );
    w::gradient(
        p,
        rect,
        if light {
            Color32::from_rgb(201, 200, 192)
        } else {
            Color32::from_rgb(68, 72, 73)
        },
        if light {
            FACE
        } else {
            Color32::from_rgb(45, 49, 51)
        },
    );
    p.rect_stroke(
        rect,
        3,
        Stroke::new(1.0_f32, if light { Color32::from_gray(219) } else { EDGE }),
        StrokeKind::Inside,
    );
    // Fine horizontal brushing stays below the controls and printed legends.
    for y in (2..height as usize).step_by(3) {
        p.line_segment(
            [
                rect.min + vec2(1.0, y as f32),
                pos2(rect.right() - 1.0, rect.top() + y as f32),
            ],
            Stroke::new(
                0.35_f32,
                Color32::from_black_alpha(if light { 7 } else { 10 }),
            ),
        );
    }
    for x in [rect.left() + 9.0, rect.right() - 9.0] {
        for y in [rect.top() + 10.0, rect.bottom() - 10.0] {
            screw(p, pos2(x, y));
        }
    }
    rect
}

fn rotary(
    ui: &mut egui::Ui,
    skin: &Skin,
    at: egui::Pos2,
    id: &str,
    value: &mut f32,
    name: &str,
    display: &str,
) {
    let bounds = Rect::from_min_size(at, vec2(106.0, 140.0));
    let response = ui.interact(bounds, ui.id().with(id), Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, format!("{name}: {display}"))
    });
    if response.dragged() {
        *value = (*value - ui.input(|i| i.pointer.delta().y) * 0.006).clamp(0.0, 1.0);
    }
    if response.has_focus() {
        ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowRight) {
                *value = (*value + 0.02).min(1.0);
            }
            if i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowLeft) {
                *value = (*value - 0.02).max(0.0);
            }
        });
    }
    let p = ui.painter();
    let center = at + vec2(53.0, 54.0);
    for i in 0..=10 {
        let a = std::f32::consts::PI * 0.75 + i as f32 / 10.0 * std::f32::consts::PI * 1.5;
        let d = vec2(a.cos(), a.sin());
        p.line_segment(
            [
                center + d * 41.0,
                center + d * if i % 5 == 0 { 46.0 } else { 44.0 },
            ],
            Stroke::new(1.0_f32, SILK),
        );
    }
    Skin::paint(
        p,
        &skin.rotary,
        Rect::from_center_size(center, vec2(100.0, 100.0)),
    );
    let a = std::f32::consts::PI * 0.75 + *value * std::f32::consts::PI * 1.5;
    let d = vec2(a.cos(), a.sin());
    p.line_segment(
        [center + d * 17.0, center + d * 28.0],
        Stroke::new(2.5_f32, Color32::from_gray(24)),
    );
    text(
        p,
        at + vec2(53.0, 112.0),
        Align2::CENTER_CENTER,
        name,
        12.0,
        INK,
    );
    text(
        p,
        at + vec2(53.0, 130.0),
        Align2::CENTER_CENTER,
        display,
        12.0,
        INK,
    );
    if response.has_focus() {
        p.rect_stroke(bounds, 3, Stroke::new(1.5_f32, INK), StrokeKind::Inside);
    }
    response.on_hover_text("Drag vertically; use arrow keys when focused");
}

impl Workspace {
    pub(super) fn named_pattern(&self) -> Option<&NamedPattern> {
        let id = self.selected_pattern?;
        self.session
            .editor
            .project()
            .patterns
            .iter()
            .find(|pattern| pattern.id == id)
    }

    pub(super) fn rack_pattern(&self) -> &Pattern {
        self.named_pattern()
            .map_or(&self.session.editor.project().pattern, |pattern| {
                &pattern.pattern
            })
    }

    pub(super) fn transport_pattern(&self) -> &Pattern {
        if self.arrangement_mode {
            &self.session.editor.project().pattern
        } else {
            self.rack_pattern()
        }
    }

    pub(super) fn select_pattern(&mut self, pattern_id: Option<u64>) {
        if self.selected_pattern == pattern_id {
            return;
        }
        self.selected_pattern = pattern_id;
        self.velocity = self
            .rack_pattern()
            .get(self.selected, self.selected_step)
            .unwrap_or(0);
        if !self.arrangement_mode {
            self.playback = None;
            self.playing = false;
        }
    }

    fn step_command(&self, track: usize, step: usize, velocity: u8) -> EditCommand {
        let track_id = self.session.editor.project().tracks[track].id;
        match self.selected_pattern {
            Some(pattern_id) => EditCommand::SetPatternStep {
                pattern_id,
                track_id,
                step,
                velocity,
            },
            None => EditCommand::SetStep {
                track_id,
                step,
                velocity,
            },
        }
    }

    fn create_pattern(&mut self, name: String) -> bool {
        let patterns = &self.session.editor.project().patterns;
        // A free ID exists within this range even when projects use arbitrary IDs.
        let Some(pattern_id) = (1..=patterns.len() as u64 + 1)
            .find(|id| patterns.iter().all(|pattern| pattern.id != *id))
        else {
            self.notice("Could not assign an ID to the new pattern.", true);
            return false;
        };
        if self.edit(&[EditCommand::AddPattern {
            pattern_id,
            name,
            pattern: Pattern::default(),
        }]) {
            self.select_pattern(Some(pattern_id));
            return true;
        }
        false
    }

    fn pattern_controls(&mut self, ui: &mut egui::Ui) {
        let mut selected = self.selected_pattern;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("rack-pattern")
                .selected_text(
                    self.named_pattern()
                        .map_or("Main pattern", |pattern| &pattern.name),
                )
                .width(156.0)
                .truncate()
                .show_ui(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            ui.selectable_value(&mut selected, None, "Main pattern");
                            for pattern in &self.session.editor.project().patterns {
                                ui.selectable_value(&mut selected, Some(pattern.id), &pattern.name)
                                    .on_hover_text(format!("Pattern ID {}", pattern.id));
                            }
                        });
                })
                .response
                .on_hover_text("Pattern to edit");
            let can_create = self.session.editor.project().patterns.len() < MAX_NAMED_PATTERNS;
            if ui
                .add_enabled(can_create, egui::Button::new("New pattern…"))
                .on_disabled_hover_text("This project has reached the named pattern limit.")
                .clicked()
            {
                self.new_pattern = Some(NewPattern {
                    name: String::new(),
                    focus_name: true,
                });
            }
        });
        self.select_pattern(selected);
    }

    pub(super) fn new_pattern_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.new_pattern.as_mut() else {
            return;
        };
        let mut cancel = false;
        let mut create = false;
        let dialog = egui::Modal::new(egui::Id::new("new-rack-pattern")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("New pattern");
            let label = ui.label("Pattern name");
            let input = ui
                .add(egui::TextEdit::singleline(&mut draft.name).desired_width(f32::INFINITY))
                .labelled_by(label.id);
            if std::mem::take(&mut draft.focus_name) {
                input.request_focus();
            }
            let trimmed = draft.name.trim();
            let too_long = trimmed.len() > MAX_PATTERN_NAME_BYTES;
            let valid = !trimmed.is_empty() && !too_long;
            if too_long {
                ui.label(
                    RichText::new(format!(
                        "Use a name up to {MAX_PATTERN_NAME_BYTES} UTF-8 bytes."
                    ))
                    .color(COPPER),
                );
            }
            ui.horizontal(|ui| {
                cancel = ui.button("Cancel").clicked();
                create = ui.add_enabled(valid, egui::Button::new("Create")).clicked();
            });
            create |= valid
                && input.lost_focus()
                && ui.input(|input| input.key_pressed(egui::Key::Enter));
        });
        if create {
            let name = draft.name.trim().to_owned();
            if self.create_pattern(name) {
                self.new_pattern = None;
            }
        } else if cancel || dialog.should_close() {
            self.new_pattern = None;
        }
    }

    pub(super) fn rack(&mut self, ui: &mut egui::Ui) {
        let r = faceplate(ui, 400.0, false);
        let p = ui.painter();
        text(
            p,
            r.min + vec2(26.0, 27.0),
            Align2::LEFT_CENTER,
            "OXIDE / RHYTHM",
            19.0,
            TEXT,
        );
        text(
            p,
            r.min + vec2(26.0, 49.0),
            Align2::LEFT_CENTER,
            "Four-channel sample sequencer",
            12.0,
            MUTED,
        );
        p.line_segment(
            [r.min + vec2(24.0, 66.0), r.right_top() + vec2(-24.0, 66.0)],
            Stroke::new(1.0_f32, SHADOW),
        );
        let controls = Rect::from_min_max(
            r.right_top() + vec2(-280.0, 17.0),
            r.right_top() + vec2(-26.0, 54.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(controls), |ui| {
            self.pattern_controls(ui);
        });
        let left = r.left() + 202.0;
        let right = r.right() - 26.0;
        let cell = (right - left) / 16.0;
        let play = self.rack_play_step();
        for beat in 0..4 {
            let x = left + beat as f32 * 4.0 * cell;
            text(
                ui.painter(),
                pos2(x + 4.0, r.top() + 84.0),
                Align2::LEFT_CENTER,
                format!("BEAT {}", beat + 1),
                10.5,
                MUTED,
            );
            ui.painter().line_segment(
                [
                    pos2(x + 54.0, r.top() + 84.0),
                    pos2(x + cell * 4.0 - 8.0, r.top() + 84.0),
                ],
                Stroke::new(1.0_f32, EDGE),
            );
        }
        // This ruler seeks within the actual one-bar pattern.
        for step in 0..STEPS {
            let tick = Rect::from_min_size(
                pos2(left + step as f32 * cell, r.top() + 91.0),
                vec2(cell, 10.0),
            );
            ui.painter().line_segment(
                [tick.left_bottom(), tick.right_bottom() - vec2(3.0, 0.0)],
                Stroke::new(2.0_f32, if play == Some(step) { ACCENT } else { SHADOW }),
            );
            if !self.arrangement_mode
                && ui
                    .interact(tick, ui.id().with(("seek-step", step)), Sense::click())
                    .on_hover_text(format!("Seek to step {}", step + 1))
                    .clicked()
            {
                self.send(PlaybackCommand::SeekStep(step));
            }
        }
        for track in 0..TRACKS {
            let y = r.top() + 103.0 + track as f32 * 49.0;
            let color = TRACK[track];
            let name = Rect::from_min_size(pos2(r.left() + 24.0, y), vec2(171.0, 44.0));
            if self.selected == track {
                ui.painter()
                    .rect_filled(name, 3, Color32::from_black_alpha(72));
                ui.painter()
                    .rect_filled(Rect::from_min_size(name.min, vec2(3.0, 44.0)), 1, color);
            }
            mono(
                ui.painter(),
                name.min + vec2(11.0, 13.0),
                format!("{:02}", track + 1),
                11.0,
                color,
            );
            text(
                ui.painter(),
                name.min + vec2(36.0, 13.0),
                Align2::LEFT_CENTER,
                &self.session.editor.project().tracks[track].name,
                13.0,
                TEXT,
            );
            text(
                ui.painter(),
                name.min + vec2(36.0, 31.0),
                Align2::LEFT_CENTER,
                if self.media_errors[track].is_some() {
                    "Sample unavailable"
                } else if self.samples[track].is_some() {
                    "One-shot sample"
                } else {
                    "Assign a sample"
                },
                11.0,
                MUTED,
            );
            let response = ui.interact(name, ui.id().with(("select", track)), Sense::click());
            if response.clicked() {
                self.select_channel(track, None);
            }
            if response.double_clicked() {
                self.audition(track);
            }
            response.on_hover_text("Select channel · double-click to audition");
            for step in 0..STEPS {
                let pad = Rect::from_min_size(
                    pos2(left + step as f32 * cell, y + 1.0),
                    vec2(cell - 3.0, 38.0),
                );
                let velocity = self.rack_pattern().get(track, step).unwrap_or(0);
                let resp = ui.interact(pad, ui.id().with(("step", track, step)), Sense::click());
                resp.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!(
                            "{} step {}, velocity {}",
                            self.session.editor.project().tracks[track].name,
                            step + 1,
                            velocity
                        ),
                    )
                });
                Skin::paint(
                    ui.painter(),
                    if velocity > 0 || resp.is_pointer_button_down_on() {
                        &self.skin.pad_down
                    } else {
                        &self.skin.pad_up
                    },
                    pad.expand2(vec2(1.5, 3.0)),
                );
                let led = Rect::from_center_size(
                    pad.center() + vec2(0.0, -8.0),
                    vec2((cell - 17.0).max(7.0), 3.0),
                );
                ui.painter().rect_filled(
                    led,
                    1,
                    if velocity > 0 {
                        color
                    } else {
                        Color32::from_rgb(24, 27, 28)
                    },
                );
                text(
                    ui.painter(),
                    pad.center() + vec2(0.0, 5.0),
                    Align2::CENTER_CENTER,
                    format!("{:02}", step + 1),
                    10.0,
                    if velocity > 0 {
                        TEXT
                    } else {
                        Color32::from_gray(144)
                    },
                );
                if self.selected == track && self.selected_step == step || resp.has_focus() {
                    ui.painter().rect_stroke(
                        pad.shrink(2.0),
                        3,
                        Stroke::new(1.0_f32, color),
                        StrokeKind::Inside,
                    );
                }
                if play == Some(step) {
                    ui.painter()
                        .circle_filled(pos2(pad.center().x, y + 43.0), 2.0, METER_GREEN);
                }
                if resp.clicked() {
                    self.select_channel(track, Some(step));
                    self.edit(&[self.step_command(
                        track,
                        step,
                        if velocity == 0 { 100 } else { 0 },
                    )]);
                } else if resp.secondary_clicked() {
                    self.select_channel(track, Some(step));
                }
                resp.on_hover_text(format!(
                    "Step {} · velocity {}\nClick toggles; right-click selects",
                    step + 1,
                    velocity
                ));
            }
        }
        let y = r.top() + 312.0;
        text(
            ui.painter(),
            pos2(r.left() + 27.0, y + 8.0),
            Align2::LEFT_CENTER,
            "STEP VELOCITY",
            11.0,
            MUTED,
        );
        text(
            ui.painter(),
            pos2(r.left() + 27.0, y + 28.0),
            Align2::LEFT_CENTER,
            format!(
                "{} / {:02}",
                self.session.editor.project().tracks[self.selected].name,
                self.selected_step + 1
            ),
            12.0,
            TRACK[self.selected],
        );
        for (offset, value) in [(0.0, "127"), (22.0, "64"), (44.0, "0")] {
            mono(
                ui.painter(),
                pos2(left - 30.0, y + offset),
                value,
                9.0,
                MUTED,
            );
        }
        for step in 0..STEPS {
            let lane = Rect::from_min_size(
                pos2(left + step as f32 * cell + 5.0, y),
                vec2(cell - 13.0, 45.0),
            );
            let v = self.rack_pattern().get(self.selected, step).unwrap_or(0);
            ui.painter().rect_filled(lane, 1, WELL);
            ui.painter().rect_filled(
                Rect::from_min_max(
                    pos2(
                        lane.left(),
                        lane.bottom() - v as f32 / 127.0 * lane.height(),
                    ),
                    lane.right_bottom(),
                ),
                1,
                TRACK[self.selected].gamma_multiply(0.8),
            );
            if step == self.selected_step {
                ui.painter()
                    .rect_stroke(lane, 1, Stroke::new(1.0_f32, TEXT), StrokeKind::Inside);
            }
            let response = ui.interact(
                lane,
                ui.id().with(("velocity", step)),
                Sense::click_and_drag(),
            );
            if response.clicked() || response.drag_started() {
                self.select_channel(self.selected, Some(step));
            }
            // Commit once on release, keeping a drag one undoable edit.
            if response.is_pointer_button_down_on() || response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    self.velocity = ((1.0 - (pos.y - lane.top()) / lane.height()).clamp(0.0, 1.0)
                        * 127.0)
                        .round() as u8;
                }
                let h = self.velocity as f32 / 127.0 * lane.height();
                ui.painter().rect_filled(
                    Rect::from_min_max(pos2(lane.left(), lane.bottom() - h), lane.right_bottom()),
                    1,
                    TEXT,
                );
            }
            let keyboard_delta = if response.has_focus() {
                ui.input(|i| {
                    if i.key_pressed(egui::Key::ArrowUp) {
                        1
                    } else if i.key_pressed(egui::Key::ArrowDown) {
                        -1
                    } else {
                        0
                    }
                })
            } else {
                0
            };
            if keyboard_delta != 0 {
                self.select_channel(self.selected, Some(step));
                self.velocity = (i16::from(self.velocity) + keyboard_delta).clamp(0, 127) as u8;
            }
            if response.drag_stopped() || response.clicked() || keyboard_delta != 0 {
                self.edit(&[self.step_command(self.selected, step, self.velocity)]);
            }
            response.on_hover_text(format!("Step {} velocity {} · drag to edit", step + 1, v));
        }
        let tools = Rect::from_min_size(
            pos2(r.left() + 26.0, r.bottom() - 34.0),
            vec2(r.width() - 52.0, 28.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(tools), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Selected step").size(11.0).color(MUTED));
                let resp = ui.add(egui::Slider::new(&mut self.velocity, 0..=127).text("Velocity"));
                if resp.drag_stopped() || resp.changed() && !ui.input(|i| i.pointer.primary_down())
                {
                    self.edit(&[self.step_command(
                        self.selected,
                        self.selected_step,
                        self.velocity,
                    )]);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::button(ui, "Clear channel", 92.0, false).clicked() {
                        let commands: Vec<_> = (0..STEPS)
                            .map(|step| self.step_command(self.selected, step, 0))
                            .collect();
                        self.edit(&commands);
                    }
                });
            });
        });
        ui.add_space(15.0);
    }

    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        let r = faceplate(ui, 310.0, true);
        let p = ui.painter();
        text(
            p,
            r.min + vec2(26.0, 29.0),
            Align2::LEFT_CENTER,
            "SAMPLER",
            19.0,
            INK,
        );
        text(
            p,
            r.min + vec2(142.0, 29.0),
            Align2::LEFT_CENTER,
            &self.session.editor.project().tracks[self.selected].name,
            14.0,
            INK,
        );
        p.text(
            r.right_top() + vec2(-27.0, 29.0),
            Align2::RIGHT_CENTER,
            "ONE-SHOT / 8 VOICES",
            FontId::monospace(11.0),
            SILK,
        );
        p.line_segment(
            [r.min + vec2(25.0, 52.0), r.right_top() + vec2(-25.0, 52.0)],
            Stroke::new(1.0_f32, Color32::from_gray(148)),
        );
        let wave = Rect::from_min_size(r.min + vec2(26.0, 67.0), vec2(r.width() - 302.0, 160.0));
        let p = ui.painter();
        p.rect_filled(wave.expand(4.0), 4, Color32::from_rgb(97, 100, 95));
        p.rect_filled(wave, 2, Color32::from_rgb(24, 34, 33));
        w::gradient(
            p,
            Rect::from_min_size(wave.min, vec2(wave.width(), 30.0)),
            Color32::from_rgb(42, 53, 50),
            Color32::from_rgb(31, 43, 40),
        );
        let filename = self
            .session
            .sample_path(self.selected)
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "No sample assigned".into());
        text(
            p,
            wave.min + vec2(12.0, 16.0),
            Align2::LEFT_CENTER,
            filename,
            12.0,
            Color32::from_rgb(193, 211, 183),
        );
        let plot = Rect::from_min_max(wave.min + vec2(13.0, 40.0), wave.max - vec2(13.0, 21.0));
        for i in 0..=8 {
            let x = plot.left() + plot.width() * i as f32 / 8.0;
            p.line_segment(
                [pos2(x, plot.top()), pos2(x, plot.bottom())],
                Stroke::new(0.5_f32, Color32::from_rgb(50, 64, 58)),
            );
        }
        for i in 0..=4 {
            let y = plot.top() + plot.height() * i as f32 / 4.0;
            p.line_segment(
                [pos2(plot.left(), y), pos2(plot.right(), y)],
                Stroke::new(0.5_f32, Color32::from_rgb(50, 64, 58)),
            );
        }
        let peaks = &self.peaks[self.selected];
        let n = ((peaks.len() as f32 / self.wave_zoom) as usize)
            .max(1)
            .min(peaks.len());
        if n > 0 {
            for (i, &peak) in peaks.iter().take(n).enumerate() {
                let x = plot.left() + i as f32 / n as f32 * plot.width();
                let h = peak.min(1.0) * plot.height() * 0.45;
                p.line_segment(
                    [pos2(x, plot.center().y - h), pos2(x, plot.center().y + h)],
                    Stroke::new(
                        (plot.width() / n as f32).max(1.0),
                        Color32::from_rgb(173, 201, 143),
                    ),
                );
            }
        } else {
            text(
                p,
                plot.center(),
                Align2::CENTER_CENTER,
                if self.media_errors[self.selected].is_some() {
                    "Sample unavailable · choose a replacement"
                } else {
                    "Choose a file in the library or drop audio here"
                },
                12.0,
                MUTED,
            );
        }
        if let Some(sample) = &self.samples[self.selected] {
            let duration =
                sample.samples.len() as f64 / sample.channels as f64 / sample.sample_rate as f64;
            mono(
                p,
                pos2(wave.left() + 12.0, wave.bottom() - 11.0),
                format!(
                    "{} Hz   {}   {:.3} s",
                    sample.sample_rate,
                    if sample.channels == 1 {
                        "MONO"
                    } else {
                        "STEREO"
                    },
                    duration
                ),
                10.5,
                Color32::from_rgb(171, 189, 162),
            );
            for i in 0..=4 {
                let x = plot.left() + plot.width() * i as f32 / 4.0;
                p.text(
                    pos2(x, plot.top() - 8.0),
                    if i == 4 {
                        Align2::RIGHT_CENTER
                    } else {
                        Align2::LEFT_CENTER
                    },
                    format!(
                        "{:.0} ms",
                        duration * 1000.0 / self.wave_zoom as f64 * i as f64 / 4.0
                    ),
                    FontId::monospace(9.0),
                    MUTED,
                );
            }
        }
        let mut zoom = (self.wave_zoom - 1.0) / 7.0;
        let zoom_display = format!("{:.1}×", self.wave_zoom);
        rotary(
            ui,
            &self.skin,
            r.right_top() + vec2(-261.0, 64.0),
            "zoom",
            &mut zoom,
            "Waveform zoom",
            &zoom_display,
        );
        self.wave_zoom = 1.0 + zoom * 7.0;
        let level = if self.preview_volume > 0.001 {
            format!("{:.1} dB", 20.0 * self.preview_volume.log10())
        } else {
            "−∞ dB".into()
        };
        rotary(
            ui,
            &self.skin,
            r.right_top() + vec2(-140.0, 64.0),
            "preview-level",
            &mut self.preview_volume,
            "Audition level",
            &level,
        );
        text(
            ui.painter(),
            r.right_top() + vec2(-145.0, 216.0),
            Align2::CENTER_CENTER,
            "Preview controls",
            11.0,
            SILK,
        );
        let controls = Rect::from_min_size(r.min + vec2(26.0, 244.0), vec2(r.width() - 52.0, 53.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(controls),|ui|{
            ui.horizontal(|ui|{
                if w::button(ui,"▶ Audition",96.0,self.preview.is_some()).on_hover_text("Play this sample through the master output; level applies on next audition").clicked(){self.audition(self.selected);}
                if w::button(ui,"Unassign",76.0,false).clicked(){self.edit(&[EditCommand::SetSample{track_id:self.session.editor.project().tracks[self.selected].id,sample:None}]);}
                ui.label(RichText::new("Preview stops pattern playback").size(11.0).color(INK));
            });
            ui.horizontal(|ui|{
                ui.label(RichText::new("Source").color(INK));
                let field=ui.add(egui::TextEdit::singleline(&mut self.sample_input).desired_width(controls.width()-111.0).hint_text("Path to WAV, FLAC or MP3"));
                if w::button(ui,"Load",49.0,false).clicked() || field.lost_focus()&&ui.input(|i|i.key_pressed(egui::Key::Enter)){self.assign(&PathBuf::from(&self.sample_input));}
            });
        });
    }
}

#[cfg(test)]
mod pattern_tests {
    use super::*;

    #[test]
    fn rack_edits_and_previews_the_selected_pattern() {
        let ctx = egui::Context::default();
        let mut workspace = Workspace::new(Session::open(None).unwrap(), &ctx).unwrap();
        let main = workspace.rack_pattern().clone();
        assert!(workspace.create_pattern("Verse".into()));
        let verse = workspace.selected_pattern.unwrap();
        assert_eq!(workspace.velocity, 0);
        assert!(workspace.edit(&[workspace.step_command(0, 0, 75)]));
        assert_eq!(workspace.transport_pattern().get(0, 0), Some(75));
        assert_eq!(workspace.session.editor.project().pattern, main);

        workspace.arrangement_mode = true;
        assert_eq!(workspace.transport_pattern(), &main);
        assert_eq!(workspace.rack_pattern().get(0, 0), Some(75));
        workspace.arrangement_mode = false;
        workspace.select_pattern(None);
        assert_eq!(workspace.velocity, main.get(0, 0).unwrap());
        workspace.select_pattern(Some(verse));
        assert_eq!(workspace.velocity, 75);

        assert!(workspace.session.editor.undo());
        workspace.changed();
        assert_eq!(workspace.velocity, 0);
        assert!(workspace.session.editor.undo());
        workspace.changed();
        assert_eq!(workspace.selected_pattern, None);
        assert_eq!(workspace.rack_pattern(), &main);
        assert!(workspace.session.editor.redo());
        workspace.changed();
        workspace.select_pattern(Some(verse));
        assert_eq!(workspace.rack_pattern().get(0, 0), Some(0));
    }
}
