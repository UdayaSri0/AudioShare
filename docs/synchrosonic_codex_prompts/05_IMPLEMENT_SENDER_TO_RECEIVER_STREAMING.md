# Prompt 5 — implement sender-to-receiver streaming over LAN

```text
Implement the first end-to-end sender-to-receiver streaming path over Wi‑Fi/LAN.

Goal:
The sender captures local system audio and streams it to one receiver device over the network.

Requirements:
- Choose and implement a sensible transport and audio encoding strategy
- Prioritize reliability, low latency, and maintainability
- Define a transport protocol with:
  - session start
  - capability negotiation
  - stream parameters
  - keepalive/heartbeat
  - stop/disconnect
  - error reporting
- Sender should be able to select one discovered receiver and start a stream
- Receiver should play the incoming stream
- Add basic stats:
  - bytes sent/received
  - estimated bitrate
  - latency estimate
  - packet loss or drop counters if relevant

Important:
- Document all protocol/data structures
- Avoid hidden magic constants
- Keep network code separate from audio engine and UI
- Make it easy to support multiple receivers later

Deliverables:
- one-receiver end-to-end stream
- sender session manager
- receiver session handling
- protocol documentation
- visible stream status in the UI state

Validation:
- compile, run, and verify end-to-end path
- summarize the full pipeline from capture -> encode -> transport -> decode -> playback
```
used
v0.0.5