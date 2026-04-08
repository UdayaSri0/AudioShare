# Sender To Receiver Transport Protocol

SynchroSonic's first LAN streaming path uses a single TCP connection per sender
to receiver session and streams raw PCM packets over a small framed protocol.

## Strategy

- Transport: TCP
- Audio encoding: raw PCM passthrough
- Session shape: one sender to one receiver

This LAN MVP prioritizes:

- reliability: ordered delivery and simple disconnect behavior
- low latency: no codec encode/decode stage
- maintainability: transport only handles framing, negotiation, keepalive, and
  session state while audio stays in the capture/playback layers

## Framing

Each frame on the wire is encoded as:

```text
4 bytes  magic      "SSN1"
1 byte   kind       frame type
4 bytes  meta_len   big-endian u32
4 bytes  body_len   big-endian u32
N bytes  metadata   UTF-8 JSON
M bytes  payload    raw bytes
```

Explicit limits:

- max metadata bytes: `8192`
- max payload bytes: `262144`

These limits live in `crates/synchrosonic-transport/src/protocol.rs`.

## Frame Types

### `Hello`

Sent by the sender immediately after the TCP connection opens.

Metadata:

- `protocol_version`
- `session_id`
- `sender_name`
- `supported_codecs`
- `desired_codec`
- `stream`
- `quality`
- `target_latency_ms`
- `keepalive_interval_ms`

Purpose:

- starts a session
- advertises sender capabilities
- proposes stream parameters and heartbeat timing

### `Accept`

Sent by the receiver after validating `Hello`.

Metadata:

- `protocol_version`
- `session_id`
- `receiver_name`
- `codec`
- `stream`
- `keepalive_interval_ms`
- `receiver_latency_ms`

Purpose:

- confirms session start
- negotiates the codec
- confirms stream parameters
- returns receiver-side timing values

### `Audio`

Sent by the sender after `Accept`.

Metadata:

- `sequence`
- `captured_at_ms`

Payload:

- raw PCM bytes for one captured chunk

### `Heartbeat`

Sent by the sender periodically while a session is active.

Metadata:

- `nonce`

Purpose:

- keeps the session active
- provides RTT-based latency measurement when acknowledged

### `HeartbeatAck`

Sent by the receiver in response to `Heartbeat`.

Metadata:

- `nonce`

### `Stop`

Can be sent by either side.

Metadata:

- `reason`

Purpose:

- ends a session cleanly

### `Error`

Can be sent by either side.

Metadata:

- `code`
- `message`

Purpose:

- reports protocol, negotiation, or runtime failures

## Negotiation Flow

1. Sender opens a TCP connection to the receiver endpoint discovered over mDNS.
2. Sender sends `Hello` with protocol version, codec support, stream shape, and
   heartbeat timing.
3. Receiver validates `Hello`.
4. Receiver sends `Accept` with the negotiated codec and stream parameters.
5. Sender starts local capture and moves to `Streaming`.
6. Sender streams `Audio` frames and periodic `Heartbeat` messages.
7. Receiver translates wire frames into `ReceiverTransportEvent` values.
8. Either side can send `Stop` or `Error`.

For the current MVP:

- the only codec is `RawPcm`
- stream parameters must match exactly after negotiation
- a session is one sender to one receiver

## Runtime Handoff

The end-to-end path is:

```text
pw-record
  -> LinuxAudioBackend / AudioCapture
  -> LanSenderSession
  -> TCP socket
  -> SynchroSonic framed protocol
  -> LanReceiverTransportServer
  -> ReceiverTransportEvent
  -> ReceiverRuntime buffer
  -> LinuxPlaybackEngine / pw-play
```

On the receiver side:

- `Hello` becomes `Connected`
- `Audio` becomes `AudioPacket`
- `Heartbeat` becomes `KeepAlive`
- disconnects become `Disconnected`
- protocol/runtime failures become `Error`

## Stats

Sender-side `StreamSessionSnapshot` exposes:

- `packets_sent`
- `packets_received`
- `bytes_sent`
- `bytes_received`
- `estimated_bitrate_bps`
- `latency_estimate_ms`
- `packet_gaps`
- `keepalives_sent`
- `keepalives_received`

Receiver-side `ReceiverSnapshot` exposes:

- `packets_received`
- `frames_received`
- `bytes_received`
- `packets_played`
- `frames_played`
- `bytes_played`
- `underruns`
- `overruns`
- `reconnect_attempts`
