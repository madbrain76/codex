//! Tests shared command-line option inheritance.

use super::SharedCliOptions;

#[test]
fn inherits_worktree_from_root_without_clearing_subcommand_choice() {
    let mut options = SharedCliOptions::default();
    let root = SharedCliOptions {
        worktree: true,
        ..Default::default()
    };
    options.inherit_exec_root_options(&root);
    assert!(options.worktree);

    options.inherit_exec_root_options(&SharedCliOptions::default());
    assert!(options.worktree);
}

#[test]
fn applies_worktree_subcommand_override_without_clearing_root_choice() {
    let mut options = SharedCliOptions::default();
    options.apply_subcommand_overrides(SharedCliOptions {
        worktree: true,
        ..Default::default()
    });
    assert!(options.worktree);

    options.apply_subcommand_overrides(SharedCliOptions::default());
    assert!(options.worktree);
}

#[test]
fn inherits_wol_mac_from_root_and_lets_the_subcommand_win() {
    let mut options = SharedCliOptions::default();
    options.inherit_exec_root_options(&SharedCliOptions {
        wol_mac: Some("aa:bb:cc:dd:ee:ff".to_string()),
        ..Default::default()
    });
    assert_eq!(options.wol_mac.as_deref(), Some("aa:bb:cc:dd:ee:ff"));

    options.apply_subcommand_overrides(SharedCliOptions {
        wol_mac: Some("11:22:33:44:55:66".to_string()),
        ..Default::default()
    });
    assert_eq!(
        options.wol_mac.as_deref(),
        Some("11:22:33:44:55:66"),
        "the value given closest to the invocation should win"
    );

    options.inherit_exec_root_options(&SharedCliOptions::default());
    assert_eq!(
        options.wol_mac.as_deref(),
        Some("11:22:33:44:55:66"),
        "re-inheriting must not clear an already resolved address"
    );
}
