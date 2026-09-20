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

const STARTER_SOURCE_TEMPLATE: &str = "return {}\n";

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
        project.join("scene.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&starter_scene()).unwrap()
        ),
    )
    .map_err(|error| format!("could not write scene.json: {error}"))?;
    fs::write(
        project.join("src/main.luau"),
        starter_source(title, game_id),
    )
    .map_err(|error| format!("could not write src/main.luau: {error}"))?;
    Ok(())
}

fn starter_scene() -> Value {
    json!({
        "formatVersion": 1,
        "worldId": "starter-world",
        "nodes": [{
        "id": "world-starter-world",
        "name": "Starter World",
        "transform": { "position": [0, 0, 0], "rotation": [0, 0, 0], "scale": [1, 1, 1] },
        "components": {}
    }]
    })
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
        "spawn": [0, 1.2, 0],
        "showSpawnPad": false,
        "physics": {
            "gravity": 28,
            "jumpVelocity": 10.5,
            "groundCollision": true,
            "groundY": 0,
            "deathY": -20,
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
    });
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
                "checkpoints": [],
                "interactions": [],
            },
        },
    })
}

fn starter_source(_title: &str, _game_id: &str) -> String {
    STARTER_SOURCE_TEMPLATE.to_owned()
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
        assert_eq!(
            fs::read_to_string(result.project.join("src/main.luau")).unwrap(),
            "return {}\n"
        );
        assert_eq!(manifest["launch"]["destinationWorld"], "starter-world");
        assert!(manifest["worlds"]["starter-world"]["blocks"].is_null());
        let scene: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(result.project.join("scene.json")).unwrap())
                .unwrap();
        assert_eq!(
            scene["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|node| node["components"]["primitive"].is_object())
                .count(),
            0
        );
        assert_eq!(scene["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(scene["nodes"][0]["id"], "world-starter-world");
        assert_eq!(scene["nodes"][0]["components"], serde_json::json!({}));
        assert_eq!(
            manifest["worlds"]["starter-world"]["checkpoints"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert!(manifest["worlds"]["starter-world"]["signs"].is_null());
        assert_eq!(
            fs::read_to_string(result.project.join("src/main.luau")).unwrap(),
            "return {}\n"
        );
        assert!(create_game("The Wild West", &root).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
