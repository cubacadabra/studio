//! Studio adapter for the shared project generator owned by creator tools.

use cubacadabra_project::CreateResult;
use std::path::Path;

pub(crate) fn create_game(title: &str, parent: &Path) -> Result<CreateResult, String> {
    // Installed Studio projects include the SDK so creation remains self-contained.
    cubacadabra_project::create_game(title, parent, true)
}

#[cfg(test)]
mod tests {
    use super::create_game;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn creates_the_starter_letter_puzzle_with_embedded_sdk() {
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
        let scene: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(result.project.join("scene.json")).unwrap())
                .unwrap();

        assert_eq!(result.game_id, "the-wild-west");
        assert_eq!(manifest["displayName"], "The Wild West");
        let nodes = scene["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 67);
        assert_eq!(
            nodes
                .iter()
                .filter(|node| node["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("Letter Cube")))
                .count(),
            11
        );
        let first_cube = nodes
            .iter()
            .find(|node| node["name"] == "Letter Cube 1")
            .unwrap();
        let last_cube = nodes
            .iter()
            .find(|node| node["name"] == "Letter Cube 11")
            .unwrap();
        assert_eq!(first_cube["transform"]["position"][0], -10.0);
        assert_eq!(last_cube["transform"]["position"][0], 10.0);
        assert_eq!(first_cube["transform"]["position"][2], -4.0);
        assert_eq!(last_cube["transform"]["position"][2], -4.0);
        let border_nodes = nodes
            .iter()
            .filter(|node| {
                node["name"]
                    .as_str()
                    .is_some_and(|name| name.contains("Border"))
            })
            .collect::<Vec<_>>();
        assert_eq!(border_nodes.len(), 34);
        assert_eq!(
            border_nodes
                .iter()
                .filter(|node| node["name"] == "Letter 1 Border Left")
                .count(),
            1
        );
        assert_eq!(
            border_nodes
                .iter()
                .filter(|node| node["name"]
                    .as_str()
                    .is_some_and(|name| name.ends_with("Border Right")))
                .count(),
            11
        );
        assert!(border_nodes.iter().all(|node| {
            node["components"]["primitive"]["color"] == "#0B102B"
                && node["components"]["primitive"]["outline"] == false
        }));
        assert!(
            nodes
                .iter()
                .filter(|node| node["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("Letter Cube")))
                .all(|node| node["components"]["primitive"]["outline"] == false)
        );
        let cube_xs = nodes
            .iter()
            .filter(|node| {
                node["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("Letter Cube"))
            })
            .map(|node| node["transform"]["position"][0].as_f64().unwrap())
            .collect::<Vec<_>>();
        assert!(
            cube_xs
                .windows(2)
                .all(|pair| (pair[1] - pair[0] - 2.0).abs() < 0.0001)
        );
        let loose_lines = nodes
            .iter()
            .filter(|node| {
                node["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("Loose Line"))
            })
            .collect::<Vec<_>>();
        assert_eq!(loose_lines.len(), 21);
        assert!(loose_lines.iter().all(|node| {
            node["components"]["primitive"].is_null()
                && node["components"]["interaction"]["kind"] == "pickup"
                && node["components"]["interaction"]["visual"]
                    .as_str()
                    .is_some_and(|visual| visual.starts_with("letter-line-"))
        }));
        assert_eq!(loose_lines[0]["transform"]["position"][1], 0.0);
        assert_eq!(loose_lines[0]["transform"]["position"][2], 13.0);
        assert_eq!(manifest["effects"]["version"], 1);
        assert_eq!(
            manifest["effects"]["templates"].as_object().unwrap().len(),
            22
        );
        assert!(
            manifest["effects"]["templates"]["letter-line-1"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["animation"]["travelTo"].is_array())
        );
        let source = fs::read_to_string(result.project.join("src/main.luau")).unwrap();
        assert!(source.contains("function Game.on_interaction"));
        assert!(source.contains("All 21 lines are back on the cubes"));
        assert!(
            result
                .project
                .join(".cubacadabra/sdk/shared-state.luau")
                .is_file()
        );
        assert!(create_game("The Wild West", &root).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
