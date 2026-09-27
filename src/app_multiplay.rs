use super::*;

impl StudioApp {
    pub(crate) fn active_client_mut(&mut self) -> &mut ClientSession {
        let selected = self
            .shell
            .as_ref()
            .map_or(0, StudioShell::controlled_player);
        if let Some(peer) = selected
            .checked_sub(1)
            .and_then(|index| self.preview_peers.get_mut(index))
        {
            &mut peer.client
        } else {
            &mut self.client
        }
    }

    pub(crate) fn sync_preview_players(&mut self) {
        let count = self
            .shell
            .as_ref()
            .filter(|shell| !shell.is_project_loading())
            .map_or(1, StudioShell::play_player_count);
        if count == 1 {
            self.preview_peers.clear();
            if self.preview_namespace.take().is_some() {
                self.network.set_game_id(self.client.game_id());
                self.client.request_transport();
            }
            return;
        }
        if self.preview_namespace.is_none() {
            let mut random = [0_u8; 12];
            if let Err(error) = getrandom::fill(&mut random) {
                self.preview_start_failed(format!(
                    "Could not create a private play session: {error}"
                ));
                return;
            }
            let namespace = format!(
                "studio-preview-{}",
                random
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            );
            self.network.set_game_id(&namespace);
            self.client.request_transport();
            self.preview_namespace = Some(namespace);
        }
        if self.preview_peers.len() + 1 == count {
            return;
        }
        self.preview_peers.clear();
        let namespace = self
            .preview_namespace
            .as_deref()
            .expect("preview namespace");
        for _ in 1..count {
            let client = match ClientSession::load(&self.manifest_source, &self.script_source) {
                Ok(mut client) => {
                    client
                        .engine_mut()
                        .set_studio_movement_joystick_visible(false);
                    client
                }
                Err(error) => {
                    self.preview_start_failed(format!(
                        "Could not start the other players: {error}"
                    ));
                    return;
                }
            };
            let network = match BackendClient::new(namespace) {
                Ok(network) => network,
                Err(error) => {
                    self.preview_start_failed(format!(
                        "Could not connect the other players: {error}"
                    ));
                    return;
                }
            };
            self.preview_peers.push(PreviewPeer { client, network });
        }
    }

    fn preview_start_failed(&mut self, message: String) {
        self.preview_peers.clear();
        if let Some(shell) = &mut self.shell {
            shell.set_playing(false);
            shell.set_notice(message);
        }
    }

    pub(crate) fn drain_preview_peer_events(&mut self) {
        for peer in &mut self.preview_peers {
            while let Some(event) = peer.network.try_recv() {
                match event {
                    BackendEvent::Connected => peer.client.transport_connected(),
                    BackendEvent::Disconnected => peer.client.transport_disconnected(),
                    BackendEvent::Message(source) => {
                        let _ = peer.client.receive_text(&source);
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn dispatch_preview_peer_actions(&mut self) {
        for peer in &mut self.preview_peers {
            for action in peer.client.poll_actions() {
                match action {
                    ClientAction::SetWorld(world_id) => peer.network.set_world(world_id),
                    ClientAction::SendText(source) => peer.network.send(source),
                }
            }
        }
    }
}
