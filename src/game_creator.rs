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
    fn creates_the_starter_letter_wall_with_embedded_sdk() {
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
        assert_eq!(nodes.len(), 35);
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
        assert_eq!(first_cube["transform"]["position"][0], -11.25);
        assert_eq!(last_cube["transform"]["position"][0], 11.25);
        assert_eq!(first_cube["transform"]["position"][2], -4.0);
        assert_eq!(last_cube["transform"]["position"][2], -4.0);
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
