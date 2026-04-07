# Prompt 3 — add device discovery over Wi‑Fi / LAN

```text
Implement LAN/Wi‑Fi device discovery for sender and receiver modes.

Goal:
Devices running this app on the same network should be discoverable automatically.

Requirements:
- Use mDNS/zeroconf discovery
- Each device should publish:
  - device name
  - device ID
  - app version
  - capabilities (sender, receiver, Bluetooth-capable, local-output-capable, etc.)
  - current availability
- Maintain an in-memory device registry
- Handle device appearance, disappearance, duplicate announcements, and stale entries
- Design it so the GUI can subscribe to discovery state changes cleanly

Architecture constraints:
- discovery must be its own module/service
- UI reads discovery state from app state, not directly from network code
- keep protocol/versioning in mind for future compatibility

Deliverables:
- receiver service advertisement
- sender discovery browser
- app state integration
- clean logs for discovery events
- docs describing discovery flow

Validation:
- compile and run
- provide a small developer test path or debug screen/list showing discovered devices
```
