//! The desktop arrangement editor uses the same project commands as the CLI.
use super::*;

const BARS_PER_PAGE: usize = 8;

enum BarEdit {
    Add(ArrangementBar),
    Move(usize),
    Replace(ArrangementBar),
    Remove,
}

impl Workspace {
    pub(super) fn sequence_view(&mut self, ui: &mut egui::Ui) {
        let was_arrangement = self.arrangement_mode;
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.selectable_value(&mut self.arrangement_mode, false, "Pattern");
            ui.selectable_value(&mut self.arrangement_mode, true, "Arrangement");
        });
        if was_arrangement != self.arrangement_mode {
            self.playback = None;
            self.playback_bars.clear();
            self.playing = false;
        }
        if self.arrangement_mode {
            self.arranger(ui);
        }
    }

    fn arranger(&mut self, ui: &mut egui::Ui) {
        let rack_pattern = self.rack_pattern().clone();
        let selected_pattern_name = self
            .named_pattern()
            .map_or("Main pattern", |pattern| &pattern.name)
            .to_owned();
        let project = self.session.editor.project();
        let selected_pattern_bar = self
            .selected_pattern
            .map_or(ArrangementBar::Main {}, |pattern_id| {
                ArrangementBar::Pattern { pattern_id }
            });
        let bars = &project.arrangement;
        let count = bars.len();
        self.selected_bar = self.selected_bar.min(count.saturating_sub(1));
        let selected = self.selected_bar;
        let current = self
            .playback
            .as_ref()
            .map(|playback| (playback.position_steps() as usize / STEPS) % count.max(1));
        let first = (selected / BARS_PER_PAGE) * BARS_PER_PAGE;
        let end = (first + BARS_PER_PAGE).min(count);
        let mut edit = None;
        let mut seek = None;

        let margin = 10;
        let content_width = ui.available_width() - f32::from(margin) * 2.0;
        egui::Frame::new()
            .fill(PANEL)
            .inner_margin(margin)
            .show(ui, |ui| {
                ui.set_min_width(content_width);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Arrangement").strong());
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{count} bars"))
                            .monospace()
                            .color(MUTED),
                    );
                    ui.separator();
                    if ui
                        .button("+ Pattern")
                        .on_hover_text(format!("Add {selected_pattern_name}"))
                        .clicked()
                    {
                        edit = Some(BarEdit::Add(selected_pattern_bar.clone()));
                    }
                    if ui.button("+ Rest").clicked() {
                        edit = Some(BarEdit::Add(ArrangementBar::Rest {}));
                    }
                    if ui
                        .button("+ Variation")
                        .on_hover_text("Copy the current pattern into a new bar.")
                        .clicked()
                    {
                        edit = Some(BarEdit::Add(ArrangementBar::Variation {
                            pattern: rack_pattern.clone(),
                        }));
                    }
                });
                ui.add_space(4.0);
                if bars.is_empty() {
                    ui.label(
                        RichText::new(
                            "Add a bar to build an arrangement. With no bars, Arrangement plays the main pattern.",
                        )
                        .color(MUTED),
                    );
                    return;
                }
                ui.horizontal(|ui| {
                    let gap = ui.spacing().item_spacing.x;
                    let width = (ui.available_width() - gap * (BARS_PER_PAGE - 1) as f32)
                        / BARS_PER_PAGE as f32;
                    for (index, bar) in bars.iter().enumerate().take(end).skip(first) {
                        let name = match bar {
                            ArrangementBar::Pattern { pattern_id } => project
                                .patterns
                                .iter()
                                .find(|pattern| pattern.id == *pattern_id)
                                .map_or("Named pattern", |pattern| &pattern.name),
                            _ => bar_label(bar),
                        };
                        let color = if index == selected {
                            TEXT
                        } else {
                            match bar {
                                ArrangementBar::Main {} => TEXT,
                                ArrangementBar::Variation { .. }
                                | ArrangementBar::Pattern { .. } => TRACK[1],
                                ArrangementBar::Rest {} => MUTED,
                            }
                        };
                        let label_width = (width - 2.0 * ui.spacing().button_padding.x).max(0.0);
                        let mut label = egui::text::LayoutJob::simple(
                            format!("{}\n{}", index + 1, name.lines().next().unwrap_or(name)),
                            FontId::monospace(11.0),
                            color,
                            label_width,
                        );
                        label.wrap.max_rows = 2;
                        label.wrap.break_anywhere = true;
                        let label = ui.fonts_mut(|fonts| fonts.layout_job(label));
                        let response = ui.add_sized(
                            [width, 44.0],
                            egui::Button::new(label).selected(index == selected),
                        );
                        if current == Some(index) {
                            let edge = response.rect.shrink(2.0);
                            ui.painter().line_segment(
                                [edge.left_bottom(), edge.right_bottom()],
                                Stroke::new(2.0_f32, ACCENT),
                            );
                        }
                        if response.clicked() {
                            self.selected_bar = index;
                        }
                        if response.double_clicked() {
                            seek = Some(index * STEPS);
                        }
                        response.on_hover_text(format!("{name}\nDouble-click to seek during playback."));
                    }
                });
                ui.horizontal(|ui| {
                    let selected = self.selected_bar;
                    if count > BARS_PER_PAGE {
                        if ui
                            .add_enabled(first > 0, egui::Button::new("‹ Page"))
                            .clicked()
                        {
                            self.selected_bar = first.saturating_sub(BARS_PER_PAGE);
                        }
                        if ui
                            .add_enabled(end < count, egui::Button::new("Page ›"))
                            .clicked()
                        {
                            self.selected_bar = end;
                        }
                        ui.separator();
                    }
                    if ui
                        .add_enabled(selected > 0, egui::Button::new("Move left"))
                        .clicked()
                    {
                        edit = Some(BarEdit::Move(selected.saturating_sub(1)));
                    }
                    if ui
                        .add_enabled(selected + 1 < count, egui::Button::new("Move right"))
                        .clicked()
                    {
                        edit = Some(BarEdit::Move(selected + 1));
                    }
                    ui.menu_button("Replace with", |ui| {
                        if self.selected_pattern.is_some()
                            && ui
                                .button("Selected pattern")
                                .on_hover_text(&selected_pattern_name)
                                .clicked()
                        {
                            edit = Some(BarEdit::Replace(selected_pattern_bar.clone()));
                            ui.close();
                        }
                        for (label, bar) in [
                            ("Main pattern", ArrangementBar::Main {}),
                            ("Rest", ArrangementBar::Rest {}),
                            (
                                "Current pattern as variation",
                                ArrangementBar::Variation {
                                    pattern: rack_pattern.clone(),
                                },
                            ),
                        ] {
                            if ui.button(label).clicked() {
                                edit = Some(BarEdit::Replace(bar));
                                ui.close();
                            }
                        }
                    });
                    if ui.button("Remove").clicked() {
                        edit = Some(BarEdit::Remove);
                    }
                });
            });
        if let Some(edit) = edit {
            self.edit_bar(edit);
        }
        if let Some(step) = seek {
            self.send(PlaybackCommand::SeekStep(step));
        }
    }

    fn edit_bar(&mut self, edit: BarEdit) {
        let mut bars = self.session.editor.project().arrangement.clone();
        let selected = self.selected_bar;
        match edit {
            BarEdit::Add(bar) => {
                self.selected_bar = bars.len();
                bars.push(bar);
            }
            BarEdit::Move(to) if selected < bars.len() && to < bars.len() => {
                bars.swap(selected, to);
                self.selected_bar = to;
            }
            BarEdit::Replace(bar) => {
                let Some(current) = bars.get_mut(selected) else {
                    return;
                };
                *current = bar;
            }
            BarEdit::Remove if selected < bars.len() => {
                bars.remove(selected);
                self.selected_bar = selected.min(bars.len().saturating_sub(1));
            }
            _ => return,
        }
        self.edit(&[EditCommand::SetArrangement { bars }]);
    }

    pub(super) fn rack_play_step(&self) -> Option<usize> {
        let step = self.playback.as_ref()?.position_steps() as usize;
        let bars = &self.session.editor.project().arrangement;
        let matches = if !self.arrangement_mode {
            true
        } else if bars.is_empty() {
            self.selected_pattern.is_none()
        } else {
            match &bars[(step / STEPS) % bars.len()] {
                ArrangementBar::Main {} => self.selected_pattern.is_none(),
                ArrangementBar::Pattern { pattern_id } => {
                    self.selected_pattern == Some(*pattern_id)
                }
                ArrangementBar::Variation { .. } | ArrangementBar::Rest {} => false,
            }
        };
        matches.then_some(step % STEPS)
    }
}

fn bar_label(bar: &ArrangementBar) -> &'static str {
    match bar {
        ArrangementBar::Main {} => "Pattern",
        ArrangementBar::Variation { .. } => "Variation",
        ArrangementBar::Pattern { .. } => "Named pattern",
        ArrangementBar::Rest {} => "Rest",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrangement_edits_share_project_history_and_variations_are_snapshots() {
        let mut app =
            Workspace::new(Session::open(None).unwrap(), &egui::Context::default()).unwrap();
        let original_pattern = app.session.editor.project().pattern.clone();
        app.edit_bar(BarEdit::Add(ArrangementBar::Main {}));
        app.edit_bar(BarEdit::Add(ArrangementBar::Rest {}));
        app.edit_bar(BarEdit::Move(0));
        app.edit_bar(BarEdit::Replace(ArrangementBar::Variation {
            pattern: original_pattern.clone(),
        }));
        app.edit(&[EditCommand::SetStep {
            track_id: 1,
            step: 0,
            velocity: 0,
        }]);
        assert_eq!(
            app.session.editor.project().arrangement,
            [
                ArrangementBar::Variation {
                    pattern: original_pattern
                },
                ArrangementBar::Main {},
            ]
        );
        app.edit_bar(BarEdit::Remove);
        assert_eq!(
            app.session.editor.project().arrangement,
            [ArrangementBar::Main {}]
        );
        assert!(app.session.editor.undo());
        app.changed();
        assert!(matches!(
            app.session.editor.project().arrangement.first(),
            Some(ArrangementBar::Variation { .. })
        ));
        assert!(app.session.editor.redo());
        app.changed();
        assert_eq!(
            app.session.editor.project().arrangement,
            [ArrangementBar::Main {}]
        );
    }
}
