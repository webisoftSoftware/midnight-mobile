use super::hardening::{isolated_runtime, open_test_session};
use super::*;

// The app exported checkpoints as a JSON string. Its bytes must restore, then re-export framed.
#[test]
fn app_json_string_checkpoint_restores_and_migrates_to_mmcp() {
    let _runtime = isolated_runtime();
    let original = open_test_session("app-checkpoint", None);
    let framed = export_wallet_checkpoint(original.id, original.generation).unwrap();
    let app_json = serde_json::to_string(&decode_checkpoint(&framed).unwrap()).unwrap();
    assert!(app_json.contains("\"walletFingerprint\":\"app-checkpoint\""));
    close_wallet_session(original.id, original.generation).unwrap();

    let restored = open_test_session("app-checkpoint", Some(app_json.into_bytes()));
    let exported = export_wallet_checkpoint(restored.id, restored.generation).unwrap();
    assert_eq!(&exported[..4], CHECKPOINT_MAGIC);
}
