use std::collections::VecDeque;

use synchrosonic_core::{
    ReceiverAudioPacket, ReceiverBufferSnapshot, ReceiverError, ReceiverLatencyProfile,
    ReceiverStreamConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferPushOutcome {
    Accepted,
    DroppedOldest { dropped_sequence: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferedPacket {
    pub packet: ReceiverAudioPacket,
    pub frame_count: u32,
}

#[derive(Debug, Clone)]
pub struct ReceiverPacketBuffer {
    packets: VecDeque<BufferedPacket>,
    queued_frames: u64,
    start_threshold_packets: usize,
    max_packets: usize,
}

impl ReceiverPacketBuffer {
    pub fn new(profile: ReceiverLatencyProfile) -> Self {
        Self {
            packets: VecDeque::new(),
            queued_frames: 0,
            start_threshold_packets: profile.start_buffer_packets.max(1),
            max_packets: profile.max_buffer_packets.max(profile.start_buffer_packets.max(1)),
        }
    }

    pub fn clear(&mut self) {
        self.packets.clear();
        self.queued_frames = 0;
    }

    pub fn is_ready(&self) -> bool {
        self.packets.len() >= self.start_threshold_packets
    }

    pub fn push(
        &mut self,
        packet: ReceiverAudioPacket,
        stream: &ReceiverStreamConfig,
    ) -> Result<BufferPushOutcome, ReceiverError> {
        let frame_count = packet
            .frame_count(stream)
            .map_err(ReceiverError::InvalidTransport)?;
        let mut outcome = BufferPushOutcome::Accepted;

        if self.packets.len() == self.max_packets {
            if let Some(dropped) = self.packets.pop_front() {
                self.queued_frames = self.queued_frames.saturating_sub(dropped.frame_count as u64);
                outcome = BufferPushOutcome::DroppedOldest {
                    dropped_sequence: dropped.packet.sequence,
                };
            }
        }

        self.queued_frames += frame_count as u64;
        self.packets.push_back(BufferedPacket { packet, frame_count });
        Ok(outcome)
    }

    pub fn pop(&mut self) -> Option<BufferedPacket> {
        let packet = self.packets.pop_front()?;
        self.queued_frames = self.queued_frames.saturating_sub(packet.frame_count as u64);
        Some(packet)
    }

    pub fn snapshot(&self) -> ReceiverBufferSnapshot {
        ReceiverBufferSnapshot {
            queued_packets: self.packets.len() as u32,
            queued_frames: self.queued_frames,
            start_threshold_packets: self.start_threshold_packets as u32,
            max_packets: self.max_packets as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synchrosonic_core::{ReceiverAudioPacket, ReceiverLatencyPreset, ReceiverStreamConfig};

    #[test]
    fn buffer_transitions_to_ready_when_threshold_is_met() {
        let profile = ReceiverLatencyPreset::Balanced.profile();
        let stream = ReceiverStreamConfig::default();
        let mut buffer = ReceiverPacketBuffer::new(profile);

        for sequence in 0..profile.start_buffer_packets {
            buffer
                .push(
                    ReceiverAudioPacket {
                        sequence: sequence as u64,
                        captured_at_ms: 0,
                        payload: vec![0; stream.packet_bytes_hint()],
                    },
                    &stream,
                )
                .expect("packet should be buffered");
        }

        assert!(buffer.is_ready());
        assert_eq!(buffer.snapshot().queued_packets, profile.start_buffer_packets as u32);
    }

    #[test]
    fn buffer_drops_oldest_packet_when_full() {
        let profile = ReceiverLatencyPreset::LowLatency.profile();
        let stream = ReceiverStreamConfig::default();
        let mut buffer = ReceiverPacketBuffer::new(profile);

        for sequence in 0..profile.max_buffer_packets {
            buffer
                .push(
                    ReceiverAudioPacket {
                        sequence: sequence as u64,
                        captured_at_ms: 0,
                        payload: vec![0; stream.packet_bytes_hint()],
                    },
                    &stream,
                )
                .expect("packet should be buffered");
        }

        let outcome = buffer
            .push(
                ReceiverAudioPacket {
                    sequence: 999,
                    captured_at_ms: 0,
                    payload: vec![0; stream.packet_bytes_hint()],
                },
                &stream,
            )
            .expect("packet should be accepted by dropping stale audio");

        assert_eq!(
            outcome,
            BufferPushOutcome::DroppedOldest {
                dropped_sequence: 0
            }
        );
        assert_eq!(buffer.snapshot().queued_packets, profile.max_buffer_packets as u32);
    }
}
