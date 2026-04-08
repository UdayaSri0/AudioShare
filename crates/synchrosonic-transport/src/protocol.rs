use std::io::{Read, Write};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use synchrosonic_core::{
    receiver::ReceiverStreamConfig,
    streaming::{StreamCodec, STREAM_PROTOCOL_VERSION},
    QualityPreset, TransportError,
};

const FRAME_MAGIC: [u8; 4] = *b"SSN1";
const FRAME_HEADER_LEN: usize = 13;
pub const MAX_METADATA_BYTES: usize = 8 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    Hello = 1,
    Accept = 2,
    Audio = 3,
    Heartbeat = 4,
    HeartbeatAck = 5,
    Stop = 6,
    Error = 7,
}

impl FrameKind {
    pub fn from_u8(value: u8) -> Result<Self, TransportError> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::Accept),
            3 => Ok(Self::Audio),
            4 => Ok(Self::Heartbeat),
            5 => Ok(Self::HeartbeatAck),
            6 => Ok(Self::Stop),
            7 => Ok(Self::Error),
            _ => Err(TransportError::InvalidProtocol(format!(
                "unknown frame kind byte {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub metadata: Vec<u8>,
    pub payload: Vec<u8>,
    pub wire_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloMessage {
    pub protocol_version: u16,
    pub session_id: String,
    pub sender_name: String,
    pub supported_codecs: Vec<StreamCodec>,
    pub desired_codec: StreamCodec,
    pub stream: ReceiverStreamConfig,
    pub quality: QualityPreset,
    pub target_latency_ms: u16,
    pub keepalive_interval_ms: u16,
}

impl HelloMessage {
    pub fn new(
        session_id: impl Into<String>,
        sender_name: impl Into<String>,
        stream: ReceiverStreamConfig,
        quality: QualityPreset,
        target_latency_ms: u16,
        keepalive_interval_ms: u16,
    ) -> Self {
        Self {
            protocol_version: STREAM_PROTOCOL_VERSION,
            session_id: session_id.into(),
            sender_name: sender_name.into(),
            supported_codecs: vec![StreamCodec::RawPcm],
            desired_codec: StreamCodec::RawPcm,
            stream,
            quality,
            target_latency_ms,
            keepalive_interval_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptMessage {
    pub protocol_version: u16,
    pub session_id: String,
    pub receiver_name: String,
    pub codec: StreamCodec,
    pub stream: ReceiverStreamConfig,
    pub keepalive_interval_ms: u16,
    pub receiver_latency_ms: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioMessage {
    pub sequence: u64,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatMessage {
    pub nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopMessage {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub code: String,
    pub message: String,
}

pub fn write_message<T: Serialize>(
    writer: &mut impl Write,
    kind: FrameKind,
    metadata: &T,
    payload: &[u8],
) -> Result<usize, TransportError> {
    let metadata = serde_json::to_vec(metadata)
        .map_err(|error| TransportError::InvalidProtocol(error.to_string()))?;
    write_frame(writer, kind, &metadata, payload)
}

pub fn write_frame(
    writer: &mut impl Write,
    kind: FrameKind,
    metadata: &[u8],
    payload: &[u8],
) -> Result<usize, TransportError> {
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(TransportError::InvalidProtocol(format!(
            "metadata frame exceeded {MAX_METADATA_BYTES} bytes"
        )));
    }
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(TransportError::InvalidProtocol(format!(
            "payload frame exceeded {MAX_PAYLOAD_BYTES} bytes"
        )));
    }

    let mut header = Vec::with_capacity(FRAME_HEADER_LEN);
    header.extend_from_slice(&FRAME_MAGIC);
    header.push(kind as u8);
    header.extend_from_slice(&(metadata.len() as u32).to_be_bytes());
    header.extend_from_slice(&(payload.len() as u32).to_be_bytes());

    writer
        .write_all(&header)
        .and_then(|()| writer.write_all(metadata))
        .and_then(|()| writer.write_all(payload))
        .map_err(|source| TransportError::Io {
            context: format!("writing {:?} frame", kind),
            source,
        })?;

    Ok(header.len() + metadata.len() + payload.len())
}

pub fn read_frame(reader: &mut impl Read) -> Result<Frame, TransportError> {
    let mut header = [0_u8; FRAME_HEADER_LEN];
    reader
        .read_exact(&mut header)
        .map_err(|source| TransportError::Io {
            context: "reading transport frame header".to_string(),
            source,
        })?;

    if header[0..4] != FRAME_MAGIC {
        return Err(TransportError::InvalidProtocol(
            "frame magic did not match SynchroSonic protocol".to_string(),
        ));
    }

    let kind = FrameKind::from_u8(header[4])?;
    let metadata_len = u32::from_be_bytes([header[5], header[6], header[7], header[8]]) as usize;
    let payload_len = u32::from_be_bytes([header[9], header[10], header[11], header[12]]) as usize;

    if metadata_len > MAX_METADATA_BYTES {
        return Err(TransportError::InvalidProtocol(format!(
            "metadata length {metadata_len} exceeded {MAX_METADATA_BYTES}"
        )));
    }
    if payload_len > MAX_PAYLOAD_BYTES {
        return Err(TransportError::InvalidProtocol(format!(
            "payload length {payload_len} exceeded {MAX_PAYLOAD_BYTES}"
        )));
    }

    let mut metadata = vec![0_u8; metadata_len];
    let mut payload = vec![0_u8; payload_len];
    reader
        .read_exact(&mut metadata)
        .and_then(|()| reader.read_exact(&mut payload))
        .map_err(|source| TransportError::Io {
            context: format!("reading {:?} frame body", kind),
            source,
        })?;

    Ok(Frame {
        kind,
        metadata,
        payload,
        wire_bytes: FRAME_HEADER_LEN + metadata_len + payload_len,
    })
}

pub fn decode_metadata<T: DeserializeOwned>(frame: &Frame) -> Result<T, TransportError> {
    serde_json::from_slice(&frame.metadata)
        .map_err(|error| TransportError::InvalidProtocol(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_round_trips_binary_audio_frames() {
        let metadata = AudioMessage {
            sequence: 4,
            captured_at_ms: 99,
        };
        let payload = vec![1_u8, 2, 3, 4];
        let mut encoded = Vec::new();

        let written = write_message(&mut encoded, FrameKind::Audio, &metadata, &payload)
            .expect("frame should encode");
        let decoded = read_frame(&mut encoded.as_slice()).expect("frame should decode");
        let decoded_metadata: AudioMessage =
            decode_metadata(&decoded).expect("metadata should decode");

        assert_eq!(written, encoded.len());
        assert_eq!(decoded.kind, FrameKind::Audio);
        assert_eq!(decoded.payload, payload);
        assert_eq!(decoded_metadata.sequence, 4);
    }
}
