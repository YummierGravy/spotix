use crate::data::{Album, Artist, Playlist, SearchResults, Show, Track};
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct QtImage {
    pub url: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct QtTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i32,
    pub image_url: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct QtAlbum {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub image_url: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct QtArtist {
    pub id: String,
    pub name: String,
    pub image_url: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct QtPlaylist {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub image_url: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct QtShow {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub image_url: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct QtSearchResults {
    pub query: String,
    pub tracks: Vec<QtTrack>,
    pub albums: Vec<QtAlbum>,
    pub artists: Vec<QtArtist>,
    pub playlists: Vec<QtPlaylist>,
    pub shows: Vec<QtShow>,
}

impl From<&Track> for QtTrack {
    fn from(track: &Track) -> Self {
        Self {
            id: track.id.0.to_base62(),
            title: track.name.to_string(),
            artist: track.artist_names(),
            album: track.album_name().to_string(),
            duration_ms: duration_ms(track.duration),
            image_url: track
                .album
                .as_ref()
                .and_then(|album| album.images.front())
                .map(|image| image.url.to_string())
                .unwrap_or_default(),
        }
    }
}

impl From<&Album> for QtAlbum {
    fn from(album: &Album) -> Self {
        Self {
            id: album.id.to_string(),
            title: album.name.to_string(),
            artist: album
                .artists
                .front()
                .map(|artist| artist.name.to_string())
                .unwrap_or_default(),
            image_url: album
                .images
                .front()
                .map(|image| image.url.to_string())
                .unwrap_or_default(),
        }
    }
}

impl From<&Artist> for QtArtist {
    fn from(artist: &Artist) -> Self {
        Self {
            id: artist.id.to_string(),
            name: artist.name.to_string(),
            image_url: artist
                .images
                .front()
                .map(|image| image.url.to_string())
                .unwrap_or_default(),
        }
    }
}

impl From<&Playlist> for QtPlaylist {
    fn from(playlist: &Playlist) -> Self {
        Self {
            id: playlist.id.to_string(),
            title: playlist.name.to_string(),
            owner: playlist.owner.display_name.to_string(),
            image_url: playlist
                .images
                .as_ref()
                .and_then(|images| images.front())
                .map(|image| image.url.to_string())
                .unwrap_or_default(),
        }
    }
}

impl From<&Show> for QtShow {
    fn from(show: &Show) -> Self {
        Self {
            id: show.id.to_string(),
            title: show.name.to_string(),
            publisher: show.publisher.to_string(),
            image_url: show
                .images
                .front()
                .map(|image| image.url.to_string())
                .unwrap_or_default(),
        }
    }
}

impl From<&SearchResults> for QtSearchResults {
    fn from(results: &SearchResults) -> Self {
        Self {
            query: results.query.to_string(),
            tracks: results
                .tracks
                .iter()
                .map(|track| QtTrack::from(&**track))
                .collect(),
            albums: results
                .albums
                .iter()
                .map(|album| QtAlbum::from(&**album))
                .collect(),
            artists: results.artists.iter().map(QtArtist::from).collect(),
            playlists: results.playlists.iter().map(QtPlaylist::from).collect(),
            shows: results
                .shows
                .iter()
                .map(|show| QtShow::from(&**show))
                .collect(),
        }
    }
}

fn duration_ms(duration: std::time::Duration) -> i32 {
    duration.as_millis().min(i32::MAX as u128) as i32
}
