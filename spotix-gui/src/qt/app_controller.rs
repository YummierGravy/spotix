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
    data::{Config, SearchTopic, Track},
    qt::{
        models::{
            QtAlbum, QtDetailRow, QtNavDocument, QtPlaylist, QtSearchResults, QtShow, QtTrack,
            QtTreeItem,
        },
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
static NAV_RESULT: OnceLock<Mutex<Option<Result<QtNavPayload, String>>>> = OnceLock::new();
static NAV_STATE: OnceLock<Mutex<QtNavState>> = OnceLock::new();
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum QtNavTarget {
    Home,
    Login,
    Library,
    SavedTracks,
    Playlists,
    SavedAlbums,
    Artists,
    Shows,
    Search,
    Lyrics,
    Playlist { id: String, name: String },
    Album { id: String, name: String },
    Artist { id: String, name: String },
    Show { id: String, name: String },
}

impl QtNavTarget {
    fn from_item_id(id: &str) -> Option<Self> {
        match id {
            "route:home" | "home" => Some(Self::Home),
            "route:login" | "login" | "account" => Some(Self::Login),
            "route:library" | "library" => Some(Self::Library),
            "route:saved-tracks" | "saved-tracks" | "tracks" => Some(Self::SavedTracks),
            "route:playlists" | "playlists" => Some(Self::Playlists),
            "route:saved-albums" | "saved-albums" | "albums" => Some(Self::SavedAlbums),
            "route:artists" | "artists" => Some(Self::Artists),
            "route:shows" | "shows" => Some(Self::Shows),
            "route:search" | "search" => Some(Self::Search),
            "route:lyrics" | "lyrics" => Some(Self::Lyrics),
            _ => {
                let (kind, spotify_id) = id.split_once(':')?;
                if spotify_id.is_empty() {
                    return None;
                }
                let name = spotify_id.to_string();
                match kind {
                    "playlist" => Some(Self::Playlist {
                        id: spotify_id.to_string(),
                        name,
                    }),
                    "album" => Some(Self::Album {
                        id: spotify_id.to_string(),
                        name,
                    }),
                    "artist" => Some(Self::Artist {
                        id: spotify_id.to_string(),
                        name,
                    }),
                    "show" => Some(Self::Show {
                        id: spotify_id.to_string(),
                        name,
                    }),
                    _ => None,
                }
            }
        }
    }

    fn route(&self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Login => "login",
            Self::Library => "library",
            Self::SavedTracks => "saved-tracks",
            Self::Playlists => "playlists",
            Self::SavedAlbums => "saved-albums",
            Self::Artists => "artists",
            Self::Shows => "shows",
            Self::Search => "search",
            Self::Lyrics => "lyrics",
            Self::Playlist { .. } => "playlist-detail",
            Self::Album { .. } => "album-detail",
            Self::Artist { .. } => "artist-detail",
            Self::Show { .. } => "show-detail",
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Home => "Home".to_string(),
            Self::Login => "Account".to_string(),
            Self::Library => "Library".to_string(),
            Self::SavedTracks => "Saved Tracks".to_string(),
            Self::Playlists => "Playlists".to_string(),
            Self::SavedAlbums => "Albums".to_string(),
            Self::Artists => "Artists".to_string(),
            Self::Shows => "Podcasts".to_string(),
            Self::Search => "Search".to_string(),
            Self::Lyrics => "Lyrics".to_string(),
            Self::Playlist { name, .. }
            | Self::Album { name, .. }
            | Self::Artist { name, .. }
            | Self::Show { name, .. } => name.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct QtNavState {
    current: QtNavTarget,
    history: Vec<QtNavTarget>,
}

impl Default for QtNavState {
    fn default() -> Self {
        let startup = startup_state();
        Self {
            current: QtNavTarget::from_item_id(&startup.route).unwrap_or(QtNavTarget::Login),
            history: Vec::new(),
        }
    }
}

struct QtNavPayload {
    target: QtNavTarget,
    document: QtNavDocument,
    tracks_for_playback: Vec<Arc<Track>>,
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
        #[qproperty(QString, nav_tree_json)]
        #[qproperty(QString, active_route_title)]
        #[qproperty(QString, detail_rows_json)]
        #[qproperty(QString, detail_status)]
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

        #[qinvokable]
        #[cxx_name = "navigateToRoute"]
        fn navigate_to_route(self: Pin<&mut Self>, route: &QString);

        #[qinvokable]
        #[cxx_name = "navigateBack"]
        fn navigate_back(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "activateTreeItem"]
        fn activate_tree_item(self: Pin<&mut Self>, item_id: &QString);

        #[qinvokable]
        #[cxx_name = "activateDetailRow"]
        fn activate_detail_row(self: Pin<&mut Self>, row_id: &QString);

        #[qinvokable]
        #[cxx_name = "openAlbum"]
        fn open_album(self: Pin<&mut Self>, id: &QString, name: &QString);

        #[qinvokable]
        #[cxx_name = "openArtist"]
        fn open_artist(self: Pin<&mut Self>, id: &QString, name: &QString);

        #[qinvokable]
        #[cxx_name = "openPlaylist"]
        fn open_playlist(self: Pin<&mut Self>, id: &QString, name: &QString);

        #[qinvokable]
        #[cxx_name = "openShow"]
        fn open_show(self: Pin<&mut Self>, id: &QString, name: &QString);
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
    nav_tree_json: QString,
    active_route_title: QString,
    detail_rows_json: QString,
    detail_status: QString,
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
        let nav_state = QtNavState::default();
        set_nav_state(nav_state.clone());
        let initial_detail = QtNavDocument {
            title: nav_state.current.title(),
            status: "Use the tree or keyboard shortcuts to navigate Spotix.".to_string(),
            rows: home_rows(&startup.status),
        };
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
            nav_tree_json: QString::from(&json_or_empty_array(&nav_tree(
                &nav_state,
                &[],
                &[],
                &[],
            ))),
            active_route_title: QString::from(&initial_detail.title),
            detail_rows_json: QString::from(&json_or_empty_array(&initial_detail.rows)),
            detail_status: QString::from(&initial_detail.status),
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
        self.navigate_to(QtNavTarget::Home, true);
    }

    pub fn go_login(self: Pin<&mut Self>) {
        self.navigate_to(QtNavTarget::Login, true);
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
        self.as_mut().set_status(QString::from("Signed out"));
        self.as_mut().set_login_status(QString::from("Signed out"));
        self.as_mut().set_login_error(QString::from(""));
        self.as_mut().set_profile_name(QString::from(""));
        self.as_mut()
            .set_library_status(QString::from("Library cleared"));
        self.as_mut().clear_library_json();
        self.navigate_to(QtNavTarget::Login, false);
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
                        self.as_mut().navigate_to(QtNavTarget::Home, false);
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
        self.as_mut().poll_nav_result();
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
                    if nav_state().current == QtNavTarget::Search {
                        self.as_mut().navigate_to(QtNavTarget::Search, false);
                    }
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
                    self.as_mut().sync_nav_tree();
                    if matches!(
                        nav_state().current,
                        QtNavTarget::Library
                            | QtNavTarget::SavedTracks
                            | QtNavTarget::Playlists
                            | QtNavTarget::SavedAlbums
                            | QtNavTarget::Shows
                    ) {
                        let current = nav_state().current;
                        self.as_mut().navigate_to(current, false);
                    }
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
            self.as_mut().navigate_to(QtNavTarget::Login, false);
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

    pub fn navigate_to_route(self: Pin<&mut Self>, route: &QString) {
        if let Some(target) = QtNavTarget::from_item_id(&route.to_string()) {
            self.navigate_to(target, true);
        }
    }

    pub fn navigate_back(mut self: Pin<&mut Self>) {
        let mut state = nav_state();
        if let Some(target) = state.history.pop() {
            set_nav_state(state);
            self.as_mut().navigate_to(target, false);
        }
    }

    pub fn activate_tree_item(mut self: Pin<&mut Self>, item_id: &QString) {
        let item_id = item_id.to_string();
        if item_id.starts_with("track:") {
            self.as_mut().activate_detail_row(&QString::from(&item_id));
            return;
        }
        if let Some(target) = QtNavTarget::from_item_id(&item_id) {
            self.as_mut().navigate_to(target, true);
        }
    }

    pub fn activate_detail_row(mut self: Pin<&mut Self>, row_id: &QString) {
        let row_id = row_id.to_string();
        if let Some(track_id) = row_id.strip_prefix("track:") {
            let context_ids = parse_json_array::<QtDetailRow>(&self.detail_rows_json().to_string())
                .into_iter()
                .filter(|row| row.playable)
                .filter_map(|row| row.id.strip_prefix("track:").map(str::to_string))
                .collect::<Vec<_>>();
            match playback_service::play_track_context(track_id, context_ids) {
                Ok(message) => {
                    self.as_mut().set_playback_status(QString::from(&message));
                    self.as_mut().refresh_playback();
                }
                Err(err) => {
                    self.as_mut()
                        .set_playback_status(QString::from(&format!("Playback failed: {err}")));
                }
            }
            return;
        }
        if let Some(target) = QtNavTarget::from_item_id(&row_id) {
            self.as_mut().navigate_to(target, true);
        }
    }

    pub fn open_album(self: Pin<&mut Self>, id: &QString, name: &QString) {
        self.navigate_to(
            QtNavTarget::Album {
                id: id.to_string(),
                name: name.to_string(),
            },
            true,
        );
    }

    pub fn open_artist(self: Pin<&mut Self>, id: &QString, name: &QString) {
        self.navigate_to(
            QtNavTarget::Artist {
                id: id.to_string(),
                name: name.to_string(),
            },
            true,
        );
    }

    pub fn open_playlist(self: Pin<&mut Self>, id: &QString, name: &QString) {
        self.navigate_to(
            QtNavTarget::Playlist {
                id: id.to_string(),
                name: name.to_string(),
            },
            true,
        );
    }

    pub fn open_show(self: Pin<&mut Self>, id: &QString, name: &QString) {
        self.navigate_to(
            QtNavTarget::Show {
                id: id.to_string(),
                name: name.to_string(),
            },
            true,
        );
    }

    fn navigate_to(mut self: Pin<&mut Self>, target: QtNavTarget, push_history: bool) {
        let mut state = nav_state();
        if push_history && state.current != target {
            state.history.push(state.current.clone());
        }
        state.current = target.clone();
        set_nav_state(state.clone());

        self.as_mut().set_route(QString::from(target.route()));
        self.as_mut()
            .set_active_route_title(QString::from(&target.title()));
        self.as_mut().sync_nav_tree();

        match immediate_nav_document(&target, self.as_ref().get_ref()) {
            Some(document) => {
                self.as_mut().apply_nav_document(document);
            }
            None => {
                self.as_mut()
                    .set_detail_status(QString::from("Loading route data..."));
                self.as_mut()
                    .set_detail_rows_json(QString::from(&json_or_empty_array(&loading_rows(
                        &target.title(),
                    ))));
                thread::spawn(move || {
                    let result = load_nav_document(target);
                    *NAV_RESULT
                        .get_or_init(|| Mutex::new(None))
                        .lock()
                        .expect("qt nav result lock poisoned") = Some(result);
                });
            }
        }
    }

    fn poll_nav_result(mut self: Pin<&mut Self>) {
        let result = NAV_RESULT
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("qt nav result lock poisoned")
            .take();

        if let Some(result) = result {
            match result {
                Ok(payload) => {
                    let state = nav_state();
                    if state.current == payload.target {
                        playback_service::register_tracks(payload.tracks_for_playback);
                        self.as_mut().apply_nav_document(payload.document);
                    }
                    self.as_mut().sync_nav_tree();
                }
                Err(err) => {
                    self.as_mut()
                        .set_detail_status(QString::from(&format!("Route load failed: {err}")));
                    self.as_mut()
                        .set_detail_rows_json(QString::from(&json_or_empty_array(&error_rows(
                            &err,
                        ))));
                }
            }
        }
    }

    fn apply_nav_document(mut self: Pin<&mut Self>, document: QtNavDocument) {
        self.as_mut()
            .set_active_route_title(QString::from(&document.title));
        self.as_mut()
            .set_detail_status(QString::from(&document.status));
        self.as_mut()
            .set_detail_rows_json(QString::from(&json_or_empty_array(&document.rows)));
    }

    fn sync_nav_tree(mut self: Pin<&mut Self>) {
        let playlists = parse_json_array::<QtPlaylist>(&self.playlists_json().to_string());
        let albums = parse_json_array::<QtAlbum>(&self.saved_albums_json().to_string());
        let shows = parse_json_array::<QtShow>(&self.saved_shows_json().to_string());
        let tree = nav_tree(&nav_state(), &playlists, &albums, &shows);
        self.as_mut()
            .set_nav_tree_json(QString::from(&json_or_empty_array(&tree)));
    }

    fn clear_library_json(mut self: Pin<&mut Self>) {
        self.as_mut().set_playlists_json(QString::from("[]"));
        self.as_mut().set_saved_tracks_json(QString::from("[]"));
        self.as_mut().set_saved_albums_json(QString::from("[]"));
        self.as_mut().set_saved_shows_json(QString::from("[]"));
        self.as_mut().sync_nav_tree();
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

fn nav_state() -> QtNavState {
    NAV_STATE
        .get_or_init(|| Mutex::new(QtNavState::default()))
        .lock()
        .expect("qt nav state lock poisoned")
        .clone()
}

fn set_nav_state(state: QtNavState) {
    *NAV_STATE
        .get_or_init(|| Mutex::new(QtNavState::default()))
        .lock()
        .expect("qt nav state lock poisoned") = state;
}

fn nav_tree(
    state: &QtNavState,
    playlists: &[QtPlaylist],
    albums: &[QtAlbum],
    shows: &[QtShow],
) -> Vec<QtTreeItem> {
    let mut items = Vec::new();
    push_tree_route(&mut items, "route:home", "", "home", "Spotix", "root", 0);
    push_tree_route(
        &mut items,
        "route:login",
        "route:home",
        "account",
        "Account",
        "",
        1,
    );
    push_tree_route(
        &mut items,
        "route:library",
        "route:home",
        "library",
        "Library",
        "saved",
        1,
    );
    push_tree_route(
        &mut items,
        "route:saved-tracks",
        "route:library",
        "tracks",
        "Saved Tracks",
        "",
        2,
    );
    push_tree_route(
        &mut items,
        "route:playlists",
        "route:library",
        "playlists",
        "Playlists",
        &format!("{} loaded", playlists.len()),
        2,
    );
    for playlist in playlists.iter().take(40) {
        push_tree_route(
            &mut items,
            &format!("playlist:{}", playlist.id),
            "route:playlists",
            "playlist",
            &playlist.title,
            &playlist.owner,
            3,
        );
    }
    push_tree_route(
        &mut items,
        "route:saved-albums",
        "route:library",
        "albums",
        "Albums",
        &format!("{} loaded", albums.len()),
        2,
    );
    for album in albums.iter().take(40) {
        push_tree_route(
            &mut items,
            &format!("album:{}", album.id),
            "route:saved-albums",
            "album",
            &album.title,
            &album.artist,
            3,
        );
    }
    push_tree_route(
        &mut items,
        "route:artists",
        "route:library",
        "artists",
        "Artists",
        "from search/detail",
        2,
    );
    push_tree_route(
        &mut items,
        "route:shows",
        "route:library",
        "shows",
        "Podcasts",
        &format!("{} loaded", shows.len()),
        2,
    );
    for show in shows.iter().take(40) {
        push_tree_route(
            &mut items,
            &format!("show:{}", show.id),
            "route:shows",
            "show",
            &show.title,
            &show.publisher,
            3,
        );
    }
    push_tree_route(
        &mut items,
        "route:search",
        "route:home",
        "search",
        "Search",
        "press /",
        1,
    );
    push_tree_route(
        &mut items,
        "route:lyrics",
        "route:home",
        "lyrics",
        "Lyrics",
        "parity",
        1,
    );

    for item in &mut items {
        item.expanded = item.depth < 2 || item.id == format!("route:{}", state.current.route());
    }
    items
}

fn push_tree_route(
    items: &mut Vec<QtTreeItem>,
    id: &str,
    parent_id: &str,
    kind: &str,
    label: &str,
    meta: &str,
    depth: i32,
) {
    items.push(QtTreeItem {
        id: id.to_string(),
        parent_id: parent_id.to_string(),
        kind: kind.to_string(),
        label: label.to_string(),
        meta: meta.to_string(),
        depth,
        expanded: true,
        selectable: true,
        playable: false,
    });
}

fn immediate_nav_document(target: &QtNavTarget, app: &qobject::SpotixApp) -> Option<QtNavDocument> {
    let title = target.title();
    let status = match target {
        QtNavTarget::Home => app.status().to_string(),
        QtNavTarget::Login => app.login_status().to_string(),
        QtNavTarget::Library
        | QtNavTarget::SavedTracks
        | QtNavTarget::Playlists
        | QtNavTarget::SavedAlbums
        | QtNavTarget::Artists
        | QtNavTarget::Shows => app.library_status().to_string(),
        QtNavTarget::Search => app.search_status().to_string(),
        QtNavTarget::Lyrics => "Lyrics view is still parity work.".to_string(),
        QtNavTarget::Playlist { .. }
        | QtNavTarget::Album { .. }
        | QtNavTarget::Artist { .. }
        | QtNavTarget::Show { .. } => return None,
    };

    let rows = match target {
        QtNavTarget::Home => home_rows(&status),
        QtNavTarget::Login => account_rows(app.authenticated(), &app.login_error().to_string()),
        QtNavTarget::Library => library_rows(app),
        QtNavTarget::SavedTracks => {
            qt_track_rows(&parse_json_array(&app.saved_tracks_json().to_string()), 0)
        }
        QtNavTarget::Playlists => parse_json_array::<QtPlaylist>(&app.playlists_json().to_string())
            .iter()
            .map(|playlist| {
                QtDetailRow::route(
                    format!("playlist:{}", playlist.id),
                    "playlist",
                    &playlist.title,
                    &playlist.owner,
                    0,
                )
            })
            .collect(),
        QtNavTarget::SavedAlbums => {
            parse_json_array::<QtAlbum>(&app.saved_albums_json().to_string())
                .iter()
                .map(|album| {
                    QtDetailRow::route(
                        format!("album:{}", album.id),
                        "album",
                        &album.title,
                        &album.artist,
                        0,
                    )
                })
                .collect()
        }
        QtNavTarget::Artists => vec![QtDetailRow::route(
            "route:search",
            "search",
            "Search for artists",
            "artist detail routes are loaded from search results",
            0,
        )],
        QtNavTarget::Shows => parse_json_array::<QtShow>(&app.saved_shows_json().to_string())
            .iter()
            .map(|show| {
                QtDetailRow::route(
                    format!("show:{}", show.id),
                    "show",
                    &show.title,
                    &show.publisher,
                    0,
                )
            })
            .collect(),
        QtNavTarget::Search => search_rows(&app.search_results_json().to_string()),
        QtNavTarget::Lyrics => vec![QtDetailRow::route(
            "route:lyrics",
            "lyrics",
            "Lyrics",
            "not yet implemented in the Qt port",
            0,
        )],
        _ => Vec::new(),
    };

    Some(QtNavDocument {
        title,
        status,
        rows,
    })
}

fn load_nav_document(target: QtNavTarget) -> Result<QtNavPayload, String> {
    let api = WebApi::global();
    let mut tracks_for_playback = Vec::new();
    let document = match &target {
        QtNavTarget::Playlist { id, name } => {
            let playlist = api.get_playlist(id).map_err(|err| err.to_string())?;
            let tracks = api
                .get_playlist_tracks_all(id)
                .map_err(|err| err.to_string())?;
            tracks_for_playback.extend(tracks.iter().cloned());
            let mut rows = vec![QtDetailRow::playlist(&playlist, 0)];
            rows.push(QtDetailRow::route(
                format!("playlist:{id}"),
                "section",
                "Tracks",
                format!("{} track(s)", tracks.len()),
                1,
            ));
            rows.extend(tracks.iter().map(|track| QtDetailRow::track(track, 2)));
            QtNavDocument {
                title: playlist.name.to_string(),
                status: format!("Playlist | {name} | {} track(s)", tracks.len()),
                rows,
            }
        }
        QtNavTarget::Album { id, name } => {
            let album = api.get_album(id).map_err(|err| err.to_string())?.data;
            let tracks = album.clone().into_tracks_with_context();
            tracks_for_playback.extend(tracks.iter().cloned());
            let mut rows = vec![QtDetailRow::album(&album, 0)];
            rows.push(QtDetailRow::route(
                format!("album:{id}"),
                "section",
                "Tracks",
                format!("{} track(s)", tracks.len()),
                1,
            ));
            rows.extend(tracks.iter().map(|track| QtDetailRow::track(track, 2)));
            QtNavDocument {
                title: album.name.to_string(),
                status: format!("Album | {name} | {}", album.release()),
                rows,
            }
        }
        QtNavTarget::Artist { id, name } => {
            let artist = api.get_artist(id).map_err(|err| err.to_string())?;
            let top_tracks = api
                .get_artist_top_tracks(id)
                .map_err(|err| err.to_string())?;
            let albums = api.get_artist_albums(id).map_err(|err| err.to_string())?;
            tracks_for_playback.extend(top_tracks.iter().cloned());
            let mut rows = vec![QtDetailRow::artist(&artist, 0)];
            rows.push(QtDetailRow::route(
                format!("artist:{id}:top-tracks"),
                "section",
                "Top Tracks",
                format!("{} track(s)", top_tracks.len()),
                1,
            ));
            rows.extend(top_tracks.iter().map(|track| QtDetailRow::track(track, 2)));
            rows.push(QtDetailRow::route(
                format!("artist:{id}:albums"),
                "section",
                "Albums",
                format!("{} album(s)", albums.albums.len()),
                1,
            ));
            rows.extend(
                albums
                    .albums
                    .iter()
                    .map(|album| QtDetailRow::album(album, 2)),
            );
            QtNavDocument {
                title: artist.name.to_string(),
                status: format!("Artist | {name}"),
                rows,
            }
        }
        QtNavTarget::Show { id, name } => {
            let show = api.get_show(id).map_err(|err| err.to_string())?.data;
            let episodes = api.get_show_episodes(id).map_err(|err| err.to_string())?;
            let mut rows = vec![QtDetailRow::show(&show, 0)];
            rows.push(QtDetailRow::route(
                format!("show:{id}:episodes"),
                "section",
                "Episodes",
                format!("{} episode(s)", episodes.len()),
                1,
            ));
            rows.extend(
                episodes
                    .iter()
                    .map(|episode| QtDetailRow::episode(episode, 2)),
            );
            QtNavDocument {
                title: show.name.to_string(),
                status: format!("Podcast | {name}"),
                rows,
            }
        }
        _ => QtNavDocument {
            title: target.title(),
            status: "Route loaded".to_string(),
            rows: Vec::new(),
        },
    };

    Ok(QtNavPayload {
        target,
        document,
        tracks_for_playback,
    })
}

fn home_rows(status: &str) -> Vec<QtDetailRow> {
    vec![
        QtDetailRow::route(
            "route:library",
            "library",
            "Library",
            "load saved content",
            0,
        ),
        QtDetailRow::route("route:search", "search", "Search", "press / to focus", 0),
        QtDetailRow::route("route:lyrics", "lyrics", "Lyrics", "parity placeholder", 0),
        QtDetailRow::route("route:login", "account", "Account", status, 0),
    ]
}

fn account_rows(authenticated: &bool, login_error: &str) -> Vec<QtDetailRow> {
    let mut rows = vec![QtDetailRow::route(
        "route:login",
        "account",
        if *authenticated {
            "Connected"
        } else {
            "Login required"
        },
        "use the account commands to login/logout",
        0,
    )];
    if !login_error.is_empty() {
        rows.push(QtDetailRow::route(
            "route:login:error",
            "error",
            "Last error",
            login_error,
            1,
        ));
    }
    rows
}

fn library_rows(app: &qobject::SpotixApp) -> Vec<QtDetailRow> {
    vec![
        QtDetailRow::route(
            "route:saved-tracks",
            "tracks",
            "Saved Tracks",
            format!(
                "{} loaded",
                parse_json_array::<QtTrack>(&app.saved_tracks_json().to_string()).len()
            ),
            0,
        ),
        QtDetailRow::route(
            "route:playlists",
            "playlists",
            "Playlists",
            format!(
                "{} loaded",
                parse_json_array::<QtPlaylist>(&app.playlists_json().to_string()).len()
            ),
            0,
        ),
        QtDetailRow::route(
            "route:saved-albums",
            "albums",
            "Albums",
            format!(
                "{} loaded",
                parse_json_array::<QtAlbum>(&app.saved_albums_json().to_string()).len()
            ),
            0,
        ),
        QtDetailRow::route(
            "route:shows",
            "shows",
            "Podcasts",
            format!(
                "{} loaded",
                parse_json_array::<QtShow>(&app.saved_shows_json().to_string()).len()
            ),
            0,
        ),
    ]
}

fn search_rows(json: &str) -> Vec<QtDetailRow> {
    let results = serde_json::from_str::<QtSearchResults>(json).unwrap_or_default();
    let mut rows = Vec::new();
    rows.push(QtDetailRow::route(
        "route:search:tracks",
        "section",
        "Tracks",
        format!("{} result(s)", results.tracks.len()),
        0,
    ));
    rows.extend(qt_track_rows(&results.tracks, 1));
    rows.push(QtDetailRow::route(
        "route:search:albums",
        "section",
        "Albums",
        format!("{} result(s)", results.albums.len()),
        0,
    ));
    rows.extend(results.albums.iter().map(|album| {
        QtDetailRow::route(
            format!("album:{}", album.id),
            "album",
            &album.title,
            &album.artist,
            1,
        )
    }));
    rows.push(QtDetailRow::route(
        "route:search:artists",
        "section",
        "Artists",
        format!("{} result(s)", results.artists.len()),
        0,
    ));
    rows.extend(results.artists.iter().map(|artist| {
        QtDetailRow::route(
            format!("artist:{}", artist.id),
            "artist",
            &artist.name,
            "",
            1,
        )
    }));
    rows.push(QtDetailRow::route(
        "route:search:playlists",
        "section",
        "Playlists",
        format!("{} result(s)", results.playlists.len()),
        0,
    ));
    rows.extend(results.playlists.iter().map(|playlist| {
        QtDetailRow::route(
            format!("playlist:{}", playlist.id),
            "playlist",
            &playlist.title,
            &playlist.owner,
            1,
        )
    }));
    rows.push(QtDetailRow::route(
        "route:search:shows",
        "section",
        "Podcasts",
        format!("{} result(s)", results.shows.len()),
        0,
    ));
    rows.extend(results.shows.iter().map(|show| {
        QtDetailRow::route(
            format!("show:{}", show.id),
            "show",
            &show.title,
            &show.publisher,
            1,
        )
    }));
    rows
}

fn qt_track_rows(tracks: &[QtTrack], depth: i32) -> Vec<QtDetailRow> {
    tracks
        .iter()
        .map(|track| QtDetailRow {
            id: format!("track:{}", track.id),
            kind: "track".to_string(),
            label: track.title.clone(),
            meta: format!("{} | {}", track.artist, track.album),
            depth,
            playable: true,
            expandable: false,
        })
        .collect()
}

fn loading_rows(title: &str) -> Vec<QtDetailRow> {
    vec![QtDetailRow::route(
        "route:loading",
        "status",
        format!("Loading {title}"),
        "waiting for Spotify data",
        0,
    )]
}

fn error_rows(error: &str) -> Vec<QtDetailRow> {
    vec![QtDetailRow::route(
        "route:error",
        "error",
        "Route load failed",
        error,
        0,
    )]
}

fn parse_json_array<T: serde::de::DeserializeOwned>(json: &str) -> Vec<T> {
    serde_json::from_str(json).unwrap_or_default()
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
