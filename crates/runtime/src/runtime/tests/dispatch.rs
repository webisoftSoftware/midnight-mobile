use super::hardening::{isolated_runtime, test_config};
use super::*;

#[test]
#[cfg(not(feature = "ledger-v9"))]
fn stagenet_sessions_get_v9_ids_and_other_networks_get_v8_ids() {
    use crate::dispatch::is_v9;
    let _runtime = isolated_runtime();
    let open = |network: &str| {
        let config = test_config("fp").replace("preview", network);
        open_wallet_session(config, vec![1; 32], vec![2; 32], vec![3; 32], None).unwrap()
    };
    let stagenet = open("stagenet");
    let preview = open("preview");
    assert!(is_v9(stagenet.id));
    assert!(!is_v9(preview.id));
    // Each handle is served by its own engine.
    for handle in [&stagenet, &preview] {
        assert!(get_wallet_snapshot(handle.id, handle.generation).is_ok());
        assert!(export_wallet_checkpoint(handle.id, handle.generation).is_ok());
        close_wallet_session(handle.id, handle.generation).unwrap();
        assert_eq!(
            get_wallet_snapshot(handle.id, handle.generation)
                .unwrap_err()
                .to_string(),
            "STALE_SESSION"
        );
    }
}

#[test]
#[cfg(feature = "ledger-v9")]
fn ledger_9_ids_start_at_id_base_and_accept_stagenet() {
    let _runtime = isolated_runtime();
    let config = test_config("fp").replace("preview", "stagenet");
    let handle = open_wallet_session(config, vec![1; 32], vec![2; 32], vec![3; 32], None).unwrap();
    assert!(handle.id > crate::ID_BASE);
}
