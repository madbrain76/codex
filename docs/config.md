# Configuration

For basic configuration instructions, see [this documentation](https://developers.openai.com/codex/config-basic).

For advanced configuration instructions, see [this documentation](https://developers.openai.com/codex/config-advanced).

For a full configuration reference, see [this documentation](https://developers.openai.com/codex/config-reference).

## Lifecycle hooks

Admins can set top-level `allow_managed_hooks_only = true` in
`requirements.toml` to ignore user, project, and session hook configs while
still allowing managed hooks from requirements and managed config layers. This
setting is only supported in `requirements.toml`; putting it in `config.toml`
does not enable managed-hooks-only mode.

## Wake-on-LAN

A self-hosted model server that is powered off refuses connections the same way
a wrong URL does. Give Codex the MAC address of its network interface and it
sends a Wake-on-LAN magic packet whenever a model request cannot reach the
server, so the host can boot before the retry policy gives up:

```sh
codex --wol-mac aa:bb:cc:dd:ee:ff
CODEX_WOL_MAC=aa:bb:cc:dd:ee:ff codex
```

`--wol-mac` takes precedence over `CODEX_WOL_MAC`. Both accept
`aa:bb:cc:dd:ee:ff`, `aa-bb-cc-dd-ee-ff`, and `aabbccddeeff`. A value that
cannot be parsed is reported as a startup warning and otherwise ignored, so a
typo never blocks a session against a server that is already awake.

Packets are rate limited, starting at one every 30 seconds and doubling up to
one every 5 minutes while the server stays unreachable. Codex broadcasts to
`255.255.255.255:9`; set `CODEX_WOL_BROADCAST=192.168.1.255:9` when the server
is on another subnet. Waking the host also requires Wake-on-LAN to be enabled
on its interface, for example with `ethtool -s eth0 wol g`.
