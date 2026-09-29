use super::*;

#[derive(Default)]
pub(crate) struct PreviewBotInput {
    forward: f32,
    strafe: f32,
    sprint: bool,
    jump: bool,
    look_x: f32,
}

impl PreviewBotInput {
    pub(crate) fn apply(self, client: &mut ClientSession) {
        client.engine_mut().set_input_values(
            self.forward,
            self.strafe,
            self.sprint,
            self.jump,
            false,
            self.look_x,
            0.0,
            0.0,
        );
    }
}

pub(crate) struct PreviewAutopilot {
    random_state: u64,
    decision_in: f32,
    jump_in: f32,
    probe_in: f32,
    probe_position: Option<[f32; 2]>,
    forward: f32,
    strafe: f32,
    sprint: bool,
    look_rate: f32,
}

impl PreviewAutopilot {
    pub(crate) fn new(index: usize) -> Self {
        Self {
            random_state: 0x9e37_79b9_7f4a_7c15_u64
                ^ (index as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9),
            decision_in: 0.0,
            jump_in: 1.5 + index as f32 * 0.3,
            probe_in: 1.0,
            probe_position: None,
            forward: 0.0,
            strafe: 0.0,
            sprint: false,
            look_rate: 0.0,
        }
    }

    pub(crate) fn reset_after_control(&mut self) {
        self.decision_in = 0.0;
        self.jump_in = 1.5;
        self.probe_in = 1.0;
        self.probe_position = None;
        self.forward = 0.0;
        self.strafe = 0.0;
        self.sprint = false;
        self.look_rate = 0.0;
    }

    pub(crate) fn moving(&self) -> bool {
        self.forward.abs() + self.strafe.abs() > 0.01
    }

    pub(crate) fn sprinting(&self) -> bool {
        self.moving() && self.sprint
    }

    pub(crate) fn next_input(&mut self, delta: f32, snapshot: &[f32]) -> PreviewBotInput {
        let delta = delta.clamp(0.0, 0.1);
        self.decision_in -= delta;
        self.jump_in -= delta;
        self.probe_in -= delta;

        let mut blocked = false;
        if let [x, _, z, ..] = snapshot {
            let position = [*x, *z];
            if self.probe_in <= 0.0 {
                if let Some(previous) = self.probe_position {
                    let distance = (position[0] - previous[0]).hypot(position[1] - previous[1]);
                    blocked = self.moving() && distance < 0.25;
                }
                self.probe_position = Some(position);
                self.probe_in = 1.0;
            } else if self.probe_position.is_none() {
                self.probe_position = Some(position);
            }
        }

        if self.decision_in <= 0.0 || blocked {
            if !blocked && self.random_unit() < 0.12 {
                self.forward = 0.0;
                self.strafe = 0.0;
                self.sprint = false;
                self.look_rate = self.random_between(-25.0, 25.0);
                self.decision_in = self.random_between(0.4, 0.9);
            } else {
                self.forward = self.random_between(0.65, 1.0);
                self.strafe = self.random_between(-0.5, 0.5);
                self.sprint = self.random_unit() < 0.16;
                self.look_rate = if blocked {
                    self.random_between(90.0, 145.0)
                        * if self.random_unit() < 0.5 { -1.0 } else { 1.0 }
                } else {
                    self.random_between(-65.0, 65.0)
                };
                self.decision_in = self.random_between(1.2, 2.8);
            }
        }

        let jump = self.moving() && (blocked || self.jump_in <= 0.0);
        if jump {
            self.jump_in = self.random_between(2.8, 5.8);
        }
        PreviewBotInput {
            forward: self.forward,
            strafe: self.strafe,
            sprint: self.sprinting(),
            jump,
            look_x: self.look_rate * delta,
        }
    }

    fn random_unit(&mut self) -> f32 {
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 7;
        self.random_state ^= self.random_state << 17;
        (self.random_state >> 40) as f32 / (1_u32 << 24) as f32
    }

    fn random_between(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.random_unit()
    }
}

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
            self.preview_player_ids.clear();
            if self.preview_namespace.take().is_some() {
                self.client.engine_mut().set_username_value("PLAYER");
                self.network.set_game_id(self.client.game_id());
                self.client.request_transport();
            }
            return;
        }
        if self.preview_namespace.is_none() {
            if let Some(name) = self
                .shell
                .as_ref()
                .and_then(|shell| shell.play_player_name(0))
            {
                self.client.engine_mut().set_username_value(name);
            }
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
        for index in 1..count {
            let client = match ClientSession::load(&self.manifest_source, &self.script_source) {
                Ok(mut client) => {
                    client
                        .engine_mut()
                        .set_studio_movement_joystick_visible(false);
                    if let Some(name) = self
                        .shell
                        .as_ref()
                        .and_then(|shell| shell.play_player_name(index))
                    {
                        client.engine_mut().set_username_value(name);
                    }
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
            self.preview_peers.push(PreviewPeer {
                client,
                network,
                autopilot: PreviewAutopilot::new(self.preview_peers.len() + 1),
            });
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
        for index in 0..self.preview_peers.len() {
            while let Some(event) = self.preview_peers[index].network.try_recv() {
                match event {
                    BackendEvent::Connected => {
                        self.preview_peers[index].client.transport_connected()
                    }
                    BackendEvent::Disconnected => {
                        self.preview_peers[index].client.transport_disconnected()
                    }
                    BackendEvent::Message(source) => {
                        self.receive_preview_message(index + 1, &source);
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn receive_preview_message(&mut self, index: usize, source: &str) {
        if self.preview_namespace.is_none() {
            let _ = self.client.receive_text(source);
            return;
        }
        // Preview sockets are guests, so the backend assigns its own labels.
        // Keep those IDs for networking while giving each local client the
        // same display name Studio shows on that player's viewport.
        if source.contains("session_identity")
            && let Ok(message) = serde_json::from_str::<Value>(source)
            && message.get("type").and_then(Value::as_str) == Some("session_identity")
            && let Some(id) = message.get("id").and_then(Value::as_str)
        {
            self.preview_player_ids.insert(id.to_owned(), index);
            if let Some(name) = self
                .shell
                .as_ref()
                .and_then(|shell| shell.play_player_name(index))
            {
                let update = serde_json::json!({"type": "player_name", "id": id, "username": name})
                    .to_string();
                let _ = self.client.receive_text(&update);
                for peer in &mut self.preview_peers {
                    let _ = peer.client.receive_text(&update);
                }
            }
        }
        let names = self
            .shell
            .as_ref()
            .map_or(&[][..], StudioShell::play_player_names);
        let named = preview_named_message(source, &self.preview_player_ids, names);
        if index == 0 {
            let _ = self.client.receive_text(&named);
        } else if let Some(peer) = self.preview_peers.get_mut(index - 1) {
            let _ = peer.client.receive_text(&named);
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

fn preview_named_message(
    source: &str,
    player_ids: &BTreeMap<String, usize>,
    names: &[String],
) -> String {
    if !source.contains("player_join") && !source.contains("player_name") {
        return source.to_owned();
    }
    let Ok(mut message) = serde_json::from_str::<Value>(source) else {
        return source.to_owned();
    };
    if !matches!(
        message.get("type").and_then(Value::as_str),
        Some("player_join" | "player_name")
    ) {
        return source.to_owned();
    }
    let Some(name) = message
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| player_ids.get(id))
        .and_then(|index| names.get(*index))
    else {
        return source.to_owned();
    };
    message["username"] = Value::String(name.to_owned());
    message.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_names_follow_player_identity_in_remote_labels() {
        let names = ["Maya".to_owned(), "Theo".to_owned()];
        let ids = BTreeMap::from([("socket-a".to_owned(), 0), ("socket-b".to_owned(), 1)]);
        let join = r#"{"type":"player_join","id":"socket-b","username":"Web Player 1234"}"#;
        let renamed = preview_named_message(join, &ids, &names);
        let parsed: Value = serde_json::from_str(&renamed).unwrap();
        assert_eq!(parsed["username"], "Theo");
        assert_eq!(parsed["id"], "socket-b");

        let name_update = r#"{"type":"player_name","id":"socket-a","username":"Player 1"}"#;
        let renamed = preview_named_message(name_update, &ids, &names);
        let parsed: Value = serde_json::from_str(&renamed).unwrap();
        assert_eq!(parsed["username"], "Maya");
        assert_eq!(preview_named_message(join, &BTreeMap::new(), &names), join);
    }

    #[test]
    fn preview_autopilot_moves_and_jumps_through_player_input() {
        let mut client =
            ClientSession::load(STANDALONE_PREVIEW_MANIFEST, STANDALONE_PREVIEW_SCRIPT)
                .expect("preview client");
        let start = client.engine().snapshot()[..3].to_vec();
        let mut autopilot = PreviewAutopilot::new(1);
        let mut farthest = 0.0_f32;
        let mut highest = 0.0_f32;
        for _ in 0..360 {
            let input = autopilot.next_input(1.0 / 60.0, client.engine().snapshot());
            input.apply(&mut client);
            client.step(1.0 / 60.0);
            let position = client.engine().snapshot();
            farthest = farthest.max((position[0] - start[0]).hypot(position[2] - start[2]));
            highest = highest.max(position[1] - start[1]);
        }
        assert!(farthest > 1.0, "uncontrolled player should roam");
        assert!(highest > 0.2, "uncontrolled player should jump");
    }
}
