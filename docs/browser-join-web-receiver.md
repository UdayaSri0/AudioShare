# Web Receiver / Guest Join Design

This document defines the honest path toward no-install guest joining from a
browser without replacing the current native Linux receiver.

## Decision

Chosen architecture: `WebRTC + browser AudioContext/AudioWorklet`.

Why this path:

- browsers do not speak the current native SynchroSonic LAN transport
- WebRTC gives us browser-compatible media transport, NAT-friendly session
  setup, congestion handling, and a realistic latency envelope
- AudioWorklet is the right browser-side playback boundary for stable audio
  rendering and drift handling
- this keeps the existing native receiver intact while adding a separate guest
  output capability

Acceptable fallback for an early LAN beta:

- `WebSocket PCM + AudioWorklet`

That fallback is easier to prototype on a trusted LAN, but it is not the long
term transport because it has weaker jitter handling, less efficient transport,
and fewer browser-native media controls.

## Non-Goals For This Phase

- do not remove or weaken native receiver mode
- do not claim browser playback works before a real browser transport exists
- do not bolt browser clients onto the native TCP transport without a protocol
  bridge

## Current Native Reality

Today the host app provides:

- native Linux capture and playback
- native receiver runtime
- mDNS discovery for native peers
- custom LAN transport for sender-to-native-receiver sessions

A browser guest is therefore a new receiver class, not a skin over the current
native receiver.

## Target Architecture

### Host Side

The desktop app remains the session authority.

Responsibilities:

- generate short-lived join tokens
- host a local HTTP surface for join landing and signaling boundaries
- advertise or share a join URL to guests
- terminate browser guest sessions
- expose browser-session diagnostics alongside native diagnostics

### Browser Side

The browser guest will eventually:

- open the join URL
- redeem a short-lived join token
- complete signaling with the host
- receive browser-compatible media over WebRTC
- render audio through an `AudioContext` and `AudioWorklet`
- expose a minimal diagnostics export for guest-side troubleshooting

## Signaling Path

Preferred initial signaling path:

1. host generates a short-lived token
2. host exposes a local HTTP join endpoint
3. guest opens the join URL
4. guest fetches session metadata from the host signaling boundary
5. host and guest exchange WebRTC offer/answer and ICE candidates through the
   same local signaling service
6. host upgrades the guest to an active browser receiver session only after the
   WebRTC transport is ready

The signaling service should stay separate from the native transport listener so
readiness, diagnostics, and error handling remain clear.

## Authentication And Join Tokens

The browser path should use explicit join tokens.

Requirements:

- short TTL, suitable for same-room or same-LAN joins
- single-use or small bounded reuse depending on session policy
- bound to the host instance that generated them
- visible expiry in the UI
- invalid, expired, or reused tokens return a clear guest-facing error page

The current prompt implements the host-side token generation and an honest local
HTTP prototype boundary. It does not implement media transport yet.

## Latency Expectations

Target expectations for the future WebRTC path:

- LAN steady-state playback: roughly `80ms` to `200ms`
- fast local networks with tuned buffering may do better, but that should not
  be promised in the UI yet
- browser guests will likely have higher and less predictable latency than the
  native Linux receiver because of browser scheduling, resampling, and autoplay
  restrictions

If a LAN beta uses WebSocket PCM first, expect:

- higher jitter sensitivity
- more buffering pressure on the browser side
- lower confidence under packet loss

## Browser Compatibility Limits

Planned support assumptions:

- Chromium-based desktop browsers: primary target
- Firefox desktop: secondary target after AudioWorklet verification
- mobile browsers: later and best-effort, because background audio rules,
  autoplay policies, and power management are stricter

Known limits the UI and docs must keep stating:

- browser playback is a separate capability from native receiver mode
- autoplay may require a user gesture before audio can start
- sample-rate behavior may differ from the native receiver path
- Bluetooth output selection inside the browser is far more limited than in the
  native Linux app

## Join URL And QR Code Strategy

Host sharing should be explicit and honest.

Phase 1, implemented now:

- generate a join link
- show it in the Receiver page
- copy it to the clipboard automatically when generated
- allow opening the host-side prototype in a browser

Phase 2, future:

- render a QR code for the same join URL
- allow reissuing and revoking tokens
- surface guest count and active browser sessions in the receiver UI

## Diagnostics And Crash Reporting

Browser sessions need their own diagnostics trail.

Host-side diagnostics should include:

- token creation time and expiry
- signaling requests served
- latest browser join error
- guest session count
- signaling and media transport failures

Guest-side diagnostics should eventually include:

- browser version and platform
- AudioContext state and sample rate
- AudioWorklet startup failures
- WebRTC selected candidate pair and connection state
- packet loss / jitter / buffering indicators
- one-click text export for bug reports

The current prompt already records host-side prototype state in the existing
SynchroSonic diagnostics snapshot.

## What Is Implemented Now

Prompt 3 adds only safe foundation work:

- a local browser-join prototype service in the desktop app
- short-lived token generation
- a host-side HTTP endpoint that proves the session boundary is reachable
- an honest placeholder join page that clearly states browser audio is not yet
  implemented
- Receiver page actions to generate and open the browser-join prototype
- diagnostics snapshot fields for browser-join activity, requests served, and
  latest errors

## What Is Future Work

The actual browser receiver path still needs:

- signaling API shape and schema stabilization
- WebRTC session setup and media path
- browser-side web app assets
- AudioWorklet playback implementation
- guest device state model in discovery/UI
- per-guest diagnostics export
- optional TURN/STUN strategy if the feature grows past simple LAN use

## Recommended Next Implementation Order

1. stabilize and expose the signaling service boundary as an internal module API
2. define browser guest session state in `synchrosonic-core`
3. add a minimal web client that can redeem a token and report diagnostics
4. implement WebRTC audio transport to the browser
5. add QR rendering and guest session visibility in the GTK UI
