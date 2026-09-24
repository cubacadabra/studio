//! The live game instance used by Studio's About surface.
//!
//! The starter game is authored by `cubacadabra-project`, just as it is for
//! New Game. Studio only supplies a small presentation driver: it walks the
//! real player through the authored interaction zones and lets the normal
//! engine and Luau handle the puzzle.

use cubacadabra_client::{ClientSession, Engine};
use cubacadabra_project::starter_game_sources;
use cubacadabra_scene::parse_authoring_scene;
use serde_json::Value;

const STEP_SECONDS: f32 = 1.0 / 30.0;
const LINE_SETTLE_SECONDS: f32 = 1.35;
const LOOP_PAUSE_SECONDS: f32 = 1.75;

pub(crate) struct AboutPreview {
    manifest_source: String,
    script_source: String,
    client: ClientSession,
    route: Vec<usize>,
    route_index: usize,
    interaction_seen: bool,
    settle_remaining: f32,
    loop_pause_remaining: f32,
}

impl AboutPreview {
    pub(crate) fn new() -> Result<Self, String> {
        let sources = starter_game_sources("Cubacadabra", "cubacadabra-about");
        let mut manifest: Value = serde_json::from_str(&sources.manifest)
            .map_err(|error| format!("About starter manifest is invalid: {error}"))?;
        let scene = parse_authoring_scene(&sources.scene)
            .map_err(|error| format!("About starter scene is invalid: {error}"))?;
        scene
            .compile_into_manifest(&mut manifest)
            .map_err(|error| format!("About starter scene could not be compiled: {error}"))?;
        let manifest_source = serde_json::to_string(&manifest)
            .map_err(|error| format!("About starter manifest could not be encoded: {error}"))?;
        let script_source = sources.main_luau;
        let client = ClientSession::load(&manifest_source, &script_source)
            .map_err(|error| format!("About starter game could not be loaded: {error}"))?;
        let route = snake_route(client.engine());
        if route.len() != client.engine().interaction_count() {
            return Err(format!(
                "About starter route covers {} of {} interactions",
                route.len(),
                client.engine().interaction_count()
            ));
        }

        Ok(Self {
            manifest_source,
            script_source,
            client,
            route,
            route_index: 0,
            interaction_seen: false,
            settle_remaining: 0.0,
            loop_pause_remaining: 0.0,
        })
    }

    pub(crate) fn engine(&self) -> &Engine {
        self.client.engine()
    }

    pub(crate) fn step(&mut self) {
        if self.loop_pause_remaining > 0.0 {
            self.loop_pause_remaining -= STEP_SECONDS;
            self.drive(0.0, 0.0);
            if self.loop_pause_remaining <= 0.0 {
                self.restart();
            }
            return;
        }

        let Some(&interaction_index) = self.route.get(self.route_index) else {
            self.loop_pause_remaining = LOOP_PAUSE_SECONDS;
            self.drive(0.0, 0.0);
            return;
        };
        if self.interaction_seen {
            self.settle_remaining -= STEP_SECONDS;
            self.drive(0.0, 0.0);
            if self.settle_remaining <= 0.0 {
                self.route_index += 1;
                self.interaction_seen = false;
            }
            return;
        }

        let Some(target) = self.client.engine().interaction_position(interaction_index) else {
            self.route_index += 1;
            return;
        };
        if self.client.engine().interaction_inside(interaction_index) {
            self.interaction_seen = true;
            self.settle_remaining = LINE_SETTLE_SECONDS;
            self.drive(0.0, 0.0);
            return;
        }

        let snapshot = self.client.engine().snapshot();
        let Some(position) = snapshot.get(..3) else {
            return;
        };
        let dx = target[0] - position[0];
        let dz = target[2] - position[2];
        let distance = dx.hypot(dz);
        if distance <= 0.001 {
            self.drive(0.0, 0.0);
        } else {
            // At the About camera's fixed yaw, forward is -Z and strafe is +X.
            self.drive(
                (-dz / distance).clamp(-1.0, 1.0),
                (dx / distance).clamp(-1.0, 1.0),
            );
        }
    }

    fn drive(&mut self, forward: f32, strafe: f32) {
        self.client
            .engine_mut()
            .set_input_values(forward, strafe, false, false, false, 0.0, 0.0, 0.0);
        self.client.step(STEP_SECONDS);
        let _ = self.client.poll_actions();
    }

    fn restart(&mut self) {
        let Ok(client) = ClientSession::load(&self.manifest_source, &self.script_source) else {
            return;
        };
        self.client = client;
        self.route_index = 0;
        self.interaction_seen = false;
        self.settle_remaining = 0.0;
        self.loop_pause_remaining = 0.0;
    }
}

fn snake_route(engine: &Engine) -> Vec<usize> {
    let mut rows: Vec<Vec<(usize, [f32; 3])>> = Vec::new();
    for index in 0..engine.interaction_count() {
        let Some(position) = engine.interaction_position(index) else {
            continue;
        };
        let row = rows
            .iter_mut()
            .find(|row| (row[0].1[2] - position[2]).abs() < 0.01);
        if let Some(row) = row {
            row.push((index, position));
        } else {
            rows.push(vec![(index, position)]);
        }
    }
    rows.sort_by(|left, right| right[0].1[2].total_cmp(&left[0].1[2]));
    rows.into_iter()
        .enumerate()
        .flat_map(|(row_index, mut row)| {
            row.sort_by(|left, right| left.1[0].total_cmp(&right.1[0]));
            if row_index % 2 == 1 {
                row.reverse();
            }
            row.into_iter().map(|(index, _)| index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::AboutPreview;

    #[test]
    fn live_starter_preview_visits_every_authored_line() {
        let mut preview = AboutPreview::new().expect("starter preview should load");
        for _ in 0..1_800 {
            preview.step();
            if preview.route_index == preview.route.len() {
                break;
            }
        }
        assert_eq!(preview.route_index, 21);
    }
}
