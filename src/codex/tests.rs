use serde_json::json;

use super::*;
use std::fs;

#[test]
fn parses_chatgpt_account_status() {
    let account = parse_account_result(&json!({
        "account": {
            "type": "chatgpt",
            "email": " player@example.com ",
            "planType": "plus"
        },
        "requiresOpenaiAuth": true
    }))
    .unwrap()
    .unwrap();

    assert_eq!(account.email.as_deref(), Some("player@example.com"));
    assert_eq!(account.plan_type.as_deref(), Some("plus"));
}

#[test]
fn ignores_non_chatgpt_accounts() {
    let account = parse_account_result(&json!({
        "account": { "type": "apiKey" },
        "requiresOpenaiAuth": true
    }))
    .unwrap();

    assert_eq!(account, None);
}

#[test]
fn accepts_signed_out_account_status() {
    let account = parse_account_result(&json!({
        "account": null,
        "requiresOpenaiAuth": true
    }))
    .unwrap();

    assert_eq!(account, None);
}

#[test]
fn only_opens_secure_openai_auth_urls() {
    assert!(is_allowed_auth_url("https://chatgpt.com/auth/login"));
    assert!(is_allowed_auth_url("https://auth.openai.com/codex/device"));
    assert!(!is_allowed_auth_url("http://chatgpt.com/auth/login"));
    assert!(!is_allowed_auth_url(
        "https://chatgpt.com.example.com/login"
    ));
    assert!(!is_allowed_auth_url("file:///tmp/login"));
}

#[test]
fn maps_started_items_to_safe_human_readable_work_statuses() {
    for (item_type, expected) in [
        ("reasoning", CodexWorkStatus::Thinking),
        ("fileChange", CodexWorkStatus::Editing),
        ("commandExecution", CodexWorkStatus::Checking),
        ("agentMessage", CodexWorkStatus::Working),
    ] {
        let message = json!({
            "method": "item/started",
            "params": { "item": { "type": item_type } }
        });
        assert_eq!(work_status_for_started_item(&message), Some(expected));
    }

    assert_eq!(
        work_status_for_started_item(&json!({
            "method": "item/started",
            "params": { "item": { "type": "unknown" } }
        })),
        None
    );
}

#[test]
fn reads_user_facing_reasoning_summary_deltas() {
    let message = json!({
        "method": "item/reasoning/summaryTextDelta",
        "params": { "delta": "Reviewing the game layout." }
    });
    assert_eq!(
        reasoning_summary_delta(&message),
        Some("Reviewing the game layout.")
    );
}

#[test]
fn prefers_the_project_checkout_root_for_codex_bootstrap() {
    let root = std::env::temp_dir().join(format!(
        "cubacadabra-studio-codex-root-{}",
        std::process::id()
    ));
    let checkout = root.join("cubacadabra");
    let project = checkout.join("examples/game");
    let launch_directory = checkout.join("studio");
    let outside_project = root.join("external-game");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&launch_directory).unwrap();
    fs::create_dir_all(&outside_project).unwrap();
    fs::write(checkout.join(".codex-root"), "").unwrap();

    assert_eq!(
        codex_bootstrap_directory_from(&project, Some(&launch_directory)),
        Some(checkout.clone())
    );
    assert_eq!(
        codex_bootstrap_directory_from(&outside_project, Some(&launch_directory)),
        Some(checkout),
        "the launch checkout should be used for an external project"
    );
    let _ = fs::remove_dir_all(root);
}
