use super::*;
impl StudioApp {
    pub(crate) fn drain_backend_events(&mut self) {
        while let Some(event) = self.network.try_recv() {
            match event {
                BackendEvent::Connected => self.client.transport_connected(),
                BackendEvent::Disconnected => self.client.transport_disconnected(),
                BackendEvent::Message(source) => {
                    let _ = self.client.receive_text(&source);
                }
                BackendEvent::MorphCatalog(source) => self.install_published_morphs(&source),
                BackendEvent::MorphCatalogError(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_catalog_error(message);
                        shell.set_notice("Morph catalog unavailable".into());
                    }
                }
                BackendEvent::MorphPacks { request_id, packs } => {
                    if let Err(message) = self.finish_morph_change(request_id, packs) {
                        if let Some(shell) = &mut self.shell {
                            shell.set_notice(message);
                        }
                    }
                }
                BackendEvent::MorphPacksError {
                    request_id,
                    message,
                } => {
                    if self
                        .pending_morph
                        .as_ref()
                        .is_some_and(|(serial, _)| *serial == request_id)
                    {
                        self.pending_morph = None;
                        if let Some(shell) = &mut self.shell {
                            shell.set_morph_loading(false);
                            shell.set_notice(format!("Appearance unchanged: {message}"));
                        }
                    }
                }
                BackendEvent::MorphThumbnail { url, bytes } => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_morph_thumbnail(url, &bytes);
                    }
                }
                BackendEvent::AuthStarted => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_notice(
                            "Finish signing in in your browser. Studio will continue automatically."
                                .to_owned(),
                        );
                    }
                }
                BackendEvent::AuthCompleted { user } => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_auth_completed(user);
                    }
                }
                BackendEvent::AuthError(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_auth_error(message);
                    }
                }
            }
        }
    }

    pub(crate) fn drain_codex_events(&mut self) {
        // Leave a few frames for the activity panel to render streamed agent
        // progress instead of consuming a fast response all at once.
        const MAX_CODEX_EVENTS_PER_FRAME: usize = 12;
        let mut processed = 0;
        while processed < MAX_CODEX_EVENTS_PER_FRAME {
            let Some(event) = self.codex.try_recv() else {
                break;
            };
            processed += 1;
            match event {
                CodexEvent::AccountStatus(account) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_account(account);
                    }
                }
                CodexEvent::BrowserOpened => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_browser_opened();
                    }
                }
                CodexEvent::LoginCompleted(account) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_connected(account);
                    }
                }
                CodexEvent::ChatReady => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_ready();
                    }
                }
                CodexEvent::WorkStatus(status) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_work_status(status);
                    }
                }
                CodexEvent::AssistantDelta(delta) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_delta(delta);
                    }
                }
                CodexEvent::AssistantMessage(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_message(message);
                    }
                }
                CodexEvent::ChatTurnCompleted => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_completed();
                        shell.set_notice("Change received — rebuilding preview…".to_owned());
                    }
                    let changes = self.capture_codex_changes();
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_changes(
                            changes
                                .iter()
                                .map(|change| change.relative_path.display().to_string())
                                .collect(),
                        );
                    }
                    if self
                        .shell
                        .as_ref()
                        .is_some_and(StudioShell::project_is_dirty)
                    {
                        self.save_project_source();
                    }
                    self.refresh_authored_manifest_from_disk();
                    self.start_codex_project_reload();
                }
                CodexEvent::ChatTurnCancelled => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_cancelled();
                        shell.set_notice(
                            "Codex stopped. The preview was not rebuilt; review or undo the changes."
                                .to_owned(),
                        );
                    }
                    let changes = self.capture_codex_changes();
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_changes(
                            changes
                                .iter()
                                .map(|change| change.relative_path.display().to_string())
                                .collect(),
                        );
                    }
                }
                CodexEvent::ChatError(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_error(message);
                    }
                }
                CodexEvent::Error(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_error(message);
                    }
                }
                CodexEvent::Unavailable(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_unavailable(message);
                    }
                }
            }
        }
    }

    pub(crate) fn dispatch_client_actions(&mut self) {
        for action in self.client.poll_actions() {
            if self.standalone_preview {
                continue;
            }
            match action {
                ClientAction::SetWorld(world_id) => self.network.set_world(world_id),
                ClientAction::SendText(source) => self.network.send(source),
            }
        }
    }
}
