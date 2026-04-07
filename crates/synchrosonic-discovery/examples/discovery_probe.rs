use std::{
    error::Error,
    thread,
    time::{Duration, Instant},
};

use synchrosonic_core::{
    config::DiscoveryConfig, services::DiscoveryService, DeviceAvailability, DeviceCapabilities,
};
use synchrosonic_discovery::{LocalDiscoveryProfile, MdnsDiscoveryService};

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "synchrosonic_discovery=debug,info".into()),
        )
        .init();

    let profile = LocalDiscoveryProfile::new(
        format!("synchrosonic-probe-{}", std::process::id()),
        "SynchroSonic Probe",
        env!("CARGO_PKG_VERSION"),
        DeviceCapabilities::sender_receiver(),
        DeviceAvailability::Available,
        51_700,
    );
    let mut discovery = MdnsDiscoveryService::with_profile(DiscoveryConfig::default(), profile);
    discovery.start()?;

    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(8) {
        while let Some(event) = discovery.poll_event()? {
            println!("event: {event:?}");
        }
        for event in discovery.prune_stale()? {
            println!("event: {event:?}");
        }

        let snapshot = discovery.snapshot();
        println!("{} device(s) currently in registry", snapshot.devices.len());
        for device in snapshot.devices {
            println!(
                "- {} [{}] {:?} {:?}",
                device.display_name, device.id, device.availability, device.endpoint
            );
        }

        thread::sleep(Duration::from_secs(1));
    }

    discovery.stop()?;
    Ok(())
}

