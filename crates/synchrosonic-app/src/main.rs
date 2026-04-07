mod ui;

use adw::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

fn main() -> gtk::glib::ExitCode {
    init_logging();

    let app = adw::Application::builder()
        .application_id("org.synchrosonic.SynchroSonic")
        .build();

    app.connect_activate(ui::build_main_window);
    app.run()
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).try_init();
}

