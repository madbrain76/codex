use super::MAC_ADDRESS_OCTETS;
use super::MAGIC_PACKET_REPLICATED_MACS;
use super::MAGIC_PACKET_SYNC_OCTETS;
use super::MacAddress;
use super::magic_packet;
use super::resolve_wol_target;
use super::send_magic_packet;
use std::net::SocketAddr;
use std::time::Duration;

const TARGET: &str = "aa:bb:cc:dd:ee:ff";

fn expected_octets() -> [u8; MAC_ADDRESS_OCTETS] {
    [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]
}

fn target_mac() -> MacAddress {
    MacAddress::parse(TARGET).expect("test MAC address should parse")
}

#[test]
fn mac_address_accepts_common_notations() {
    for notation in [
        "aa:bb:cc:dd:ee:ff",
        "AA:BB:CC:DD:EE:FF",
        "aa-bb-cc-dd-ee-ff",
        "  aabbccddeeff  ",
    ] {
        let mac = MacAddress::parse(notation)
            .unwrap_or_else(|| panic!("{notation} should parse as a MAC address"));
        assert_eq!(
            mac.as_octets(),
            expected_octets(),
            "unexpected octets for {notation}"
        );
        assert_eq!(
            mac.to_string(),
            TARGET,
            "{notation} should normalise to colon form"
        );
    }
}

#[test]
fn mac_address_rejects_malformed_values() {
    for value in [
        "",
        "aa:bb:cc:dd:ee",
        "aa:bb:cc:dd:ee:ff:00",
        "zz:bb:cc:dd:ee:ff",
        "aabbccddeef",
        "aa:bb:cc:dd:ee:ff:gg",
        "éé:bb:cc:dd:ee:ff",
    ] {
        assert!(
            MacAddress::parse(value).is_none(),
            "{value} should not parse as a MAC address"
        );
    }
}

#[test]
fn magic_packet_repeats_target_address_after_sync_octets() {
    let packet = magic_packet(&target_mac());
    let expected_length =
        MAGIC_PACKET_SYNC_OCTETS + MAC_ADDRESS_OCTETS * MAGIC_PACKET_REPLICATED_MACS;
    assert_eq!(packet.len(), expected_length);
    assert!(
        packet[..MAGIC_PACKET_SYNC_OCTETS]
            .iter()
            .all(|octet| *octet == 0xff)
    );
    for repetition in 0..MAGIC_PACKET_REPLICATED_MACS {
        let start = MAGIC_PACKET_SYNC_OCTETS + repetition * MAC_ADDRESS_OCTETS;
        assert_eq!(packet[start..start + MAC_ADDRESS_OCTETS], expected_octets());
    }
}

#[test]
fn wol_target_prefers_command_line_over_environment() {
    let mut startup_warnings = Vec::new();
    let mac = resolve_wol_target(
        Some("11:22:33:44:55:66"),
        Some(TARGET),
        &mut startup_warnings,
    )
    .expect("command-line MAC should be used");
    assert_eq!(mac.as_octets(), [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    assert!(startup_warnings.is_empty());
}

#[test]
fn wol_target_falls_back_to_environment_when_unset_on_command_line() {
    for blank in [None, Some(""), Some("   ")] {
        let mut startup_warnings = Vec::new();
        let mac = resolve_wol_target(blank, Some(TARGET), &mut startup_warnings)
            .expect("environment MAC should be used");
        assert_eq!(mac.as_octets(), expected_octets());
        assert!(startup_warnings.is_empty());
    }
}

#[test]
fn wol_target_is_disabled_without_a_configured_address() {
    for blank in [None, Some(""), Some("   ")] {
        let mut startup_warnings = Vec::new();
        assert!(resolve_wol_target(blank, blank, &mut startup_warnings).is_none());
        assert!(startup_warnings.is_empty());
    }
}

#[test]
fn unusable_wol_target_warns_instead_of_failing() {
    let mut startup_warnings = Vec::new();
    assert!(resolve_wol_target(Some("not-a-mac"), None, &mut startup_warnings).is_none());
    assert_eq!(startup_warnings.len(), 1);
    assert!(startup_warnings[0].contains("not-a-mac"));
}

#[tokio::test]
async fn magic_packet_reaches_a_bound_socket() {
    let listener = tokio::net::UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("should bind a loopback socket");
    let destination = listener
        .local_addr()
        .expect("loopback socket should have a local address");

    send_magic_packet(&target_mac(), destination)
        .await
        .expect("magic packet should be sent");

    let mut received = vec![0_u8; magic_packet(&target_mac()).len()];
    let length = tokio::time::timeout(Duration::from_secs(5), listener.recv(&mut received))
        .await
        .expect("magic packet should arrive")
        .expect("loopback socket should receive");
    assert_eq!(received[..length], magic_packet(&target_mac()));
}
