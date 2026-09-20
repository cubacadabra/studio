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
    fn creates_the_shared_blank_starter_with_embedded_sdk() {
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
        assert_eq!(scene["nodes"].as_array().map(Vec::len), Some(1));
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
