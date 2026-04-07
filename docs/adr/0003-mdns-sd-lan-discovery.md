# ADR 0003: mDNS-SD LAN Discovery

## Status

Accepted.

## Context

SynchroSonic senders and receivers need to discover compatible devices on the
same Wi-Fi/LAN without manual IP entry. Discovery must remain independent from
GTK widgets and should carry enough metadata for future protocol compatibility.

## Decision

Use the `mdns-sd` crate for mDNS/DNS-SD browsing and advertisement in the
`synchrosonic-discovery` crate. Advertise `_synchrosonic._tcp.local.` services
with TXT properties for device identity, app version, protocol version,
capabilities, and availability. Maintain an in-memory registry keyed by
`DeviceId`, and expose portable `DiscoveryEvent` and `DiscoverySnapshot` values
through `synchrosonic-core`.

## Consequences

- Discovery can run without coupling to GTK or Tokio.
- The GUI can read app-state snapshots and does not own mDNS sockets.
- Duplicate service announcements update a single registry entry by device ID.
- Stale entries are marked unavailable so the UI can represent transient network
  loss.
- Future protocol changes can branch on `protocol_version` without changing the
  service type immediately.

## Alternatives Considered

- Avahi D-Bus directly: Linux-native, but less portable and more D-Bus plumbing
  before the app needs it.
- Tokio-specific mDNS crates: workable later, but unnecessary coupling for this
  phase.
- Manual UDP broadcast: simpler to prototype, but less standard than zeroconf
  service discovery.

