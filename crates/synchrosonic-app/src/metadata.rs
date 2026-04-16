pub const APP_ID: &str = "org.synchrosonic.SynchroSonic";
pub const APP_NAME: &str = "SynchroSonic";
pub const APP_ICON_NAME: &str = "org.synchrosonic.SynchroSonic";
pub const APP_BINARY_NAME: &str = "synchrosonic-app";
pub const APP_SUMMARY: &str = env!("CARGO_PKG_DESCRIPTION");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_LICENSE: &str = env!("CARGO_PKG_LICENSE");
pub const APP_HOMEPAGE: &str = env!("CARGO_PKG_HOMEPAGE");
pub const APP_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
pub const APP_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
pub const APP_BUG_TRACKER: &str = "https://github.com/UdayaSri0/AudioShare/issues";
pub const APP_RELEASES: &str = "https://github.com/UdayaSri0/AudioShare/releases";
pub const APP_SECURITY_POLICY_PATH: &str = "SECURITY.md";
pub const APP_CONTRIBUTING_PATH: &str = "CONTRIBUTING.md";

pub fn authors_display() -> String {
    APP_AUTHORS
        .split(':')
        .filter(|author| !author.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn release_channel_label() -> &'static str {
    if APP_VERSION.contains("-rc.") {
        "Release candidate"
    } else if APP_VERSION.contains('-') {
        "Pre-release"
    } else {
        "Stable release"
    }
}

pub fn release_channel_summary() -> &'static str {
    if APP_VERSION.contains("-rc.") {
        "This build is prepared as a public preview while stable packaging and security-reporting blockers are still being closed."
    } else if APP_VERSION.contains('-') {
        "This build is a pre-release and may still change before a stable public tag."
    } else {
        "This build is prepared as a stable public release."
    }
}
