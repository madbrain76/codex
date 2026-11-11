//! Wake-on-LAN support for self-hosted model servers that may be powered off.
//!
//! A local inference host that is asleep refuses connections outright, which is indistinguishable
//! from a wrong URL. Sending a magic packet gives the host a chance to boot before the retry policy
//! gives up, so the user does not have to get up and press the power button.

use std::fmt;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::time::Duration;

use tracing::warn;

/// Environment variable naming the network interface to wake when the model server is unreachable.
pub const WOL_MAC_ENV_VAR: &str = "CODEX_WOL_MAC";

/// Environment variable overriding the UDP destination that magic packets are sent to.
pub const WOL_BROADCAST_ENV_VAR: &str = "CODEX_WOL_BROADCAST";

const MAC_ADDRESS_OCTETS: usize = 6;
const MAGIC_PACKET_SYNC_OCTETS: usize = 6;
const MAGIC_PACKET_REPLICATED_MACS: usize = 16;
const DEFAULT_WOL_BROADCAST: &str = "255.255.255.255:9";
const MAGIC_PACKET_TIMEOUT: Duration = Duration::from_secs(2);

/// Six-octet Ethernet address of the network interface to wake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacAddress {
    octets: [u8; MAC_ADDRESS_OCTETS],
}

impl MacAddress {
    /// Parses `aa:bb:cc:dd:ee:ff`, `aa-bb-cc-dd-ee-ff`, and `aabbccddeeff`.
    pub fn parse(value: &str) -> Option<Self> {
        let digits: Vec<char> = value
            .trim()
            .chars()
            .filter(|character| *character != ':' && *character != '-')
            .collect();
        if digits.len() != MAC_ADDRESS_OCTETS * 2 {
            return None;
        }
        let mut octets = [0_u8; MAC_ADDRESS_OCTETS];
        for (index, octet) in octets.iter_mut().enumerate() {
            let high = digits[index * 2].to_digit(16)?;
            let low = digits[index * 2 + 1].to_digit(16)?;
            *octet = u8::try_from(high * 16 + low).ok()?;
        }
        Some(Self { octets })
    }

    /// Returns the address in network byte order.
    pub fn as_octets(&self) -> [u8; MAC_ADDRESS_OCTETS] {
        self.octets
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [first, second, third, fourth, fifth, sixth] = self.octets;
        write!(
            formatter,
            "{first:02x}:{second:02x}:{third:02x}:{fourth:02x}:{fifth:02x}:{sixth:02x}"
        )
    }
}

/// Builds the frame a sleeping NIC recognises as a wake-up call.
pub fn magic_packet(mac: &MacAddress) -> Vec<u8> {
    let mut packet = vec![0xff_u8; MAGIC_PACKET_SYNC_OCTETS];
    for _ in 0..MAGIC_PACKET_REPLICATED_MACS {
        packet.extend_from_slice(&mac.octets);
    }
    packet
}

/// Resolves the interface to wake, preferring the command-line flag over the environment.
///
/// An unusable value is reported through `startup_warnings` instead of failing the session: a
/// mistyped MAC must not stop Codex from talking to a server that happens to be awake.
pub fn resolve_wol_target(
    cli_value: Option<&str>,
    environment_value: Option<&str>,
    startup_warnings: &mut Vec<String>,
) -> Option<MacAddress> {
    let cli_value = cli_value.map(str::trim).filter(|value| !value.is_empty());
    let environment_value = environment_value
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let configured = match (cli_value, environment_value) {
        (Some(cli_value), _) => Some(cli_value),
        (None, Some(environment_value)) => Some(environment_value),
        (None, None) => None,
    }?;
    match MacAddress::parse(configured) {
        Some(mac) => Some(mac),
        None => {
            startup_warnings.push(format!(
                "ignoring Wake-on-LAN MAC address `{configured}`: expected six hexadecimal octets such as `aa:bb:cc:dd:ee:ff`"
            ));
            None
        }
    }
}

/// Resolves the UDP destination for magic packets, defaulting to the all-hosts broadcast address.
pub fn broadcast_destination() -> std::io::Result<SocketAddr> {
    let configured = std::env::var(WOL_BROADCAST_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_WOL_BROADCAST.to_string());
    configured
        .as_str()
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("`{WOL_BROADCAST_ENV_VAR}` resolved to no address: {configured}"),
            )
        })
}

/// Sends `mac`'s magic packet to `destination` over UDP.
pub async fn send_magic_packet(mac: &MacAddress, destination: SocketAddr) -> std::io::Result<()> {
    let socket = tokio::net::UdpSocket::bind(("0.0.0.0", 0)).await?;
    socket.set_broadcast(true)?;
    socket.send_to(&magic_packet(mac), destination).await?;
    Ok(())
}

/// Wakes the host owning `mac`, never propagating failures to the caller.
///
/// The model request that triggered the wake is already going to retry, so a broadcast that the
/// operating system refuses must not turn a retryable connection error into a terminal one.
pub(crate) async fn wake_host(mac: MacAddress) {
    match tokio::time::timeout(MAGIC_PACKET_TIMEOUT, wake_host_once(mac)).await {
        Ok(Ok(())) => {
            warn!(mac = %mac, "model server unreachable; sent Wake-on-LAN magic packet");
        }
        Ok(Err(err)) => {
            warn!(mac = %mac, error = %err, "failed to send Wake-on-LAN magic packet");
        }
        Err(_) => {
            warn!(mac = %mac, "timed out sending Wake-on-LAN magic packet");
        }
    }
}

async fn wake_host_once(mac: MacAddress) -> std::io::Result<()> {
    let destination = broadcast_destination()?;
    send_magic_packet(&mac, destination).await
}

#[cfg(test)]
#[path = "wol_tests.rs"]
mod tests;
