#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::new_without_default, clippy::type_complexity)]

use druid::AppLauncher;
use spotix_gui::{bootstrap, data::AppState, delegate::Delegate, ui};

fn main() {
    bootstrap::init_logging();
    let config = bootstrap::load_config();

    ui::theme::configure_fontconfig();
    ui::theme::ensure_preset_themes();
    ui::desktop::ensure_desktop_integration();

    let mut state = AppState::default_with_config(config.clone());

    if let Some(cache) = bootstrap::create_cache() {
        state.preferences.cache = Some(cache);
    }

    bootstrap::install_webapi(state.session.clone(), &config);
    let delegate;
    let launcher;
    if state.config.has_credentials() {
        // Credentials are configured, open the main window.
        let window = ui::main_window(&state.config);
        delegate = Delegate::with_main(window.id);
        launcher = AppLauncher::with_window(window).configure_env(ui::theme::setup);

        // Load user's local tracks for the WebApi.
        bootstrap::load_local_tracks(&state.config);
    } else {
        // No configured credentials, open the account setup.
        let window = ui::account_setup_window();
        delegate = Delegate::with_preferences(window.id);
        launcher = AppLauncher::with_window(window).configure_env(ui::theme::setup);
    };

    launcher
        .delegate(delegate)
        .launch(state)
        .expect("Application launch");
}
