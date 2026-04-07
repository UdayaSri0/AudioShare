# LAN Device Discovery

SynchroSonic uses mDNS/DNS-SD through the `synchrosonic-discovery` crate so
sender and receiver instances can find each other on the same Wi-Fi/LAN without
manual IP entry.

## Service Type

The current service type is:

```text
_synchrosonic._tcp.local.
```

Each running app instance can advertise itself and browse the same service type.
The advertised TXT record includes:

- `app=synchrosonic`
- `device_id`
- `device_name`
- `app_version`
- `protocol_version`
- `sender`
- `receiver`
- `local_output`
- `bluetooth`
- `availability`

`protocol_version` starts at `1` and is part of every registry snapshot so future
versions can handle compatibility explicitly.

## Flow

`MdnsDiscoveryService::start` creates an mDNS daemon, registers the local device,
and starts browsing for SynchroSonic services. mDNS browse events are converted
into portable `DiscoveryEvent` values:

- `DeviceDiscovered`
- `DeviceUpdated`
- `DeviceRemoved`
- `DeviceExpired`

The discovery crate owns the network daemon and in-memory registry. The GTK app
polls discovery events, applies them to `AppState`, and renders the Devices page
from `AppState::discovered_devices`. Widgets do not call mDNS APIs directly.

## Registry Behavior

The registry is keyed by `DeviceId`, not service fullname, so duplicate
announcements for the same device update the existing entry. `ServiceRemoved`
events remove devices by service fullname. Stale entries are marked
`Unavailable` through `prune_stale` instead of silently disappearing, which gives
the UI a chance to show transient network loss.

## Developer Probe

Run a local discovery probe with:

```bash
RUST_LOG=synchrosonic_discovery=debug cargo run -p synchrosonic-discovery --example discovery_probe
```

The probe advertises a temporary local SynchroSonic service, browses for matching
services, prints discovery events, and prints the registry snapshot once per
second.

