use crate::state::{ChatMessage, UiCommand, UiState};
use eframe::egui;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

pub fn run(
    title: String,
    state: Arc<Mutex<UiState>>,
    commands: UnboundedSender<UiCommand>,
) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([560.0, 620.0])
            .with_always_on_top()
            .with_decorations(false)
            .with_transparent(false),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(move |_cc| {
            Ok(Box::new(OverlayApp { state, commands }))
        }),
    )
}

struct OverlayApp {
    state: Arc<Mutex<UiState>>,
    commands: UnboundedSender<UiCommand>,
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        let mut state = self.state.lock().unwrap();
        let mut pending: Vec<UiCommand> = Vec::new();

        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::NONE
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let drag = ui.interact(
                    rect,
                    ui.id().with("header-drag"),
                    egui::Sense::click_and_drag(),
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.horizontal(|ui| {
                    ui.heading("Copilot");
                    ui.separator();
                    let (me_color, them_color) = (
                        if state.me_voice_active() {
                            egui::Color32::from_rgb(46, 160, 67)
                        } else {
                            egui::Color32::GRAY
                        },
                        if state.them_voice_active() {
                            egui::Color32::from_rgb(66, 153, 225)
                        } else {
                            egui::Color32::GRAY
                        },
                    );
                    ui.label(egui::RichText::new("●").color(me_color).size(14.0))
                        .on_hover_text("You (microphone)");
                    ui.label(egui::RichText::new("Me").weak());
                    ui.label(egui::RichText::new("●").color(them_color).size(14.0))
                        .on_hover_text("Interlocutor (system audio)");
                    ui.label(egui::RichText::new("Them").weak());
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {}",
                            state.status,
                            if state.invisible {
                                "hidden"
                            } else {
                                "visible"
                            }
                        ))
                        .weak(),
                    );
                    if state.session_idle {
                        ui.separator();
                        ui.label(
                            egui::RichText::new("session idle")
                                .color(egui::Color32::from_rgb(219, 155, 32)),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    let chat_selected = state.view == "chat";
                    if ui
                        .selectable_label(chat_selected, "Chat")
                        .on_hover_text("Live conversation")
                        .clicked()
                    {
                        pending.push(UiCommand::SetView("chat".to_string()));
                    }
                    let sessions_selected = state.view != "chat";
                    if ui
                        .selectable_label(sessions_selected, format!("Sessions ({})", state.sessions.len()))
                        .on_hover_text("Saved sessions with notes")
                        .clicked()
                    {
                        pending.push(UiCommand::SetView("sessions".to_string()));
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new(
                            "drag to move · c=center · h=hide s=shot a=answer l=listen q=quit",
                        )
                        .weak()
                        .small(),
                    );
                });
            });

        egui::TopBottomPanel::bottom("composer").show(ctx, |ui| {
            egui::CollapsingHeader::new("Transcript").show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("transcript")
                    .max_height(110.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (role, text) in &state.transcript {
                            ui.label(egui::RichText::new(format!("{role}: {text}")).weak());
                        }
                    });
            });
            ui.separator();
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 170.0).max(120.0);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut state.input)
                        .hint_text("Ask the AI...")
                        .desired_width(width),
                );
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let clicked = ui.button("Send").clicked();
                if (clicked || (enter && edit.has_focus())) && !state.input.trim().is_empty() {
                    let text = state.input.trim().to_string();
                    state.input.clear();
                    pending.push(UiCommand::Chat(text));
                    edit.request_focus();
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Answer last").clicked() {
                    pending.push(UiCommand::AnswerLast);
                }
                if ui
                    .button(if state.screenshot_busy {
                        "Screenshot..."
                    } else {
                        "Screenshot"
                    })
                    .on_hover_text("Capture the screen and ask AI about it")
                    .clicked()
                    && !state.screenshot_busy
                {
                    pending.push(UiCommand::Screenshot);
                }
                if ui
                    .button("Recap")
                    .on_hover_text("Brief recap of the conversation")
                    .clicked()
                {
                    pending.push(UiCommand::Recap);
                }
                if ui
                    .button("Follow-ups")
                    .on_hover_text("Suggest follow-up questions")
                    .clicked()
                {
                    pending.push(UiCommand::FollowUps);
                }
                if ui
                    .button("Finish")
                    .on_hover_text("End session: save transcript and generate notes")
                    .clicked()
                {
                    pending.push(UiCommand::FinishSession);
                }
            });
            ui.horizontal(|ui| {
                let language_label = match state.language.as_str() {
                    "en" => "English",
                    "ru" => "Русский",
                    "uz" => "O'zbekcha",
                    _ => "Auto",
                };
                egui::ComboBox::from_id_salt("language")
                    .selected_text(language_label)
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for (code, label) in [
                            ("auto", "Auto"),
                            ("en", "English"),
                            ("ru", "Русский"),
                            ("uz", "O'zbekcha"),
                        ] {
                            if ui
                                .selectable_label(state.language == code, label)
                                .clicked()
                                && state.language != code
                            {
                                state.language = code.to_string();
                                pending.push(UiCommand::SetLanguage(code.to_string()));
                            }
                        }
                    });
                let listen_label = if state.listen_them {
                    "Listening: ON"
                } else {
                    "Listening: OFF"
                };
                let listen_button = egui::Button::new(
                    egui::RichText::new(listen_label).strong().color(if state.listen_them {
                        egui::Color32::from_rgb(46, 160, 67)
                    } else {
                        egui::Color32::from_rgb(200, 60, 60)
                    }),
                )
                .fill(if state.listen_them {
                    egui::Color32::from_rgb(28, 60, 34)
                } else {
                    egui::Color32::from_rgb(70, 28, 28)
                });
                if ui
                    .add(listen_button)
                    .on_hover_text("Toggle listening to the interlocutor")
                    .clicked()
                {
                    let value = !state.listen_them;
                    state.listen_them = value;
                    pending.push(UiCommand::SetListen(value));
                }
                if ui.checkbox(&mut state.auto_answer, "Auto-answer").changed() {
                    pending.push(UiCommand::SetAutoAnswer(state.auto_answer));
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match state.view.as_str() {
            "sessions" => {
                ui.heading("Saved sessions");
                ui.add_space(4.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if state.sessions.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "No saved sessions yet. Press Finish to save one.",
                            )
                            .weak(),
                        );
                    }
                    let mut open_id: Option<String> = None;
                    for session in &state.sessions {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&session.title).strong());
                            });
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} lines of transcript",
                                    session.transcript.len()
                                ))
                                .weak()
                                .small(),
                            );
                            if ui.button("Open").clicked() {
                                open_id = Some(session.id.clone());
                            }
                        });
                        ui.add_space(4.0);
                    }
                    if let Some(id) = open_id {
                        pending.push(UiCommand::OpenSession(id));
                    }
                });
            }
            "detail" => {
                let mut open_id: Option<(String, String)> = None;
                let mut close = false;
                let UiState {
                    open_session,
                    session_input,
                    ..
                } = &mut *state;
                if let Some(session) = open_session.as_mut() {
                    ui.horizontal(|ui| {
                        ui.heading(&session.title);
                        if ui.button("Back").clicked() {
                            close = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Copy summary").clicked() {
                            let summary = session.summary.clone();
                            ctx.copy_text(summary);
                        }
                        if ui.button("Copy transcript").clicked() {
                            let text = session
                                .transcript
                                .iter()
                                .map(|(role, line)| format!("{role}: {line}"))
                                .collect::<Vec<_>>()
                                .join("\n");
                            ctx.copy_text(text);
                        }
                    });
                    ui.separator();
                    ui.label(egui::RichText::new("Notes (AI summary)").strong());
                    egui::ScrollArea::vertical()
                        .id_salt("summary")
                        .max_height(160.0)
                        .show(ui, |ui| {
                            if session.summary.is_empty() {
                                ui.label(egui::RichText::new("No summary.").weak());
                            } else {
                                ui.label(&session.summary);
                            }
                        });
                    ui.separator();
                    ui.label(egui::RichText::new("Your notes").strong());
                    let notes_changed = ui
                        .add(
                            egui::TextEdit::multiline(&mut session.notes)
                                .hint_text("Write your notes here...")
                                .desired_rows(3),
                        )
                        .changed();
                    if notes_changed {
                        pending.push(UiCommand::SetNotes(session.notes.clone()));
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("Transcript").strong());
                    egui::ScrollArea::vertical()
                        .id_salt("session-transcript")
                        .show(ui, |ui| {
                            for (role, text) in &session.transcript {
                                ui.label(egui::RichText::new(format!("{role}: {text}")).weak());
                            }
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 110.0).max(120.0);
                        let edit = ui.add(
                            egui::TextEdit::singleline(session_input)
                                .hint_text("Ask about this session...")
                                .desired_width(width),
                        );
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let clicked = ui.button("Ask").clicked();
                        if (clicked || (enter && edit.has_focus()))
                            && !session_input.trim().is_empty()
                        {
                            let question = session_input.trim().to_string();
                            session_input.clear();
                            open_id = Some((session.id.clone(), question));
                        }
                    });
                }
                if close {
                    pending.push(UiCommand::CloseSessionDetail);
                }
                if let Some((id, question)) = open_id {
                    pending.push(UiCommand::AskAboutSession(id, question));
                }
            }
            _ => {
                egui::ScrollArea::vertical()
                    .id_salt("chat")
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if state.chat.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "Chat is empty. Type below or let the mic listen.",
                                )
                                .weak(),
                            );
                        }
                        for ChatMessage { role, text } in &state.chat {
                            ui.label(egui::RichText::new(role).strong());
                            ui.label(text);
                            ui.add_space(6.0);
                        }
                    });
            }
        });

        drop(state);
        for command in pending {
            let _ = self.commands.send(command);
        }
    }
}
