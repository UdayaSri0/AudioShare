mod logging;
mod metadata;
mod persistence;
mod ui;

use adw::prelude::*;
use synchrosonic_core::DiagnosticEvent;

use crate::{
    logging::init_logging,
    metadata::APP_ID,
    persistence::{load_startup_config, AppPaths},
};

fn main() -> gtk::glib::ExitCode {
    let startup = load_startup_config(AppPaths::resolve());
    let logging = init_logging(
        startup.config.diagnostics.verbose_logging,
        &startup.paths.log_path,
    );
    let mut startup_diagnostics = startup.diagnostics;
    for warning in logging.warnings {
        startup_diagnostics.push(DiagnosticEvent::warning("logging", warning));
    }

    let launch = ui::UiLaunchContext {
        config: startup.config,
        startup_diagnostics,
        paths: startup.paths,
        log_store: logging.store,
    };

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(move |app| ui::build_main_window(app, launch.clone()));
    app.run()
}
