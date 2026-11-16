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
    fn select_channel(&mut self, track: usize, step: Option<usize>) {
        self.selected = track;
        if let Some(step) = step {
            self.selected_step = step;
        }
        self.velocity = self
            .session
            .editor
            .project()
            .pattern
            .get(track, self.selected_step)
            .unwrap_or(0);
        self.sample_input = self
            .session
            .sample_path(track)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
    }

    fn audition(&mut self, track: usize) {
        self.stop();
        if let Some(sample) = &self.samples[track] {
            match Playback::start(sample.clone(), None, self.volume) {
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
                                "One-bar pattern loop: the pattern repeats continuously.",
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
                                ui.label(
                                    RichText::new("Pattern 01  •  1 bar")
                                        .size(10.0)
                                        .color(MUTED),
                                );
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
                section(ui, "Project media", "4 CHANNELS");
                for (track, &color) in TRACK.iter().enumerate() {
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 39.0), Sense::click());
                    if self.selected == track {
                        ui.painter().rect_filled(rect, 0, RAISED);
                    }
                    ui.painter().rect_filled(
                        Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(3.0, 19.0)),
                        0,
                        color,
                    );
                    label(
                        ui.painter(),
                        rect.min + vec2(24.0, 12.0),
                        &self.session.editor.project().tracks[track].name,
                        11.5,
                        TEXT,
                    );
                    label(
                        ui.painter(),
                        rect.min + vec2(24.0, 28.0),
                        if self.media_errors[track].is_some() {
                            "Sample unavailable"
                        } else if self.samples[track].is_some() {
                            "Audio sample  /  Sampler"
                        } else {
                            "No sample assigned"
                        },
                        9.5,
                        MUTED,
                    );
                    if response.clicked() {
                        self.select_channel(track, None);
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

    pub(super) fn timeline(&mut self, ui: &mut egui::Ui) {
        section(ui, "Pattern timeline", "LOOP MONITOR  /  FOUR REPEATS");
        let row_height = 54.0;
        let header = 191.0;
        let (rect, _) = ui.allocate_exact_size(
            vec2(
                ui.available_width(),
                35.0 + row_height * TRACKS as f32 + 20.0,
            ),
            Sense::hover(),
        );
        let grid = Rect::from_min_max(rect.min + vec2(header, 35.0), rect.max - vec2(0.0, 20.0));
        let beat_width = grid.width() / 16.0;
        ui.painter().rect_filled(rect, 0, BACK);
        ui.painter().rect_filled(
            Rect::from_min_max(rect.min + vec2(header, 0.0), pos2(rect.right(), grid.top())),
            0,
            WELL,
        );
        for beat in 0..=16 {
            let x = grid.left() + beat as f32 * beat_width;
            let bar_start = beat % 4 == 0;
            ui.painter().line_segment(
                [
                    pos2(x, rect.top() + if bar_start { 9.0 } else { 23.0 }),
                    pos2(x, grid.bottom()),
                ],
                Stroke::new(1.0_f32, if bar_start { EDGE } else { PANEL }),
            );
            if beat < 16 {
                number(
                    ui.painter(),
                    pos2(x + 5.0, rect.top() + 14.0),
                    if bar_start {
                        format!("{}", beat / 4 + 1)
                    } else {
                        format!("{}.{}", beat / 4 + 1, beat % 4 + 1)
                    },
                    if bar_start { 12.0 } else { 9.0 },
                    if bar_start { TEXT } else { MUTED },
                );
            }
            if beat < 16 {
                for sub in 1..4 {
                    let sub_x = x + sub as f32 / 4.0 * beat_width;
                    ui.painter().line_segment(
                        [pos2(sub_x, grid.top()), pos2(sub_x, grid.bottom())],
                        Stroke::new(1.0_f32, Color32::from_gray(44)),
                    );
                }
            }
        }
        label(
            ui.painter(),
            rect.min + vec2(12.0, 16.0),
            "CHANNEL",
            9.0,
            MUTED,
        );
        for (track, &color) in TRACK.iter().enumerate() {
            let y = grid.top() + track as f32 * row_height;
            let left = Rect::from_min_size(pos2(rect.left(), y), vec2(header - 1.0, row_height));
            w::gradient(
                ui.painter(),
                left,
                if self.selected == track {
                    CONTROL
                } else {
                    RAISED
                },
                if self.selected == track {
                    RAISED
                } else {
                    PANEL
                },
            );
            ui.painter().line_segment(
                [left.left_bottom(), pos2(rect.right(), left.bottom())],
                Stroke::new(1.0_f32, SHADOW),
            );
            ui.painter().rect_filled(
                Rect::from_min_size(left.min + vec2(7.0, 8.0), vec2(3.0, row_height - 16.0)),
                1,
                color,
            );
            let badge = Rect::from_min_size(left.min + vec2(16.0, 10.0), vec2(31.0, 31.0));
            w::plate(ui.painter(), badge, PANEL, true);
            w::icon(ui.painter(), badge.center(), Icon::Wave, 17.0, color);
            label(
                ui.painter(),
                left.min + vec2(57.0, 16.0),
                &self.session.editor.project().tracks[track].name,
                12.0,
                TEXT,
            );
            label(
                ui.painter(),
                left.min + vec2(57.0, 33.0),
                "Sampler  •  One-shot",
                9.0,
                MUTED,
            );
            w::meter(
                ui.painter(),
                Rect::from_min_size(left.min + vec2(57.0, 43.0), vec2(111.0, 4.0)),
                self.levels[track],
                true,
            );
            let response = ui.interact(
                left,
                ui.id().with(("timeline-channel", track)),
                Sense::click(),
            );
            if response.clicked() {
                self.select_channel(track, None);
            }
            for bar in 0..4 {
                let clip = Rect::from_min_size(
                    pos2(grid.left() + bar as f32 * beat_width * 4.0 + 1.0, y + 3.0),
                    vec2(beat_width * 4.0 - 3.0, row_height - 6.0),
                );
                let p = ui.painter().with_clip_rect(clip);
                w::gradient(
                    &p,
                    clip,
                    color.gamma_multiply(0.64),
                    color.gamma_multiply(0.42),
                );
                p.rect_stroke(
                    clip,
                    2,
                    Stroke::new(1.0_f32, color.gamma_multiply(0.85)),
                    StrokeKind::Inside,
                );
                let title = Rect::from_min_size(clip.min, vec2(clip.width(), 13.0));
                p.rect_filled(title, 0, color.gamma_multiply(0.8));
                label(
                    &p,
                    title.left_center() + vec2(6.0, 0.0),
                    format!(
                        "{}  /  Pattern 01",
                        self.session.editor.project().tracks[track].name
                    ),
                    9.0,
                    TEXT,
                );
                let step_width = clip.width() / STEPS as f32;
                for step in 0..STEPS {
                    let velocity = self
                        .session
                        .editor
                        .project()
                        .pattern
                        .get(track, step)
                        .unwrap_or(0);
                    if velocity == 0 {
                        continue;
                    }
                    let x = clip.left() + step as f32 * step_width;
                    if let Some(sample) = &self.samples[track] {
                        let duration = sample.samples.len() as f32
                            / sample.channels as f32
                            / sample.sample_rate as f32;
                        let width = duration * self.bpm as f32 / 15.0 * step_width;
                        let count = (width as usize).clamp(2, 140);
                        for i in 0..count {
                            let peak = self.peaks[track]
                                .get(i * self.peaks[track].len() / count)
                                .copied()
                                .unwrap_or(0.0);
                            let height = peak * velocity as f32 / 127.0 * 15.0;
                            let px = x + i as f32 / count as f32 * width;
                            let center = clip.center().y + 6.0;
                            p.line_segment(
                                [pos2(px, center - height), pos2(px, center + height)],
                                Stroke::new(1.0_f32, color.lerp_to_gamma(Color32::WHITE, 0.45)),
                            );
                        }
                    } else {
                        p.rect_filled(
                            Rect::from_min_size(
                                pos2(x + 2.0, clip.top() + 25.0),
                                vec2((step_width - 3.0).max(2.0), 5.0),
                            ),
                            1,
                            color,
                        );
                    }
                }
                let response = ui.interact(
                    clip,
                    ui.id().with(("monitor-clip", track, bar)),
                    Sense::click(),
                );
                if response.clicked() {
                    let step = response
                        .interact_pointer_pos()
                        .map_or(0, |p| ((p.x - clip.left()) / step_width) as usize)
                        .min(15);
                    self.select_channel(track, Some(step));
                    self.send(PlaybackCommand::SeekStep(step));
                }
                response.on_hover_text(
                    "Current pattern repeated. Click to select a channel and seek within its bar.",
                );
            }
        }
        let position = self.playback.as_ref().map_or(0.0, Playback::position_steps);
        let play_x = grid.left() + (position.rem_euclid(64.0) / 64.0) * grid.width();
        ui.painter().line_segment(
            [pos2(play_x, rect.top() + 24.0), pos2(play_x, grid.bottom())],
            Stroke::new(1.0_f32, Color32::WHITE),
        );
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                pos2(play_x - 4.0, rect.top() + 21.0),
                pos2(play_x + 4.0, rect.top() + 21.0),
                pos2(play_x, rect.top() + 28.0),
            ],
            Color32::WHITE,
            Stroke::NONE,
        ));
        let seconds_per_bar = 240.0 / self.bpm;
        for bar in 0..4 {
            number(
                ui.painter(),
                pos2(
                    grid.left() + bar as f32 * beat_width * 4.0 + 4.0,
                    rect.bottom() - 8.0,
                ),
                format!("{:.2}s", bar as f64 * seconds_per_bar),
                9.0,
                MUTED,
            );
        }
    }

    pub(super) fn rack(&mut self, ui: &mut egui::Ui) {
        section(ui, "Step sequencer", "PATTERN 01  /  16 STEPS  /  4/4");
        let labels = 191.0;
        let (rect, _) = ui.allocate_exact_size(
            vec2(ui.available_width(), 22.0 + TRACKS as f32 * 37.0 + 40.0),
            Sense::hover(),
        );
        let gap = 5.0;
        let pad_width = (rect.width() - labels - 24.0 - 15.0 * gap) / STEPS as f32;
        let play_step = self
            .playback
            .as_ref()
            .map(|p| p.position_steps() as usize % STEPS);
        for step in 0..STEPS {
            let x = rect.left() + labels + 12.0 + step as f32 * (pad_width + gap);
            number(
                ui.painter(),
                pos2(x + 2.0, rect.top() + 12.0),
                format!("{:02}", step + 1),
                9.0,
                if step % 4 == 0 { TEXT } else { MUTED },
            );
        }
        for (track, &color) in TRACK.iter().enumerate() {
            let top = rect.top() + 22.0 + track as f32 * 37.0;
            let row = Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), 36.0));
            ui.painter().rect_filled(
                row,
                0,
                if self.selected == track {
                    RAISED
                } else {
                    PANEL
                },
            );
            ui.painter().line_segment(
                [row.left_bottom(), row.right_bottom()],
                Stroke::new(1.0_f32, SHADOW),
            );
            let name_rect = Rect::from_min_size(row.min, vec2(labels - 42.0, 36.0));
            number(
                ui.painter(),
                row.min + vec2(9.0, 17.0),
                format!("{:02}", track + 1),
                9.0,
                MUTED,
            );
            ui.painter()
                .circle_filled(row.min + vec2(34.0, 17.0), 3.0, color);
            label(
                ui.painter(),
                row.min + vec2(47.0, 13.0),
                &self.session.editor.project().tracks[track].name,
                11.5,
                TEXT,
            );
            label(
                ui.painter(),
                row.min + vec2(47.0, 27.0),
                if self.samples[track].is_some() {
                    "Sample loaded"
                } else {
                    "Choose sample"
                },
                8.5,
                MUTED,
            );
            if ui
                .interact(
                    name_rect,
                    ui.id().with(("rack-channel", track)),
                    Sense::click(),
                )
                .clicked()
            {
                self.select_channel(track, None);
            }
            let preview_rect = Rect::from_min_size(
                pos2(rect.left() + labels - 35.0, top + 7.0),
                vec2(23.0, 23.0),
            );
            w::plate(ui.painter(), preview_rect, CONTROL, false);
            w::icon(ui.painter(), preview_rect.center(), Icon::Play, 8.0, MUTED);
            if ui
                .interact(
                    preview_rect,
                    ui.id().with(("audition", track)),
                    Sense::click(),
                )
                .on_hover_text("Audition sample")
                .clicked()
            {
                self.audition(track);
            }
            for step in 0..STEPS {
                let pad = Rect::from_min_size(
                    pos2(
                        rect.left() + labels + 12.0 + step as f32 * (pad_width + gap),
                        top + 4.0,
                    ),
                    vec2(pad_width, 23.0),
                );
                let velocity = self
                    .session
                    .editor
                    .project()
                    .pattern
                    .get(track, step)
                    .unwrap_or(0);
                let response =
                    ui.interact(pad, ui.id().with(("step", track, step)), Sense::click());
                response.widget_info(|| {
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
                let fill = if velocity > 0 {
                    color.gamma_multiply(0.6 + velocity as f32 / 127.0 * 0.4)
                } else if (step / 4) % 2 == 0 {
                    Color32::from_gray(76)
                } else {
                    Color32::from_gray(57)
                };
                w::plate(ui.painter(), pad, fill, velocity > 0);
                let led =
                    Rect::from_min_size(pad.min + vec2(3.0, 3.0), vec2(pad.width() - 6.0, 2.0));
                ui.painter().rect_filled(
                    led,
                    0,
                    if velocity > 0 {
                        color.lerp_to_gamma(Color32::WHITE, 0.3)
                    } else {
                        SHADOW
                    },
                );
                if velocity > 0 {
                    let bar = Rect::from_min_size(
                        pos2(pad.left() + 2.0, pad.bottom() + 3.0),
                        vec2((pad.width() - 4.0) * velocity as f32 / 127.0, 3.0),
                    );
                    ui.painter().rect_filled(bar, 0, color.gamma_multiply(0.8));
                }
                if self.selected == track && self.selected_step == step {
                    ui.painter().rect_stroke(
                        pad.expand(1.0),
                        2,
                        Stroke::new(1.0_f32, TEXT),
                        StrokeKind::Inside,
                    );
                }
                if response.hovered() || response.has_focus() {
                    ui.painter().rect_stroke(
                        pad.shrink(1.0),
                        1,
                        Stroke::new(1.0_f32, TEXT),
                        StrokeKind::Inside,
                    );
                }
                if play_step == Some(step) {
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            pos2(pad.center().x - 2.0, row.bottom() - 4.0),
                            vec2(4.0, 2.0),
                        ),
                        0,
                        METER_GREEN,
                    );
                }
                if response.clicked() {
                    self.select_channel(track, Some(step));
                    self.velocity = if velocity == 0 { 100 } else { 0 };
                    self.edit(&[EditCommand::SetStep {
                        track_id: self.session.editor.project().tracks[track].id,
                        step,
                        velocity: self.velocity,
                    }]);
                } else if response.secondary_clicked() {
                    self.select_channel(track, Some(step));
                }
                response.on_hover_text(format!(
                    "Step {} · velocity {}\nClick toggles; right-click selects",
                    step + 1,
                    velocity
                ));
            }
        }
        let tools = Rect::from_min_size(
            pos2(rect.left() + 9.0, rect.bottom() - 30.0),
            vec2(rect.width() - 18.0, 28.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(tools), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} / Step {:02}",
                        self.session.editor.project().tracks[self.selected].name,
                        self.selected_step + 1
                    ))
                    .size(10.5)
                    .color(TRACK[self.selected]),
                );
                ui.add_space(10.0);
                ui.label(RichText::new("Velocity").size(10.0).color(MUTED));
                ui.add(egui::Slider::new(&mut self.velocity, 0..=127).text(""));
                if self.velocity
                    != self
                        .session
                        .editor
                        .project()
                        .pattern
                        .get(self.selected, self.selected_step)
                        .unwrap_or(0)
                    && !ui.input(|i| i.pointer.primary_down())
                {
                    self.edit(&[EditCommand::SetStep {
                        track_id: self.session.editor.project().tracks[self.selected].id,
                        step: self.selected_step,
                        velocity: self.velocity,
                    }]);
                }
                ui.add_space(14.0);
                if w::button(ui, "Clear channel", 88.0, false).clicked() {
                    let commands: Vec<_> = (0..STEPS)
                        .map(|step| EditCommand::SetStep {
                            track_id: self.session.editor.project().tracks[self.selected].id,
                            step,
                            velocity: 0,
                        })
                        .collect();
                    self.edit(&commands);
                }
            });
        });
    }

    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        section(ui, "Sampler", "SOURCE WAVEFORM  /  CHANNEL SETTINGS");
        let color = TRACK[self.selected];
        let cabinet = egui::Frame::new()
            .fill(Color32::from_rgb(35, 36, 38))
            .inner_margin(12)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&self.session.editor.project().tracks[self.selected].name)
                            .size(12.0)
                            .color(color),
                    );
                    ui.add_space(7.0);
                    let path = self
                        .session
                        .sample_path(self.selected)
                        .map(|p| {
                            p.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        })
                        .unwrap_or_else(|| "No sample".into());
                    ui.label(RichText::new(path).size(11.0).color(MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::button(ui, "Audition", 67.0, false).clicked() {
                            self.audition(self.selected);
                        }
                        if w::button(ui, "Unassign", 67.0, false).clicked() {
                            self.edit(&[EditCommand::SetSample {
                                track_id: self.session.editor.project().tracks[self.selected].id,
                                sample: None,
                            }]);
                        }
                    });
                });
                ui.add_space(5.0);
                let width = ui.available_width();
                ui.horizontal_top(|ui| {
                    ui.set_max_height(153.0);
                    ui.vertical(|ui| {
                        ui.set_width((width - 250.0).max(300.0));
                        let (rect, _) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 126.0), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(rect, 1, Color32::from_rgb(22, 25, 28));
                        p.rect_stroke(rect, 1, Stroke::new(1.0_f32, EDGE), StrokeKind::Inside);
                        for row in 1..4 {
                            let y = rect.top() + row as f32 / 4.0 * rect.height();
                            p.line_segment(
                                [pos2(rect.left(), y), pos2(rect.right(), y)],
                                Stroke::new(1.0_f32, PANEL),
                            );
                        }
                        for column in 1..8 {
                            let x = rect.left() + column as f32 / 8.0 * rect.width();
                            p.line_segment(
                                [pos2(x, rect.top()), pos2(x, rect.bottom())],
                                Stroke::new(1.0_f32, Color32::from_gray(39)),
                            );
                        }
                        let peaks = &self.peaks[self.selected];
                        let visible = ((peaks.len() as f32 / self.wave_zoom) as usize)
                            .max(1)
                            .min(peaks.len());
                        if peaks.is_empty() {
                            p.text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                if self.media_errors[self.selected].is_some() {
                                    "Sample unavailable · choose a replacement"
                                } else {
                                    "Drop an audio file or choose one in the library"
                                },
                                FontId::proportional(12.0),
                                MUTED,
                            );
                        } else {
                            for (i, &peak) in peaks.iter().take(visible).enumerate() {
                                let x = rect.left()
                                    + 6.0
                                    + i as f32 / visible as f32 * (rect.width() - 12.0);
                                let h = peak.min(1.0) * (rect.height() * 0.5 - 10.0);
                                p.line_segment(
                                    [pos2(x, rect.center().y - h), pos2(x, rect.center().y + h)],
                                    Stroke::new(1.2_f32, color),
                                );
                            }
                        }
                        p.text(
                            rect.min + vec2(5.0, 4.0),
                            Align2::LEFT_TOP,
                            "+1",
                            FontId::monospace(8.0),
                            MUTED,
                        );
                        p.text(
                            rect.left_center() + vec2(5.0, 0.0),
                            Align2::LEFT_CENTER,
                            "0",
                            FontId::monospace(8.0),
                            MUTED,
                        );
                        p.text(
                            rect.left_bottom() + vec2(5.0, -4.0),
                            Align2::LEFT_BOTTOM,
                            "−1",
                            FontId::monospace(8.0),
                            MUTED,
                        );
                        if let Some(sample) = &self.samples[self.selected] {
                            let duration = sample.samples.len() as f64
                                / sample.channels as f64
                                / sample.sample_rate as f64;
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} Hz  /  {}  /  {:.3} s",
                                        sample.sample_rate,
                                        if sample.channels == 1 {
                                            "Mono"
                                        } else {
                                            "Stereo"
                                        },
                                        duration
                                    ))
                                    .monospace()
                                    .size(9.5)
                                    .color(MUTED),
                                );
                                ui.add(
                                    egui::Slider::new(&mut self.wave_zoom, 1.0..=8.0)
                                        .text("Zoom")
                                        .show_value(false),
                                );
                            });
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.set_width(231.0);
                        ui.label(RichText::new("PERFORMANCE").size(9.0).color(MUTED));
                        ui.horizontal(|ui| {
                            let mut value = self.velocity as f32 / 127.0;
                            if w::knob(
                                ui,
                                &mut value,
                                "Step velocity",
                                &self.velocity.to_string(),
                                color,
                            ) {
                                self.velocity = (value * 127.0).round() as u8;
                            }
                            let display = if self.volume > 0.0001 {
                                format!("{:.1} dB", 20.0 * self.volume.log10())
                            } else {
                                "−∞ dB".into()
                            };
                            w::knob(ui, &mut self.volume, "Master output", &display, ACCENT);
                        });
                        ui.label(
                            RichText::new("8 voices / channel   •   One-shot playback")
                                .size(9.5)
                                .color(MUTED),
                        );
                        if self.velocity
                            != self
                                .session
                                .editor
                                .project()
                                .pattern
                                .get(self.selected, self.selected_step)
                                .unwrap_or(0)
                            && !ui.input(|i| i.pointer.primary_down())
                        {
                            self.edit(&[EditCommand::SetStep {
                                track_id: self.session.editor.project().tracks[self.selected].id,
                                step: self.selected_step,
                                velocity: self.velocity,
                            }]);
                        }
                        if self.volume != self.session.editor.project().master_volume
                            && !ui.input(|i| i.pointer.primary_down())
                        {
                            self.edit(&[EditCommand::SetMasterVolume {
                                volume: self.volume,
                            }]);
                        }
                    });
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Source").size(10.0).color(MUTED));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sample_input)
                            .hint_text("Path to WAV, FLAC or MP3")
                            .desired_width((width - 110.0).max(100.0))
                            .font(egui::TextStyle::Small),
                    );
                    if w::button(ui, "Load", 49.0, false).clicked() {
                        self.assign(&PathBuf::from(&self.sample_input));
                    }
                });
            });
        let cabinet_rect = cabinet.response.rect;
        for x in [cabinet_rect.left(), cabinet_rect.right() - 7.0] {
            let cheek = Rect::from_min_size(
                pos2(x, cabinet_rect.top()),
                vec2(7.0, cabinet_rect.height()),
            );
            w::wood(ui.painter(), cheek);
        }
    }
}
