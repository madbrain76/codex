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

## Provider outage retries

A self-hosted server that is booting or reloading a model answers requests with
`503 Service Unavailable` for as long as it takes to load the weights, which is
far longer than a fixed retry count can cover. Codex therefore keeps a sampling
turn alive for **10 minutes by default**, polling every 15 seconds, and only
surfaces the error once that window is consumed:

```toml
[model_providers.mine]
base_url = "http://hal:8110/v1"
wire_api = "responses"
stream_retry_deadline_ms = 600_000 # default; 0 disables the window
stream_retry_interval_ms = 15_000  # default; clamped to 1s..5m
```

The window is spent only on failures that mean "not right now": `5xx` responses
including `503 model is still loading`, aborted or idle streams, and connection
failures. A request rejected for its contents — an invalid parameter, an
unsupported option, a content filter — fails immediately and never spends the
window. Setting `stream_retry_deadline_ms = 0` restores the count-based
`stream_max_retries` policy on its own.
