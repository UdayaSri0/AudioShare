use std::{
    error::Error,
    thread,
    time::{Duration, Instant},
};

use synchrosonic_audio::LinuxAudioBackend;
use synchrosonic_core::{services::AudioBackend, AudioSourceKind, CaptureSettings};

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "synchrosonic_audio=debug,info".into()),
        )
        .init();

    let backend = LinuxAudioBackend::new();
    let sources = backend.list_sources()?;
    if sources.is_empty() {
        println!("No PipeWire capture sources were found.");
        return Ok(());
    }

    let source = sources
        .iter()
        .find(|source| source.is_default && source.kind == AudioSourceKind::Monitor)
        .or_else(|| sources.iter().find(|source| source.is_default))
        .unwrap_or(&sources[0]);
    println!("Starting capture probe from: {}", source.display_name);

    let settings = CaptureSettings {
        source_id: Some(source.id.clone()),
        ..CaptureSettings::default()
    };
    let mut capture = backend.start_capture(settings)?;

    let started = Instant::now();
    let mut printed = 0_u8;
    while started.elapsed() < Duration::from_secs(3) && printed < 5 {
        if let Some(frame) = capture.try_recv_frame()? {
            println!(
                "frame={} bytes={} peak={:.3} rms={:.3}",
                frame.sequence,
                frame.payload.len(),
                frame.stats.peak_amplitude,
                frame.stats.rms_amplitude
            );
            printed += 1;
        } else {
            thread::sleep(Duration::from_millis(20));
        }
    }

    println!("capture stats: {:?}", capture.stats());
    capture.stop()?;
    Ok(())
}
