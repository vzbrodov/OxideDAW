use super::widgets::{self as w, Icon};
use super::*;
use eframe::egui::{Color32, Painter};

fn label(p: &Painter, pos: egui::Pos2, text: impl ToString, size: f32, color: Color32) {
    p.text(
        pos,
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(size),
        color,
    );
}
fn number(p: &Painter, pos: egui::Pos2, text: impl ToString, size: f32, color: Color32) {
    p.text(
        pos,
        Align2::LEFT_CENTER,
        text,
        FontId::monospace(size),
        color,
    );
}
fn section(ui: &mut egui::Ui, title: &str, detail: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 29.0), Sense::hover());
    w::gradient(ui.painter(), rect, RAISED, PANEL);
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, SHADOW),
    );
    label(
        ui.painter(),
        rect.left_center() + vec2(10.0, 0.0),
        title,
        12.0,
        TEXT,
    );
    ui.painter().text(
        rect.right_center() - vec2(12.0, 0.0),
        Align2::RIGHT_CENTER,
        detail,
        FontId::proportional(10.5),
        MUTED,
    );
}

impl Workspace {
    pub(super) fn select_channel(&mut self, track: usize, step: Option<usize>) {
        self.selected = track;
        if let Some(step) = step {
            self.selected_step = step;
        }
        self.velocity = self
            .rack_pattern()
            .get(track, self.selected_step)
            .unwrap_or(0);
        self.sample_input = self
            .session
            .sample_path(track)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
    }

    pub(super) fn audition(&mut self, track: usize) {
        self.stop();
        if let Some(sample) = &self.samples[track] {
            match Playback::start(sample.clone(), None, self.preview_volume * self.volume) {
                Ok(playback) => {
                    let seconds = sample.samples.len() as f64
                        / sample.channels as f64
                        / sample.sample_rate as f64;
                    self.preview_until =
                        Some(std::time::Instant::now() + Duration::from_secs_f64(seconds + 0.2));
                    self.preview = Some(playback);
                    self.notice(
                        format!(
                            "Auditioning {}.",
                            self.session.editor.project().tracks[track].name
                        ),
                        false,
                    );
                }
                Err(error) => self.notice(format!("Cannot audition sample: {error}"), true),
            }
        } else {
            self.notice("Choose an audio file for this channel first.", true);
        }
    }

    pub(super) fn transport(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("transport")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new("OXIDE").size(13.0).strong());
                    ui.separator();
                    ui.menu_button("File", |ui| {
                        if ui.button("Open project…").clicked() {
                            self.show_file(FileAction::Open);
                            ui.close();
                        }
                        if ui.button("Save    Ctrl+S").clicked() {
                            self.save_requested = true;
                            ui.close();
                        }
                        if ui.button("Save as…").clicked() {
                            self.show_file(FileAction::Save);
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.wav_export.is_none(),
                                egui::Button::new("Export WAV…"),
                            )
                            .clicked()
                        {
                            self.show_file(FileAction::ExportWav);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Close    Ctrl+Q").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            ui.close();
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        if ui.button("Undo    Ctrl+Z").clicked() {
                            if self.session.editor.undo() {
                                self.changed();
                            }
                            ui.close();
                        }
                        if ui.button("Redo    Ctrl+Shift+Z").clicked() {
                            if self.session.editor.redo() {
                                self.changed();
                            }
                            ui.close();
                        }
                    });
                    ui.menu_button("View", |ui| {
                        ui.checkbox(&mut self.browser_visible, "Sample browser");
                    });
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{}{}",
                            self.session.editor.project().name,
                            if self.session.dirty() { " •" } else { "" }
                        ))
                        .color(MUTED)
                        .size(11.5),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("OxideDAW  /  Pattern studio")
                                .size(10.5)
                                .color(MUTED),
                        );
                    });
                });
                let (background, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
                w::gradient(
                    ui.painter(),
                    background,
                    Color32::from_gray(128),
                    Color32::from_gray(91),
                );
                ui.painter().line_segment(
                    [background.left_top(), background.right_top()],
                    Stroke::new(1.0_f32, HILITE),
                );
                ui.painter().line_segment(
                    [background.left_bottom(), background.right_bottom()],
                    Stroke::new(1.0_f32, SHADOW),
                );
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(background.shrink2(vec2(12.0, 10.0))),
                    |ui| {
                        ui.horizontal(|ui| {
                            if w::icon_button(ui, Icon::Folder, "Open project", false).clicked() {
                                self.show_file(FileAction::Open);
                            }
                            if w::button(ui, "Save", 45.0, false).clicked() {
                                self.save_requested = true;
                            }
                            if w::button(ui, "Undo", 44.0, false).clicked()
                                && self.session.editor.undo()
                            {
                                self.changed();
                            }
                            if w::button(ui, "Redo", 44.0, false).clicked()
                                && self.session.editor.redo()
                            {
                                self.changed();
                            }
                            ui.add_space(12.0);
                            if w::icon_button(ui, Icon::Rewind, "Return to start", false).clicked()
                            {
                                self.send(PlaybackCommand::SeekStep(0));
                            }
                            if w::icon_button(ui, Icon::Stop, "Stop", false).clicked() {
                                self.stop();
                            }
                            if w::icon_button(
                                ui,
                                if self.playing {
                                    Icon::Pause
                                } else {
                                    Icon::Play
                                },
                                "Play / Pause · Space",
                                self.playing,
                            )
                            .clicked()
                            {
                                self.toggle_play();
                            }
                            let (loop_rect, loop_response) =
                                ui.allocate_exact_size(vec2(34.0, 29.0), Sense::hover());
                            w::plate(ui.painter(), loop_rect, ACCENT, true);
                            w::icon(
                                ui.painter(),
                                loop_rect.center(),
                                Icon::Loop,
                                12.0,
                                Color32::BLACK,
                            );
                            loop_response.on_hover_text(
                                "Loop the pattern or arrangement selected in the workspace.",
                            );
                            ui.add_space(12.0);
                            let (lcd, _) =
                                ui.allocate_exact_size(vec2(286.0, 40.0), Sense::hover());
                            ui.painter().rect_filled(lcd, 3, WELL);
                            ui.painter().rect_stroke(
                                lcd,
                                3,
                                Stroke::new(1.0_f32, SHADOW),
                                StrokeKind::Inside,
                            );
                            ui.painter().line_segment(
                                [
                                    lcd.left_bottom() + vec2(1.0, 1.0),
                                    lcd.right_bottom() + vec2(-1.0, 1.0),
                                ],
                                Stroke::new(1.0_f32, HILITE),
                            );
                            let position =
                                self.playback.as_ref().map_or(0.0, Playback::position_steps);
                            let bars = self.session.editor.project().arrangement.len().max(1);
                            let position = if self.arrangement_mode {
                                position.rem_euclid((bars * STEPS) as f32)
                            } else {
                                position
                            };
                            let fraction = ((position.fract() * 240.0) as usize).min(239);
                            number(
                                ui.painter(),
                                lcd.min + vec2(12.0, 15.0),
                                format!(
                                    "{:03}.{}.{}",
                                    position as usize / 16 + 1,
                                    position as usize % 16 / 4 + 1,
                                    format_args!("{:03}", fraction)
                                ),
                                21.0,
                                Color32::from_rgb(210, 223, 226),
                            );
                            label(
                                ui.painter(),
                                lcd.min + vec2(12.0, 33.0),
                                "BAR     BEAT     TICK",
                                8.5,
                                MUTED,
                            );
                            ui.painter().line_segment(
                                [lcd.min + vec2(169.0, 5.0), lcd.min + vec2(169.0, 35.0)],
                                Stroke::new(1.0_f32, EDGE),
                            );
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(Rect::from_min_size(
                                    lcd.min + vec2(177.0, 3.0),
                                    vec2(99.0, 24.0),
                                )),
                                |ui| {
                                    let response = ui.add(
                                        egui::DragValue::new(&mut self.bpm)
                                            .range(1.0..=999.0)
                                            .speed(0.1)
                                            .fixed_decimals(2),
                                    );
                                    if response.drag_stopped() || response.lost_focus() {
                                        self.edit(&[EditCommand::SetBpm { bpm: self.bpm }]);
                                    }
                                },
                            );
                            label(
                                ui.painter(),
                                lcd.min + vec2(181.0, 33.0),
                                "TEMPO / BPM",
                                8.5,
                                MUTED,
                            );
                            ui.advance_cursor_after_rect(lcd);
                            ui.add_space(12.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new("4/4     1/16").monospace().color(TEXT));
                                let source = if self.arrangement_mode
                                    && !self.session.editor.project().arrangement.is_empty()
                                {
                                    format!(
                                        "Arrangement  •  {bars} bar{}",
                                        if bars == 1 { "" } else { "s" }
                                    )
                                } else {
                                    let name = if self.arrangement_mode {
                                        "Main pattern"
                                    } else {
                                        self.rack_pattern_name()
                                    };
                                    format!("Pattern  •  {name}")
                                };
                                let fitted =
                                    w::fit_label(ui, &source, FontId::proportional(10.0), 140.0);
                                ui.label(RichText::new(fitted).size(10.0).color(MUTED))
                                    .on_hover_text(source);
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if w::button(ui, "Library", 60.0, self.browser_visible)
                                        .clicked()
                                    {
                                        self.browser_visible = !self.browser_visible;
                                    }
                                    ui.add_space(9.0);
                                    let (meter, _) =
                                        ui.allocate_exact_size(vec2(100.0, 23.0), Sense::hover());
                                    w::meter(
                                        ui.painter(),
                                        Rect::from_min_size(
                                            meter.min + vec2(0.0, 1.0),
                                            vec2(100.0, 7.0),
                                        ),
                                        self.levels[TRACKS],
                                        true,
                                    );
                                    w::meter(
                                        ui.painter(),
                                        Rect::from_min_size(
                                            meter.min + vec2(0.0, 10.0),
                                            vec2(100.0, 7.0),
                                        ),
                                        self.levels[TRACKS + 1],
                                        true,
                                    );
                                    label(
                                        ui.painter(),
                                        meter.min + vec2(0.0, 23.0),
                                        "STEREO OUTPUT",
                                        7.5,
                                        MUTED,
                                    );
                                },
                            );
                        });
                    },
                );
            });
    }

    pub(super) fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(25.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(6))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if self.playing { "AUDIO ●" } else { "AUDIO" })
                            .size(9.0)
                            .color(if self.playing { METER_GREEN } else { MUTED }),
                    );
                    ui.separator();
                    ui.add(
                        egui::Label::new(
                            RichText::new(&self.status).size(10.5).color(if self.error {
                                METER_YELLOW
                            } else {
                                MUTED
                            }),
                        )
                        .truncate(),
                    );
                });
            });
    }

    pub(super) fn browser(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("browser")
            .resizable(true)
            .default_width(208.0)
            .width_range(180.0..=300.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(0))
            .show(ctx, |ui| {
                section(ui, "Library", "AUDIO FILES");
                egui::Frame::new().inner_margin(9).show(ui, |ui| {
                    let folder = ui.add(
                        egui::TextEdit::singleline(&mut self.folder)
                            .font(egui::TextStyle::Small)
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        if w::button(ui, "Up", 35.0, false).clicked() {
                            if let Some(parent) = Path::new(&self.folder).parent() {
                                self.folder = parent.display().to_string();
                                self.refresh_browser();
                            }
                        }
                        if w::button(ui, "Open folder", 90.0, false).clicked()
                            || (folder.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            self.refresh_browser();
                        }
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .hint_text("Search audio files")
                            .desired_width(f32::INFINITY),
                    );
                });
                let (header, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
                ui.painter().rect_filled(header, 0, BACK);
                w::icon(
                    ui.painter(),
                    header.left_center() + vec2(13.0, 0.0),
                    Icon::Folder,
                    10.0,
                    ACCENT,
                );
                label(
                    ui.painter(),
                    header.left_center() + vec2(25.0, 0.0),
                    "Current folder",
                    11.0,
                    TEXT,
                );
                let filter = self.filter.to_lowercase();
                let mut picked = None;
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .id_salt("library-files")
                    .show(ui, |ui| {
                        for path in &self.files {
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            if !name.to_lowercase().contains(&filter) {
                                continue;
                            }
                            let directory = path.is_dir();
                            let (rect, response) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 25.0),
                                Sense::click(),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name)
                            });
                            let selected =
                                self.loaded_paths[self.selected].as_deref() == Some(path.as_path());
                            if selected || response.hovered() || response.has_focus() {
                                ui.painter().rect_filled(
                                    rect,
                                    0,
                                    if selected { RAISED } else { CONTROL },
                                );
                            }
                            w::icon(
                                ui.painter(),
                                rect.left_center() + vec2(17.0, 0.0),
                                if directory { Icon::Folder } else { Icon::Wave },
                                11.0,
                                if selected {
                                    TRACK[self.selected]
                                } else {
                                    MUTED
                                },
                            );
                            let name = if name.chars().count() > 25 {
                                format!("{}…", name.chars().take(24).collect::<String>())
                            } else {
                                name.into_owned()
                            };
                            label(
                                ui.painter(),
                                rect.left_center() + vec2(30.0, 0.0),
                                name,
                                11.5,
                                TEXT,
                            );
                            if response.on_hover_text(path.display().to_string()).clicked() {
                                picked = Some(path.clone());
                            }
                        }
                        if self.files.is_empty() {
                            ui.label(
                                RichText::new("Choose a folder containing WAV, FLAC or MP3 files.")
                                    .color(MUTED),
                            );
                        }
                    });
                if let Some(path) = picked {
                    if path.is_dir() {
                        self.folder = path.display().to_string();
                        self.refresh_browser();
                    } else {
                        self.assign(&path);
                    }
                }
                ui.add_space(10.0);
                egui::Frame::new()
                    .fill(BACK)
                    .inner_margin(11)
                    .show(ui, |ui| {
                        ui.label(RichText::new("ASSIGN TO CHANNEL").size(9.0).color(MUTED));
                        ui.label(
                            RichText::new(
                                &self.session.editor.project().tracks[self.selected].name,
                            )
                            .size(13.0)
                            .color(TRACK[self.selected]),
                        );
                        ui.label(
                            RichText::new("Click a file above, or drag audio into the workspace.")
                                .size(10.5)
                                .color(MUTED),
                        );
                    });
            });
    }

    pub(super) fn master(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("master")
            .resizable(false)
            .exact_width(119.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(0))
            .show(ctx, |ui| {
                section(ui, "Master", "");
                egui::Frame::new().inner_margin(9).show(ui, |ui| {
                    ui.label(RichText::new("STEREO OUT").size(9.0).color(MUTED));
                    let db = if self.volume < 0.0001 {
                        "−∞".into()
                    } else {
                        format!("{:.1}", 20.0 * self.volume.log10())
                    };
                    ui.label(RichText::new(format!("{db} dB")).monospace().size(15.0));
                    let (area, response) =
                        ui.allocate_exact_size(vec2(97.0, 368.0), Sense::click_and_drag());
                    let meter_rect =
                        Rect::from_min_size(area.min + vec2(4.0, 17.0), vec2(12.0, 300.0));
                    w::meter(ui.painter(), meter_rect, self.levels[TRACKS], false);
                    w::meter(
                        ui.painter(),
                        meter_rect.translate(vec2(14.0, 0.0)),
                        self.levels[TRACKS + 1],
                        false,
                    );
                    let rail = Rect::from_min_size(area.min + vec2(65.0, 17.0), vec2(6.0, 300.0));
                    ui.painter().rect_filled(rail.expand(1.0), 3, SHADOW);
                    ui.painter().line_segment(
                        [rail.right_top(), rail.right_bottom()],
                        Stroke::new(1.0_f32, HILITE),
                    );
                    for db_tick in [0, -6, -12, -18, -24, -36, -48, -60] {
                        let y = rail.top() - db_tick as f32 / 60.0 * rail.height();
                        ui.painter().line_segment(
                            [pos2(rail.left() - 6.0, y), pos2(rail.left() - 2.0, y)],
                            Stroke::new(1.0_f32, HILITE),
                        );
                        number(
                            ui.painter(),
                            pos2(area.left() + 34.0, y),
                            db_tick,
                            8.5,
                            MUTED,
                        );
                    }
                    if (response.dragged() || response.clicked())
                        && response
                            .interact_pointer_pos()
                            .is_some_and(|p| p.x > area.left() + 46.0)
                    {
                        if let Some(pointer) = response.interact_pointer_pos() {
                            let value =
                                (1.0 - (pointer.y - rail.top()) / rail.height()).clamp(0.0, 1.0);
                            self.volume = if value == 0.0 {
                                0.0
                            } else {
                                10.0_f32.powf((-60.0 + value * 60.0) / 20.0)
                            };
                        }
                    }
                    if response.has_focus() {
                        ui.input(|i| {
                            if i.key_pressed(egui::Key::ArrowUp) {
                                self.volume = (self.volume + 0.01).min(1.0);
                            }
                            if i.key_pressed(egui::Key::ArrowDown) {
                                self.volume = (self.volume - 0.01).max(0.0);
                            }
                        });
                    }
                    let position = if self.volume > 0.0 {
                        ((20.0 * self.volume.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let handle = Rect::from_center_size(
                        pos2(rail.center().x, rail.bottom() - position * rail.height()),
                        vec2(25.0, 36.0),
                    );
                    w::plate(ui.painter(), handle, Color32::from_gray(153), false);
                    for y in [-6.0, -2.0, 2.0, 6.0] {
                        ui.painter().line_segment(
                            [
                                handle.center() + vec2(-7.0, y),
                                handle.center() + vec2(7.0, y),
                            ],
                            Stroke::new(1.0_f32, SHADOW),
                        );
                    }
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            handle.expand(2.0),
                            2,
                            Stroke::new(1.0_f32, ACCENT),
                            StrokeKind::Inside,
                        );
                    }
                    if self.volume != self.session.editor.project().master_volume
                        && !ui.input(|i| i.pointer.primary_down())
                    {
                        self.edit(&[EditCommand::SetMasterVolume {
                            volume: self.volume,
                        }]);
                    }
                    let peak = self.levels[TRACKS].max(self.levels[TRACKS + 1]);
                    let peak_text = if peak > 0.0001 {
                        format!("{:.1}", 20.0 * peak.log10())
                    } else {
                        "−∞".into()
                    };
                    number(
                        ui.painter(),
                        area.min + vec2(4.0, 343.0),
                        format!("PEAK {peak_text}"),
                        9.0,
                        MUTED,
                    );
                    ui.separator();
                    ui.label(RichText::new("Default device").size(10.0).color(MUTED));
                    ui.label(
                        RichText::new("Hard limiter\n−1.0 … +1.0")
                            .size(9.5)
                            .color(MUTED),
                    );
                    ui.add_space(10.0);
                    if w::button(ui, "0 dB", 44.0, false)
                        .on_hover_text("Set master gain to unity")
                        .clicked()
                    {
                        self.edit(&[EditCommand::SetMasterVolume { volume: 1.0 }]);
                    }
                });
            });
    }
}
