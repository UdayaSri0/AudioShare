mod browser_join;
mod diagnostics;
mod logging;
mod metadata;
mod persistence;
mod ui;

use adw::prelude::*;
use synchrosonic_core::DiagnosticEvent;

use crate::{
    diagnostics::DiagnosticsRuntime,
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
    let (diagnostics_runtime, diagnostics_warnings) =
        DiagnosticsRuntime::bootstrap(&startup.paths, &startup.config, logging.store.clone())
            .expect("diagnostics bootstrap should succeed");
    diagnostics_runtime.install_panic_hook();
    let mut startup_diagnostics = startup.diagnostics;
    for warning in logging.warnings {
        startup_diagnostics.push(DiagnosticEvent::warning("logging", warning));
    }
    startup_diagnostics.extend(diagnostics_warnings);

    let launch = ui::UiLaunchContext {
        config: startup.config,
        startup_diagnostics,
        paths: startup.paths,
        log_store: logging.store,
        diagnostics_runtime: diagnostics_runtime.clone(),
    };

    let app = adw::Application::builder().application_id(APP_ID).build();
    {
        let diagnostics_runtime = diagnostics_runtime.clone();
        app.connect_shutdown(move |_| {
            if let Err(error) = diagnostics_runtime.mark_clean_shutdown() {
                tracing::warn!(error = %error, "failed to mark clean shutdown");
            }
        });
    }

    app.connect_activate(move |app| ui::build_main_window(app, launch.clone()));
    let exit_code = app.run();
    if let Err(error) = diagnostics_runtime.mark_clean_shutdown() {
        tracing::warn!(error = %error, "failed to finalize clean shutdown marker");
    }
    exit_code
}
