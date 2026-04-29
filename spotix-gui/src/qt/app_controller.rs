use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};

use cxx_qt_lib::QString;
use spotix_core::{
    connection::Credentials,
    oauth::{self, OAuthToken},
    session::{SessionConfig, SessionConnection},
};

use crate::{
    data::{Config, SearchTopic},
    qt::{
        models::{QtAlbum, QtPlaylist, QtSearchResults, QtShow, QtTrack},
        playback_service, runtime,
    },
    webapi::WebApi,
};

#[derive(Clone)]
struct StartupState {
    authenticated: bool,
    status: String,
    route: String,
    session_configured: bool,
}

impl Default for StartupState {
    fn default() -> Self {
        Self {
            authenticated: false,
            status: "Starting Spotix".to_string(),
            route: "login".to_string(),
            session_configured: false,
        }
    }
}

static STARTUP_STATE: OnceLock<Mutex<StartupState>> = OnceLock::new();
static LOGIN_RESULT: OnceLock<Mutex<Option<Result<SpotifyAuthResult, String>>>> = OnceLock::new();
static LIBRARY_RESULT: OnceLock<Mutex<Option<Result<LibraryJson, String>>>> = OnceLock::new();
static SEARCH_RESULT: OnceLock<Mutex<Option<Result<QtSearchResults, String>>>> = OnceLock::new();
static PLAYBACK_CONFIGURED: OnceLock<Mutex<bool>> = OnceLock::new();

pub fn set_startup_state(authenticated: bool, session_configured: bool) {
    let route = if authenticated { "home" } else { "login" };
    let status = if session_configured {
        "Connected with saved Spotify credentials"
    } else if authenticated {
        "Ready to load your Spotify library"
    } else {
        "Sign in to connect Spotify"
    };
    let startup = StartupState {
        authenticated,
        status: status.to_string(),
        route: route.to_string(),
        session_configured,
    };

    let state = STARTUP_STATE.get_or_init(|| Mutex::new(StartupState::default()));
    *state.lock().expect("startup state lock poisoned") = startup;
}

struct SpotifyAuthResult {
    credentials: Option<Credentials>,
    oauth_token: OAuthToken,
}

fn startup_state() -> StartupState {
    STARTUP_STATE
        .get_or_init(|| Mutex::new(StartupState::default()))
        .lock()
        .expect("startup state lock poisoned")
        .clone()
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, authenticated)]
        #[qproperty(QString, status)]
        #[qproperty(QString, route)]
        #[qproperty(bool, login_busy)]
        #[qproperty(QString, login_status)]
        #[qproperty(QString, login_error)]
        #[qproperty(QString, search_query)]
        #[qproperty(QString, search_status)]
        #[qproperty(QString, search_results_json)]
        #[qproperty(QString, profile_name)]
        #[qproperty(QString, library_status)]
        #[qproperty(QString, playlists_json)]
        #[qproperty(QString, saved_tracks_json)]
        #[qproperty(QString, saved_albums_json)]
        #[qproperty(QString, saved_shows_json)]
        #[qproperty(QString, playback_state)]
        #[qproperty(QString, now_playing_title)]
        #[qproperty(QString, now_playing_artist)]
        #[qproperty(QString, now_playing_album)]
        #[qproperty(QString, playback_status)]
        #[qproperty(QString, queue_summary)]
        #[qproperty(i32, playback_progress_ms)]
        #[qproperty(i32, playback_duration_ms)]
        #[qproperty(f64, volume)]
        #[namespace = "spotix"]
        type SpotixApp = super::SpotixAppRust;

        #[qinvokable]
        #[cxx_name = "goHome"]
        fn go_home(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "goLogin"]
        fn go_login(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "submitSearch"]
        fn submit_search(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "startSpotifyLogin"]
        fn start_spotify_login(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "logout"]
        fn logout(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "refreshSession"]
        fn refresh_session(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "loadLibrary"]
        fn load_library(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "playTrack"]
        fn play_track(self: Pin<&mut Self>, id: &QString);

        #[qinvokable]
        #[cxx_name = "playFirstSearchResult"]
        fn play_first_search_result(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "refreshPlayback"]
        fn refresh_playback(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "playPause"]
        fn play_pause(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "playPrevious"]
        fn play_previous(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "playNext"]
        fn play_next(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "stopPlayback"]
        fn stop_playback(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "seekPlayback"]
        fn seek_playback(self: Pin<&mut Self>, progress_ratio: f64);

        #[qinvokable]
        #[cxx_name = "setPlaybackVolume"]
        fn set_playback_volume(self: Pin<&mut Self>, volume: f64);
    }
}

pub struct SpotixAppRust {
    authenticated: bool,
    status: QString,
    route: QString,
    login_busy: bool,
    login_status: QString,
    login_error: QString,
    search_query: QString,
    search_status: QString,
    search_results_json: QString,
    profile_name: QString,
    library_status: QString,
    playlists_json: QString,
    saved_tracks_json: QString,
    saved_albums_json: QString,
    saved_shows_json: QString,
    playback_state: QString,
    now_playing_title: QString,
    now_playing_artist: QString,
    now_playing_album: QString,
    playback_status: QString,
    queue_summary: QString,
    playback_progress_ms: i32,
    playback_duration_ms: i32,
    volume: f64,
}

impl Default for SpotixAppRust {
    fn default() -> Self {
        let startup = startup_state();
        let playback = playback_service::snapshot();
        Self {
            authenticated: startup.authenticated,
            status: QString::from(&startup.status),
            route: QString::from(&startup.route),
            login_busy: false,
            login_status: QString::from(if startup.authenticated {
                "Spotify credentials found"
            } else {
                "Sign in to Spotify"
            }),
            login_error: QString::from(""),
            search_query: QString::from(""),
            search_status: QString::from("Search is ready"),
            search_results_json: QString::from(empty_search_json()),
            profile_name: QString::from(""),
            library_status: QString::from(if startup.session_configured {
                "Session configured from saved credentials"
            } else {
                "Library not loaded"
            }),
            playlists_json: QString::from("[]"),
            saved_tracks_json: QString::from("[]"),
            saved_albums_json: QString::from("[]"),
            saved_shows_json: QString::from("[]"),
            playback_state: QString::from(playback.state.as_str()),
            now_playing_title: QString::from(&playback.title),
            now_playing_artist: QString::from(&playback.artist),
            now_playing_album: QString::from(&playback.album),
            playback_status: QString::from(&playback.status),
            queue_summary: QString::from(&playback.queue_summary),
            playback_progress_ms: duration_ms(playback.progress),
            playback_duration_ms: duration_ms(playback.duration),
            volume: playback.volume,
        }
    }
}

impl qobject::SpotixApp {
    pub fn go_home(self: Pin<&mut Self>) {
        self.set_route(QString::from("home"));
    }

    pub fn go_login(self: Pin<&mut Self>) {
        self.set_route(QString::from("login"));
    }

    pub fn submit_search(mut self: Pin<&mut Self>) {
        let query = self.search_query().to_string();
        let query = query.trim();

        if query.is_empty() {
            self.as_mut()
                .set_search_status(QString::from("Enter a search term first"));
            return;
        }

        if WebApi::global().is_rate_limited() {
            self.as_mut()
                .set_search_status(QString::from("Spotify search is rate limited"));
            return;
        }

        self.as_mut()
            .set_search_status(QString::from("Searching Spotify..."));

        let query = query.to_string();
        self.as_mut()
            .set_search_status(QString::from("Searching Spotify..."));
        thread::spawn(move || {
            let result = run_search(&query);
            *SEARCH_RESULT
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("qt search result lock poisoned") = Some(result);
        });
    }

    pub fn start_spotify_login(mut self: Pin<&mut Self>) {
        if *self.login_busy() {
            return;
        }

        let Some(runtime) = runtime::snapshot() else {
            self.as_mut()
                .set_login_error(QString::from("Qt runtime is not initialized"));
            return;
        };

        self.as_mut().set_login_busy(true);
        self.as_mut()
            .set_login_status(QString::from("Opening Spotify login in your browser..."));
        self.as_mut().set_login_error(QString::from(""));

        let client_id = runtime.config.effective_webapi_client_id().to_string();
        let (auth_url, pkce_verifier) = oauth::generate_auth_url(8888, &client_id);
        let proxy_url = Config::proxy();

        thread::spawn(move || {
            let result = run_spotify_login(client_id, pkce_verifier, proxy_url);
            *LOGIN_RESULT
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("qt login result lock poisoned") = Some(result);
        });

        if let Err(err) = open::that(&auth_url) {
            *LOGIN_RESULT
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("qt login result lock poisoned") =
                Some(Err(format!("Failed to open browser: {err}")));
        }
    }

    pub fn logout(mut self: Pin<&mut Self>) {
        let _ = runtime::with_runtime(|runtime| {
            runtime.config.clear_credentials();
            runtime.config.save();
            runtime.session.shutdown();
        });
        set_playback_configured(false);
        self.as_mut().set_authenticated(false);
        self.as_mut().set_route(QString::from("login"));
        self.as_mut().set_status(QString::from("Signed out"));
        self.as_mut().set_login_status(QString::from("Signed out"));
        self.as_mut().set_login_error(QString::from(""));
        self.as_mut().set_profile_name(QString::from(""));
        self.as_mut()
            .set_library_status(QString::from("Library cleared"));
        self.clear_library_json();
    }

    pub fn refresh_session(mut self: Pin<&mut Self>) {
        let login_result = LOGIN_RESULT
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("qt login result lock poisoned")
            .take();

        if let Some(result) = login_result {
            self.as_mut().set_login_busy(false);
            match result {
                Ok(payload) => {
                    let mut authenticated = false;
                    let mut status = "Spotify OAuth token saved".to_string();
                    let _ = runtime::with_runtime(|runtime| {
                        runtime
                            .config
                            .store_oauth_token(payload.oauth_token.clone());
                        WebApi::global().set_oauth_token(payload.oauth_token.clone());
                        WebApi::global()
                            .set_webapi_client_id(runtime.config.effective_webapi_client_id());
                        WebApi::global().clear_rate_limit_state();

                        if let Some(credentials) = payload.credentials {
                            runtime.config.store_credentials(credentials.clone());
                            runtime.session.update_config(SessionConfig {
                                login_creds: credentials,
                                proxy_url: Config::proxy(),
                            });
                            playback_service::init(runtime.session.clone(), &runtime.config);
                            set_playback_configured(true);
                            authenticated = true;
                            status = "Connected to Spotify".to_string();
                        }
                        runtime.config.save();
                    });

                    self.as_mut().set_authenticated(authenticated);
                    self.as_mut().set_status(QString::from(&status));
                    self.as_mut().set_login_status(QString::from(&status));
                    self.as_mut().set_login_error(QString::from(""));
                    if authenticated {
                        self.as_mut().set_route(QString::from("home"));
                    }
                }
                Err(err) => {
                    self.as_mut().set_login_error(QString::from(&err));
                    self.as_mut()
                        .set_login_status(QString::from("Spotify login failed"));
                }
            }
            return;
        }

        let configured = runtime::configure_session_from_config();
        self.as_mut().set_authenticated(configured);
        if configured {
            if !playback_configured()
                && let Some(runtime) = runtime::snapshot()
            {
                playback_service::init(runtime.session, &runtime.config);
                set_playback_configured(true);
            }
            self.as_mut()
                .set_login_status(QString::from("Connected with saved credentials"));
            self.as_mut()
                .set_status(QString::from("Connected with saved Spotify credentials"));
        }
        self.as_mut().poll_search_result();
        self.as_mut().poll_library_result();
        self.as_mut().handle_oauth_revoked();
    }

    pub fn load_library(mut self: Pin<&mut Self>) {
        if !*self.authenticated() {
            self.as_mut()
                .set_library_status(QString::from("Sign in before loading your library"));
            return;
        }

        self.as_mut()
            .set_library_status(QString::from("Loading Spotify library..."));

        thread::spawn(|| {
            let result = load_library_json();
            *LIBRARY_RESULT
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("qt library result lock poisoned") = Some(result);
        });
    }

    fn poll_search_result(mut self: Pin<&mut Self>) {
        let result = SEARCH_RESULT
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("qt search result lock poisoned")
            .take();

        if let Some(result) = result {
            match result {
                Ok(results) => {
                    let message = format!(
                        "{} artists, {} albums, {} tracks, {} playlists, {} podcasts",
                        results.artists.len(),
                        results.albums.len(),
                        results.tracks.len(),
                        results.playlists.len(),
                        results.shows.len()
                    );
                    self.as_mut().set_search_status(QString::from(&message));
                    self.as_mut()
                        .set_search_results_json(QString::from(&json_or_empty(&results)));
                }
                Err(err) => {
                    self.as_mut()
                        .set_search_status(QString::from(&format!("Search failed: {err}")));
                    self.as_mut()
                        .set_search_results_json(QString::from(empty_search_json()));
                }
            }
        }
    }

    fn poll_library_result(mut self: Pin<&mut Self>) {
        let result = LIBRARY_RESULT
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("qt library result lock poisoned")
            .take();

        if let Some(result) = result {
            match result {
                Ok(library) => {
                    playback_service::register_tracks(library.tracks_for_playback);
                    self.as_mut()
                        .set_profile_name(QString::from(&library.profile));
                    self.as_mut()
                        .set_playlists_json(QString::from(&library.playlists_json));
                    self.as_mut()
                        .set_saved_tracks_json(QString::from(&library.saved_tracks_json));
                    self.as_mut()
                        .set_saved_albums_json(QString::from(&library.saved_albums_json));
                    self.as_mut()
                        .set_saved_shows_json(QString::from(&library.saved_shows_json));
                    self.as_mut()
                        .set_library_status(QString::from("Library loaded"));
                }
                Err(err) => {
                    self.as_mut()
                        .set_library_status(QString::from(&format!("Library load failed: {err}")));
                }
            }
        }
    }

    fn handle_oauth_revoked(mut self: Pin<&mut Self>) {
        if WebApi::global().take_oauth_revoked() {
            let _ = runtime::with_runtime(|runtime| {
                runtime.config.clear_credentials();
                runtime.config.save();
                runtime.session.shutdown();
            });
            self.as_mut().set_authenticated(false);
            self.as_mut().set_route(QString::from("login"));
            self.as_mut()
                .set_login_status(QString::from("Spotify login expired"));
            self.as_mut().set_login_error(QString::from(
                "Spotify revoked the saved refresh token. Please log in again.",
            ));
            self.as_mut()
                .set_status(QString::from("Spotify login expired"));
            self.as_mut()
                .set_library_status(QString::from("Login required"));
            self.as_mut().clear_library_json();
        }
    }

    pub fn play_track(mut self: Pin<&mut Self>, id: &QString) {
        match playback_service::play_track_id(&id.to_string()) {
            Ok(message) => {
                self.as_mut().set_playback_status(QString::from(&message));
                self.as_mut().refresh_playback();
            }
            Err(err) => {
                self.as_mut()
                    .set_playback_status(QString::from(&format!("Playback failed: {err}")));
            }
        }
    }

    pub fn play_first_search_result(mut self: Pin<&mut Self>) {
        let query = self.search_query().to_string();
        self.as_mut()
            .set_playback_status(QString::from("Loading first search result..."));
        match playback_service::play_first_search_result(&query) {
            Ok(message) => {
                self.as_mut().set_search_status(QString::from(&message));
                self.refresh_playback();
            }
            Err(err) => {
                self.as_mut()
                    .set_playback_status(QString::from(&format!("Playback failed: {err}")));
            }
        }
    }

    pub fn refresh_playback(mut self: Pin<&mut Self>) {
        let playback = playback_service::snapshot();
        self.as_mut()
            .set_playback_state(QString::from(playback.state.as_str()));
        self.as_mut()
            .set_now_playing_title(QString::from(&playback.title));
        self.as_mut()
            .set_now_playing_artist(QString::from(&playback.artist));
        self.as_mut()
            .set_now_playing_album(QString::from(&playback.album));
        self.as_mut()
            .set_playback_status(QString::from(&playback.status));
        self.as_mut()
            .set_queue_summary(QString::from(&playback.queue_summary));
        self.as_mut()
            .set_playback_progress_ms(duration_ms(playback.progress));
        self.as_mut()
            .set_playback_duration_ms(duration_ms(playback.duration));
        self.as_mut().set_volume(playback.volume);
    }

    pub fn play_pause(mut self: Pin<&mut Self>) {
        playback_service::pause_or_resume();
        self.as_mut().refresh_playback();
    }

    pub fn play_previous(mut self: Pin<&mut Self>) {
        playback_service::previous();
        self.as_mut().refresh_playback();
    }

    pub fn play_next(mut self: Pin<&mut Self>) {
        playback_service::next();
        self.as_mut().refresh_playback();
    }

    pub fn stop_playback(mut self: Pin<&mut Self>) {
        playback_service::stop();
        self.as_mut().refresh_playback();
    }

    pub fn seek_playback(mut self: Pin<&mut Self>, progress_ratio: f64) {
        playback_service::seek(progress_ratio);
        self.as_mut().refresh_playback();
    }

    pub fn set_playback_volume(mut self: Pin<&mut Self>, volume: f64) {
        playback_service::set_volume(volume);
        self.as_mut().refresh_playback();
    }

    fn clear_library_json(mut self: Pin<&mut Self>) {
        self.as_mut().set_playlists_json(QString::from("[]"));
        self.as_mut().set_saved_tracks_json(QString::from("[]"));
        self.as_mut().set_saved_albums_json(QString::from("[]"));
        self.as_mut().set_saved_shows_json(QString::from("[]"));
    }
}

fn duration_ms(duration: std::time::Duration) -> i32 {
    duration.as_millis().min(i32::MAX as u128) as i32
}

fn playback_configured() -> bool {
    *PLAYBACK_CONFIGURED
        .get_or_init(|| Mutex::new(false))
        .lock()
        .expect("qt playback configured lock poisoned")
}

fn set_playback_configured(configured: bool) {
    *PLAYBACK_CONFIGURED
        .get_or_init(|| Mutex::new(false))
        .lock()
        .expect("qt playback configured lock poisoned") = configured;
}

struct LibraryJson {
    profile: String,
    playlists_json: String,
    saved_tracks_json: String,
    saved_albums_json: String,
    saved_shows_json: String,
    tracks_for_playback: Vec<Arc<crate::data::Track>>,
}

fn load_library_json() -> Result<LibraryJson, String> {
    let api = WebApi::global();
    let profile = api
        .get_user_profile()
        .map_err(|err| err.to_string())?
        .display_name
        .to_string();
    let playlists = api.get_playlists().map_err(|err| err.to_string())?;
    let saved_tracks = api.get_saved_tracks().map_err(|err| err.to_string())?;
    let saved_albums = api.get_saved_albums().map_err(|err| err.to_string())?;
    let saved_shows = api.get_saved_shows().map_err(|err| err.to_string())?;

    let qt_playlists = playlists.iter().map(QtPlaylist::from).collect::<Vec<_>>();
    let qt_tracks = saved_tracks
        .iter()
        .map(|track| QtTrack::from(&**track))
        .collect::<Vec<_>>();
    let qt_albums = saved_albums
        .iter()
        .map(|album| QtAlbum::from(&**album))
        .collect::<Vec<_>>();
    let qt_shows = saved_shows
        .iter()
        .map(|show| QtShow::from(&**show))
        .collect::<Vec<_>>();

    Ok(LibraryJson {
        profile,
        playlists_json: json_or_empty_array(&qt_playlists),
        saved_tracks_json: json_or_empty_array(&qt_tracks),
        saved_albums_json: json_or_empty_array(&qt_albums),
        saved_shows_json: json_or_empty_array(&qt_shows),
        tracks_for_playback: saved_tracks.iter().cloned().collect(),
    })
}

fn run_search(query: &str) -> Result<QtSearchResults, String> {
    if WebApi::global().is_rate_limited() {
        return Err("Spotify search is rate limited".to_string());
    }
    let results = WebApi::global()
        .search(query, SearchTopic::all(), 10)
        .map_err(|err| err.to_string())?;
    playback_service::register_tracks(results.tracks.iter().cloned());
    Ok(QtSearchResults::from(&results))
}

fn run_spotify_login(
    client_id: String,
    pkce_verifier: oauth::PkceCodeVerifier,
    proxy_url: Option<String>,
) -> Result<SpotifyAuthResult, String> {
    let code = oauth::get_authcode_listener(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8888),
        Duration::from_secs(300),
    )
    .map_err(|err| err.to_string())?;
    let token = oauth::exchange_code_for_token(8888, code, pkce_verifier, &client_id)
        .map_err(|err| err.to_string())?;

    let mut credentials = None;
    let mut last_err = None;
    for attempt in 0..3 {
        match SessionConnection::open(SessionConfig {
            login_creds: Credentials::from_access_token(token.access_token.clone()),
            proxy_url: proxy_url.clone(),
        }) {
            Ok(connection) => {
                credentials = Some(connection.credentials);
                break;
            }
            Err(err) => {
                log::warn!(
                    "qt login: Shannon authentication failed (attempt {}): {err}",
                    attempt + 1
                );
                last_err = Some(err);
            }
        }
    }

    if credentials.is_none()
        && let Some(err) = last_err
    {
        log::warn!(
            "qt login: Shannon auth failed after retries ({err}), OAuth token will still be saved"
        );
    }

    Ok(SpotifyAuthResult {
        credentials,
        oauth_token: token,
    })
}

fn json_or_empty<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|err| {
        log::warn!("qt json: failed to serialize value: {err}");
        empty_search_json().to_string()
    })
}

fn json_or_empty_array<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|err| {
        log::warn!("qt json: failed to serialize array: {err}");
        "[]".to_string()
    })
}

fn empty_search_json() -> &'static str {
    r#"{"query":"","tracks":[],"albums":[],"artists":[],"playlists":[],"shows":[]}"#
}
