use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::Sender;
use spotix_core::{
    audio::{normalize::NormalizationLevel, output::DefaultAudioOutput},
    cache::Cache,
    cdn::Cdn,
    item_id::{ItemId, ItemIdType},
    player::{Player, PlayerCommand, PlayerEvent, item::PlaybackItem},
    session::SessionService,
};

use crate::{
    data::{Config, SearchTopic, Track},
    webapi::WebApi,
};

#[derive(Clone)]
pub struct PlaybackSnapshot {
    pub state: PlaybackUiState,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub progress: Duration,
    pub duration: Duration,
    pub volume: f64,
    pub queue_summary: String,
    pub status: String,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            state: PlaybackUiState::Stopped,
            title: "Nothing playing".to_string(),
            artist: "Select a track to start playback".to_string(),
            album: String::new(),
            progress: Duration::ZERO,
            duration: Duration::ZERO,
            volume: 1.0,
            queue_summary: "Queue is empty".to_string(),
            status: "Playback service is idle".to_string(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum PlaybackUiState {
    Loading,
    Playing,
    Paused,
    Blocked,
    Stopped,
}

impl PlaybackUiState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Loading => "Loading",
            Self::Playing => "Playing",
            Self::Paused => "Paused",
            Self::Blocked => "Blocked",
            Self::Stopped => "Stopped",
        }
    }
}

#[derive(Clone)]
struct TrackMetadata {
    title: String,
    artist: String,
    album: String,
    duration: Duration,
}

pub struct QtPlaybackService {
    sender: Sender<PlayerEvent>,
    snapshot: Arc<Mutex<PlaybackSnapshot>>,
    metadata: Arc<Mutex<HashMap<ItemId, TrackMetadata>>>,
    _thread: JoinHandle<()>,
    _output: DefaultAudioOutput,
}

static PLAYBACK_SERVICE: OnceLock<Mutex<Option<QtPlaybackService>>> = OnceLock::new();

pub fn init(session: SessionService, config: &Config) {
    let service = match QtPlaybackService::new(session, config) {
        Ok(service) => service,
        Err(err) => {
            log::error!("qt playback: failed to initialize playback service: {err}");
            return;
        }
    };

    let store = PLAYBACK_SERVICE.get_or_init(|| Mutex::new(None));
    *store.lock().expect("qt playback service lock poisoned") = Some(service);
}

pub fn snapshot() -> PlaybackSnapshot {
    with_service(|service| service.snapshot()).unwrap_or_else(|| PlaybackSnapshot {
        status: "Playback service is not initialized".to_string(),
        ..PlaybackSnapshot::default()
    })
}

pub fn play_first_search_result(query: &str) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Enter a search term first".to_string());
    }
    if WebApi::global().is_rate_limited() {
        return Err("Spotify search is rate limited".to_string());
    }

    let results = WebApi::global()
        .search(query, &[SearchTopic::Track], 10)
        .map_err(|err| err.to_string())?;
    let track = results
        .tracks
        .iter()
        .find(|track| !matches!(track.is_playable, Some(false)))
        .cloned()
        .ok_or_else(|| "No playable tracks found".to_string())?;

    with_service(|service| service.play_tracks(vec![track], 0))
        .unwrap_or_else(|| Err("Playback service is not initialized".to_string()))
}

pub fn pause_or_resume() {
    send(PlayerCommand::PauseOrResume);
}

pub fn previous() {
    send(PlayerCommand::Previous);
}

pub fn next() {
    send(PlayerCommand::Next);
}

pub fn stop() {
    send(PlayerCommand::Stop);
}

pub fn seek(progress_ratio: f64) {
    let duration = snapshot().duration;
    if duration.is_zero() {
        return;
    }
    let clamped = progress_ratio.clamp(0.0, 1.0);
    send(PlayerCommand::Seek {
        position: duration.mul_f64(clamped),
    });
}

pub fn set_volume(volume: f64) {
    let volume = volume.clamp(0.0, 1.0);
    let _ = with_service(|service| {
        service.update_snapshot(|snapshot| {
            snapshot.volume = volume;
        });
    });
    send(PlayerCommand::SetVolume { volume });
}

pub fn register_tracks(tracks: impl IntoIterator<Item = Arc<Track>>) {
    let _ = with_service(|service| {
        service.register_tracks(tracks);
    });
}

pub fn play_track_id(id: &str) -> Result<String, String> {
    let item_id = ItemId::from_base62(id, ItemIdType::Track)
        .ok_or_else(|| format!("Invalid track id: {id}"))?;
    with_service(|service| service.play_track_id(item_id))
        .unwrap_or_else(|| Err("Playback service is not initialized".to_string()))
}

fn send(command: PlayerCommand) {
    let _ = with_service(|service| {
        service.send(command);
    });
}

fn with_service<T>(f: impl FnOnce(&QtPlaybackService) -> T) -> Option<T> {
    let store = PLAYBACK_SERVICE.get_or_init(|| Mutex::new(None));
    let guard = store.lock().expect("qt playback service lock poisoned");
    guard.as_ref().map(f)
}

impl QtPlaybackService {
    fn new(session: SessionService, config: &Config) -> Result<Self, String> {
        let output = DefaultAudioOutput::open().map_err(|err| err.to_string())?;
        let cache_dir =
            Config::cache_dir().ok_or_else(|| "No cache directory found".to_string())?;
        let proxy_url = Config::proxy();
        let player = Player::new(
            session.clone(),
            Cdn::new(session, proxy_url.as_deref()).map_err(|err| err.to_string())?,
            Cache::new(cache_dir).map_err(|err| err.to_string())?,
            config.playback(),
            &output,
            config.credentials_clone(),
        );
        let sender = player.sender();
        let snapshot = Arc::new(Mutex::new(PlaybackSnapshot {
            volume: config.volume,
            status: "Playback service is ready".to_string(),
            ..PlaybackSnapshot::default()
        }));
        let metadata = Arc::new(Mutex::new(HashMap::new()));
        let thread_snapshot = Arc::clone(&snapshot);
        let thread_metadata = Arc::clone(&metadata);

        let thread = thread::spawn(move || {
            Self::service_events(player, thread_snapshot, thread_metadata);
        });

        sender
            .send(PlayerEvent::Command(PlayerCommand::SetVolume {
                volume: config.volume,
            }))
            .map_err(|err| err.to_string())?;

        Ok(Self {
            sender,
            snapshot,
            metadata,
            _thread: thread,
            _output: output,
        })
    }

    fn snapshot(&self) -> PlaybackSnapshot {
        self.snapshot
            .lock()
            .expect("qt playback snapshot lock poisoned")
            .clone()
    }

    fn update_snapshot(&self, f: impl FnOnce(&mut PlaybackSnapshot)) {
        let mut snapshot = self
            .snapshot
            .lock()
            .expect("qt playback snapshot lock poisoned");
        f(&mut snapshot);
    }

    fn send(&self, command: PlayerCommand) {
        if let Err(err) = self.sender.send(PlayerEvent::Command(command)) {
            log::warn!("qt playback: failed to send player command: {err}");
        }
    }

    fn play_tracks(&self, tracks: Vec<Arc<Track>>, position: usize) -> Result<String, String> {
        if tracks.is_empty() {
            return Err("No tracks to play".to_string());
        }

        let mut metadata = self
            .metadata
            .lock()
            .expect("qt playback metadata lock poisoned");
        let items = tracks
            .iter()
            .map(|track| {
                metadata.insert(
                    track.id.0,
                    TrackMetadata {
                        title: track.name.to_string(),
                        artist: track.artist_names(),
                        album: track.album_name().to_string(),
                        duration: track.duration,
                    },
                );
                PlaybackItem {
                    item_id: track.id.0,
                    norm_level: NormalizationLevel::Track,
                }
            })
            .collect::<Vec<_>>();
        drop(metadata);

        let position = position.min(items.len().saturating_sub(1));
        self.sender
            .send(PlayerEvent::Command(PlayerCommand::LoadQueue {
                items,
                position,
            }))
            .map_err(|err| err.to_string())?;

        self.update_snapshot(|snapshot| {
            snapshot.queue_summary = format!("{} track(s) queued", tracks.len());
            snapshot.status = "Loading queue".to_string();
        });

        Ok(format!("Playing {}", tracks[position].name))
    }

    fn register_tracks(&self, tracks: impl IntoIterator<Item = Arc<Track>>) {
        let mut metadata = self
            .metadata
            .lock()
            .expect("qt playback metadata lock poisoned");
        for track in tracks {
            metadata.insert(
                track.id.0,
                TrackMetadata {
                    title: track.name.to_string(),
                    artist: track.artist_names(),
                    album: track.album_name().to_string(),
                    duration: track.duration,
                },
            );
        }
    }

    fn play_track_id(&self, item_id: ItemId) -> Result<String, String> {
        let title = self
            .metadata
            .lock()
            .expect("qt playback metadata lock poisoned")
            .get(&item_id)
            .map(|metadata| metadata.title.clone())
            .unwrap_or_else(|| item_id.to_base62());

        self.sender
            .send(PlayerEvent::Command(PlayerCommand::LoadAndPlay {
                item: PlaybackItem {
                    item_id,
                    norm_level: NormalizationLevel::Track,
                },
            }))
            .map_err(|err| err.to_string())?;

        self.update_snapshot(|snapshot| {
            snapshot.status = "Loading track".to_string();
            snapshot.queue_summary = "Single track playback".to_string();
        });

        Ok(format!("Playing {title}"))
    }

    fn service_events(
        mut player: Player,
        snapshot: Arc<Mutex<PlaybackSnapshot>>,
        metadata: Arc<Mutex<HashMap<ItemId, TrackMetadata>>>,
    ) {
        for event in player.receiver() {
            Self::update_from_event(&snapshot, &metadata, &event);
            player.handle(event);
        }
    }

    fn update_from_event(
        snapshot: &Arc<Mutex<PlaybackSnapshot>>,
        metadata: &Arc<Mutex<HashMap<ItemId, TrackMetadata>>>,
        event: &PlayerEvent,
    ) {
        let mut snapshot = snapshot.lock().expect("qt playback snapshot lock poisoned");
        match event {
            PlayerEvent::Loading { item } => {
                snapshot.state = PlaybackUiState::Loading;
                snapshot.status = "Loading track".to_string();
                Self::apply_metadata(&mut snapshot, metadata, item.item_id);
            }
            PlayerEvent::Playing { path, position } => {
                snapshot.state = PlaybackUiState::Playing;
                snapshot.progress = *position;
                snapshot.status = "Playing".to_string();
                Self::apply_metadata(&mut snapshot, metadata, path.item_id);
            }
            PlayerEvent::Pausing { path, position } => {
                snapshot.state = PlaybackUiState::Paused;
                snapshot.progress = *position;
                snapshot.status = "Paused".to_string();
                Self::apply_metadata(&mut snapshot, metadata, path.item_id);
            }
            PlayerEvent::Resuming { path, position } | PlayerEvent::Position { path, position } => {
                snapshot.state = PlaybackUiState::Playing;
                snapshot.progress = *position;
                snapshot.status = "Playing".to_string();
                Self::apply_metadata(&mut snapshot, metadata, path.item_id);
            }
            PlayerEvent::Blocked { path, position } => {
                snapshot.state = PlaybackUiState::Blocked;
                snapshot.progress = *position;
                snapshot.status = "Buffering".to_string();
                Self::apply_metadata(&mut snapshot, metadata, path.item_id);
            }
            PlayerEvent::Stopped => {
                snapshot.state = PlaybackUiState::Stopped;
                snapshot.progress = Duration::ZERO;
                snapshot.status = "Stopped".to_string();
            }
            _ => {}
        }
    }

    fn apply_metadata(
        snapshot: &mut PlaybackSnapshot,
        metadata: &Arc<Mutex<HashMap<ItemId, TrackMetadata>>>,
        item_id: ItemId,
    ) {
        let metadata = metadata.lock().expect("qt playback metadata lock poisoned");
        if let Some(track) = metadata.get(&item_id) {
            snapshot.title.clone_from(&track.title);
            snapshot.artist.clone_from(&track.artist);
            snapshot.album.clone_from(&track.album);
            snapshot.duration = track.duration;
        }
    }
}
