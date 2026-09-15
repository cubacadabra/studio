//! Create a starter game project without depending on the Python developer tools.

use serde_json::{Value, json};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};
use unicode_normalization::UnicodeNormalization;

const DEFAULT_VERSION: &str = "0.3.0";

const SDK_FILES: &[(&str, &str)] = &[
    (
        "cycle.luau",
        include_str!("../../tools/src/cubacadabra/sdk/cycle.luau"),
    ),
    (
        "disclosure.luau",
        include_str!("../../tools/src/cubacadabra/sdk/disclosure.luau"),
    ),
    (
        "obby.luau",
        include_str!("../../tools/src/cubacadabra/sdk/obby.luau"),
    ),
    (
        "shared-state.luau",
        include_str!("../../tools/src/cubacadabra/sdk/shared-state.luau"),
    ),
    (
        "survival.luau",
        include_str!("../../tools/src/cubacadabra/sdk/survival.luau"),
    ),
];

const STARTER_SOURCE_TEMPLATE: &str = r#"local CubaObby = require("@cubacadabra/obby")

local Game = {}
local obby = CubaObby.create({})
local status = "REACH THE FINISH"

local function document()
    return {
        nodes = {
            {
                id = "course-title",
                kind = "text",
                text = __TITLE__,
                layout = { anchor = "topLeft", width = 420, height = 30, offset = { 26, 122 } },
                style = { color = __FOREGROUND__, fontSize = 20 },
            },
            {
                id = "course-status",
                kind = "text",
                text = status,
                layout = { anchor = "topLeft", width = 420, height = 26, offset = { 26, 158 } },
                style = { color = __ACCENT__, fontSize = 14 },
            },
            {
                id = "course-checkpoint",
                kind = "text",
                text = "CHECKPOINT  START",
                layout = { anchor = "topLeft", width = 320, height = 22, offset = { 26, 190 } },
                style = { color = __HIGHLIGHT__, fontSize = 12 },
            },
            {
                id = "course-help",
                kind = "text",
                text = "MOVE  •  JUMP  •  REACH THE FINISH",
                layout = { anchor = "bottom", width = "90%", maxWidth = 620, height = 24, offset = { 0, -24 } },
                style = { color = __FOREGROUND_MUTED__, fontSize = 11, textAlign = "center" },
            },
            {
                id = "player-joystick",
                kind = "joystick",
                action = "player.move",
                layout = { anchor = "bottomLeft", width = 120, height = 120, offset = { 20, -24 } },
                style = {
                    background = __JOYSTICK_BACKGROUND__,
                    borderColor = __JOYSTICK_BORDER__,
                    borderWidth = 2,
                    cornerRadius = 60,
                    accent = __ACCENT__,
                },
            },
            {
                id = "player-jump",
                kind = "button",
                text = "JUMP",
                action = "player.jump",
                layout = { anchor = "bottomRight", width = 86, height = 44, offset = { -22, -84 } },
                style = {
                    background = __CONTROL_BACKGROUND__,
                    borderColor = __ACCENT__,
                    borderWidth = 2,
                    cornerRadius = 17,
                    foreground = __FOREGROUND__,
                    accent = __ACCENT__,
                    textAlign = "center",
                    fontSize = 14,
                },
            },
            {
                id = "player-run",
                kind = "button",
                text = "RUN",
                action = "player.run",
                layout = { anchor = "bottomRight", width = 86, height = 44, offset = { -22, -30 } },
                style = {
                    background = __CONTROL_BACKGROUND__,
                    borderColor = __ACCENT__,
                    borderWidth = 2,
                    cornerRadius = 17,
                    foreground = __FOREGROUND__,
                    accent = __ACCENT__,
                    textAlign = "center",
                    fontSize = 14,
                },
            },
        },
    }
end

local function update_hud(api)
    local snapshot = obby:status()
    api.ui:set_text("course-status", status)
    api.ui:set_text("course-checkpoint", "CHECKPOINT  " .. string.upper(snapshot.checkpoint))
end

function Game.on_start(api)
    api.lobby:set_enabled(false)
    api.lobby:set_status("Jump across the platforms and reach the finish.")
    api.session:start(__ID__, { mode = "preview" })
    api.ui:set_document(document())
    update_hud(api)
end

function Game.on_player_event(api, event)
    obby:handle(api, event)
    if event.kind == "checkpoint" and event.id == "finish" then
        status = "COURSE COMPLETE  •  PLAY IT AGAIN"
    elseif event.kind == "checkpoint" then
        status = "CHECKPOINT SAVED  •  KEEP GOING"
    elseif event.kind == "death" then
        status = "YOU FELL  •  TRY THE NEXT JUMP"
    elseif event.kind == "respawn" then
        status = "BACK IN  •  YOU CAN DO THIS"
    end
    update_hud(api)
end

return Game
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GameCreateResult {
    pub(crate) game_id: String,
    pub(crate) display_name: String,
    pub(crate) project: PathBuf,
}

pub(crate) fn create_game(title: &str, parent: &Path) -> Result<GameCreateResult, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("title is required".to_owned());
    }
    let game_id = game_id(title)?;
    let parent = parent
        .canonicalize()
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                fs::create_dir_all(parent)?;
                parent.canonicalize()
            } else {
                Err(error)
            }
        })
        .map_err(|error| {
            format!(
                "could not use the parent directory {}: {error}",
                parent.display()
            )
        })?;
    let project = parent.join(&game_id);
    if project.exists() {
        return Err(format!(
            "game directory already exists: {}",
            project.display()
        ));
    }

    fs::create_dir(&project).map_err(|error| {
        format!(
            "could not create game directory {}: {error}",
            project.display()
        )
    })?;
    let result = write_project(&project, title, &game_id);
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&project);
        return Err(error);
    }
    Ok(GameCreateResult {
        game_id,
        display_name: title.to_owned(),
        project,
    })
}

pub(crate) fn game_id(title: &str) -> Result<String, String> {
    let normalized: String = title
        .nfkd()
        .filter(|character| character.is_ascii())
        .collect();
    let mut id = String::new();
    for character in normalized.to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character);
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_matches('-').to_owned();
    if id.is_empty() {
        return Err("title must contain at least one letter or number".to_owned());
    }
    if id.len() < 3 || id.len() > 64 {
        return Err("title must produce a game id between 3 and 64 characters".to_owned());
    }
    Ok(id)
}

fn write_project(project: &Path, title: &str, game_id: &str) -> Result<(), String> {
    fs::create_dir(project.join("src"))
        .map_err(|error| format!("could not create source directory: {error}"))?;
    fs::create_dir_all(project.join("assets/audio"))
        .map_err(|error| format!("could not create audio asset directory: {error}"))?;
    fs::create_dir_all(project.join("assets/images"))
        .map_err(|error| format!("could not create image asset directory: {error}"))?;
    write_starter_floor_texture(&project.join("assets/images/starter-floor.png"))?;

    let sdk_destination = project.join(".cubacadabra/sdk");
    fs::create_dir_all(&sdk_destination)
        .map_err(|error| format!("could not create SDK directory: {error}"))?;
    for (filename, source) in SDK_FILES {
        fs::write(sdk_destination.join(filename), source)
            .map_err(|error| format!("could not write SDK module {filename}: {error}"))?;
    }

    fs::write(
        project.join(".luaurc"),
        "{\n  \"aliases\": {\n    \"cubacadabra\": \".cubacadabra/sdk\"\n  }\n}\n",
    )
    .map_err(|error| format!("could not write .luaurc: {error}"))?;
    fs::write(
        project.join("manifest.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&manifest(title, game_id)).unwrap()
        ),
    )
    .map_err(|error| format!("could not write manifest.json: {error}"))?;
    fs::write(
        project.join("src/main.luau"),
        starter_source(title, game_id),
    )
    .map_err(|error| format!("could not write src/main.luau: {error}"))?;
    Ok(())
}

fn write_starter_floor_texture(path: &Path) -> Result<(), String> {
    let mut image = image::RgbaImage::new(128, 128);
    for y in 0..128 {
        for x in 0..128 {
            let tile = (x / 16 + y / 16) % 2;
            let color = if tile == 0 {
                image::Rgba([32, 41, 93, 255])
            } else {
                image::Rgba([44, 55, 116, 255])
            };
            image.put_pixel(x, y, color);
        }
    }
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| format!("could not encode starter floor texture: {error}"))?;
    fs::write(path, encoded.into_inner())
        .map_err(|error| format!("could not write starter floor texture: {error}"))
}

fn manifest(title: &str, game_id: &str) -> Value {
    let palette = json!({
        "sky": "#151A3F",
        "ground": "#20295D",
        "groundEdge": "#080D26",
        "grid": "#3B4C86",
        "signal": "#57E5D0",
        "hot": "#FF5B85",
        "coral": "#FF8A3D",
        "butter": "#F1E95B",
        "periwinkle": "#9F7BFF",
        "ink": "#0B102B",
        "paper": "#F7F5E9",
    });
    let world = json!({
        "groundSize": 70,
        "gridSize": 64,
        "gridDivisions": 32,
        "spawn": [0, 1.2, 18],
        "showSpawnPad": false,
        "physics": {
            "gravity": 28,
            "jumpVelocity": 10.5,
            "groundCollision": false,
            "groundY": -12,
            "deathY": -9,
            "respawnDelay": 0.65,
        },
        "groundMaterial": "floor",
        "materials": {
            "floor": {
                "image": "starter-floor",
                "tileU": 8,
                "tileV": 8,
            }
        },
        "clouds": [
            { "position": [-20, 19, -35], "scale": 0.9 },
            { "position": [24, 23, -48], "scale": 1.2 },
        ],
    });
    let course_blocks = json!([
        { "id": "start-platform", "position": [0, 0.5, 18], "size": [7, 1, 7], "color": "signal" },
        { "id": "first-jump", "position": [0, 0.6, 7], "size": [4.5, 1.2, 4.5], "color": "coral" },
        { "id": "left-route", "position": [-4.5, 1.8, -3], "size": [4.5, 1.2, 4.5], "color": "butter" },
        { "id": "right-route", "position": [4.5, 3.1, -13], "size": [4.5, 1.2, 4.5], "color": "periwinkle" },
        { "id": "checkpoint-platform", "position": [0, 4.5, -22], "size": [6, 1, 6], "color": "signal" },
        { "id": "finish-platform", "position": [0, 4.5, -32], "size": [8, 1, 6], "color": "hot" },
    ]);
    let course_checkpoints = json!([
        { "id": "start", "position": [0, 1.5, 18], "radius": 3.5 },
        { "id": "checkpoint", "position": [0, 5.5, -22], "radius": 2.8 },
        { "id": "finish", "position": [0, 5.5, -32], "radius": 3.6 },
    ]);
    json!({
        "id": game_id,
        "version": DEFAULT_VERSION,
        "sdkVersion": DEFAULT_VERSION,
        "package": { "formatVersion": 3, "entry": "game.luau" },
        "displayName": title,
        "lobby": false,
        "startWorld": "lobby",
        "launch": {
            "destinationWorld": "starter-world",
            "authoritative": true,
        },
        "scene": {
            "eyebrow": "cubacadabra",
            "title": title,
            "description": "A new Cubacadabra game.",
            "maxPlayers": 18,
        },
        "palette": palette.clone(),
        "assets": {
            "images": {
                "starter-floor": {
                    "path": "assets/images/starter-floor.png"
                }
            }
        },
        "avatars": {
            "player": {
                "skin": "#E8AE86",
                "shirt": "#57E5D0",
                "pants": "#4C3F91",
                "shoes": "#0B102B",
                "character": {
                    "version": 1,
                    "body": "cuba:person.v1",
                    "face": "determined",
                    "outfit": "cuba:everyday-hoodie.v1",
                    "equipment": {},
                    "colors": {
                        "primary": "#57E5D0",
                        "secondary": "#4C3F91",
                        "sole": "#0B102B",
                    },
                    "revision": 1,
                },
            },
            "npcs": [],
        },
        "world": world.clone(),
        "launchPads": [],
        "blocks": [],
        "worlds": {
            "starter-world": {
                "palette": palette,
                "world": world,
                "blocks": course_blocks,
                "checkpoints": course_checkpoints,
                "signs": [
                    {
                        "id": "welcome-sign",
                        "text": "EDIT THIS SIGN IN INSPECTOR",
                        "position": [0, 2.2, 14],
                        "maxWidth": 7.0,
                        "color": "paper"
                    }
                ],
                "interactions": [],
            },
        },
    })
}

fn starter_source(title: &str, game_id: &str) -> String {
    let literal = |value: &str| serde_json::to_string(value).unwrap();
    STARTER_SOURCE_TEMPLATE
        .replace("__JOYSTICK_BACKGROUND__", &literal("#0B102BC9"))
        .replace("__JOYSTICK_BORDER__", &literal("#57E5D055"))
        .replace("__CONTROL_BACKGROUND__", &literal("#0B102BF5"))
        .replace("__FOREGROUND__", &literal("#F7F5E9"))
        .replace("__FOREGROUND_MUTED__", &literal("#F7F5E9B8"))
        .replace("__ACCENT__", &literal("#57E5D0"))
        .replace("__HIGHLIGHT__", &literal("#F1E95B"))
        .replace("__TITLE__", &literal(title))
        .replace("__ID__", &literal(game_id))
}

#[cfg(test)]
mod tests {
    use super::{create_game, game_id};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn game_id_matches_cli_slug_rules() {
        assert_eq!(game_id("Café at Dawn").unwrap(), "cafe-at-dawn");
        assert_eq!(game_id("The Wild West").unwrap(), "the-wild-west");
        assert!(game_id("!!!").is_err());
        assert!(game_id("A").is_err());
    }

    #[test]
    fn creates_the_standard_starter_layout_without_external_tools() {
        let root = std::env::temp_dir().join(format!(
            "cubacadabra-studio-create-game-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let result = create_game("The Wild West", &root).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(result.project.join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["id"], "the-wild-west");
        assert_eq!(manifest["displayName"], "The Wild West");
        assert!(result.project.join("src/main.luau").is_file());
        assert!(result.project.join("assets/audio").is_dir());
        assert!(result.project.join("assets/images").is_dir());
        assert!(
            result
                .project
                .join("assets/images/starter-floor.png")
                .is_file()
        );
        assert!(
            result
                .project
                .join(".cubacadabra/sdk/shared-state.luau")
                .is_file()
        );
        assert!(
            fs::read_to_string(result.project.join("src/main.luau"))
                .unwrap()
                .contains("api.session:start(\"the-wild-west\"")
        );
        assert_eq!(manifest["launch"]["destinationWorld"], "starter-world");
        assert_eq!(
            manifest["worlds"]["starter-world"]["blocks"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
        assert_eq!(
            manifest["worlds"]["starter-world"]["checkpoints"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            manifest["worlds"]["starter-world"]["signs"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            fs::read_to_string(result.project.join("src/main.luau"))
                .unwrap()
                .contains("@cubacadabra/obby")
        );
        assert!(create_game("The Wild West", &root).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
