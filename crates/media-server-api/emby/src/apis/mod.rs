use std::error;
use std::fmt;

#[derive(Debug, Clone)]
pub struct ResponseContent<T> {
    pub status: reqwest::StatusCode,
    pub content: String,
    pub entity: Option<T>,
}

#[derive(Debug)]
pub enum Error<T> {
    Reqwest(reqwest::Error),
    Serde(serde_json::Error),
    Io(std::io::Error),
    ResponseError(ResponseContent<T>),
}

impl <T> fmt::Display for Error<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (module, e) = match self {
            Error::Reqwest(e) => ("reqwest", e.to_string()),
            Error::Serde(e) => ("serde", e.to_string()),
            Error::Io(e) => ("IO", e.to_string()),
            Error::ResponseError(e) => ("response", format!("status code {}", e.status)),
        };
        write!(f, "error in {}: {}", module, e)
    }
}

impl <T: fmt::Debug> error::Error for Error<T> {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(match self {
            Error::Reqwest(e) => e,
            Error::Serde(e) => e,
            Error::Io(e) => e,
            Error::ResponseError(_) => return None,
        })
    }
}

impl <T> From<reqwest::Error> for Error<T> {
    fn from(e: reqwest::Error) -> Self {
        Error::Reqwest(e)
    }
}

impl <T> From<serde_json::Error> for Error<T> {
    fn from(e: serde_json::Error) -> Self {
        Error::Serde(e)
    }
}

impl <T> From<std::io::Error> for Error<T> {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub fn urlencode<T: AsRef<str>>(s: T) -> String {
    ::url::form_urlencoded::byte_serialize(s.as_ref().as_bytes()).collect()
}

pub fn parse_deep_object(prefix: &str, value: &serde_json::Value) -> Vec<(String, String)> {
    if let serde_json::Value::Object(object) = value {
        let mut params = vec![];

        for (key, value) in object {
            match value {
                serde_json::Value::Object(_) => params.append(&mut parse_deep_object(
                    &format!("{}[{}]", prefix, key),
                    value,
                )),
                serde_json::Value::Array(array) => {
                    for (i, value) in array.iter().enumerate() {
                        params.append(&mut parse_deep_object(
                            &format!("{}[{}][{}]", prefix, key, i),
                            value,
                        ));
                    }
                },
                serde_json::Value::String(s) => params.push((format!("{}[{}]", prefix, key), s.clone())),
                _ => params.push((format!("{}[{}]", prefix, key), value.to_string())),
            }
        }

        return params;
    }

    unimplemented!("Only objects are supported with style=deepObject")
}

/// Internal use only
/// A content type supported by this client.
#[allow(dead_code)]
enum ContentType {
    Json,
    Text,
    Unsupported(String)
}

impl From<&str> for ContentType {
    fn from(content_type: &str) -> Self {
        if content_type.starts_with("application") && content_type.contains("json") {
            return Self::Json;
        } else if content_type.starts_with("text/plain") {
            return Self::Text;
        } else {
            return Self::Unsupported(content_type.to_string());
        }
    }
}

pub mod activity_log_service_api;
pub mod artists_service_api;
pub mod audio_service_api;
pub mod backup_api_api;
pub mod bif_service_api;
pub mod branding_service_api;
pub mod channel_service_api;
pub mod codec_parameter_service_api;
pub mod collection_service_api;
pub mod configuration_service_api;
pub mod connect_service_api;
pub mod content_service_api;
pub mod device_service_api;
pub mod display_preferences_service_api;
pub mod dlna_server_service_api;
pub mod dlna_service_api;
pub mod dynamic_hls_service_api;
pub mod encoding_info_service_api;
pub mod environment_service_api;
pub mod feature_service_api;
pub mod ffmpeg_options_service_api;
pub mod game_genres_service_api;
pub mod generic_ui_api_service_api;
pub mod genres_service_api;
pub mod hls_segment_service_api;
pub mod image_service_api;
pub mod instant_mix_service_api;
pub mod item_lookup_service_api;
pub mod item_refresh_service_api;
pub mod item_update_service_api;
pub mod items_service_api;
pub mod library_service_api;
pub mod library_structure_service_api;
pub mod live_stream_service_api;
pub mod live_tv_service_api;
pub mod localization_service_api;
pub mod media_info_service_api;
pub mod movies_service_api;
pub mod music_genres_service_api;
pub mod notifications_service_api;
pub mod official_rating_service_api;
pub mod open_api_service_api;
pub mod package_service_api;
pub mod party_service_api;
pub mod persons_service_api;
pub mod playlist_service_api;
pub mod playstate_service_api;
pub mod plugin_service_api;
pub mod remote_image_service_api;
pub mod scheduled_task_service_api;
pub mod sessions_service_api;
pub mod studios_service_api;
pub mod subtitle_options_service_api;
pub mod subtitle_service_api;
pub mod suggestions_service_api;
pub mod sync_service_api;
pub mod system_service_api;
pub mod tag_service_api;
pub mod tone_map_options_service_api;
pub mod trailers_service_api;
pub mod tv_shows_service_api;
pub mod universal_audio_service_api;
pub mod user_library_service_api;
pub mod user_notifications_service_api;
pub mod user_service_api;
pub mod user_views_service_api;
pub mod video_hls_service_api;
pub mod video_service_api;
pub mod videos_service_api;
pub mod web_app_service_api;

pub mod configuration;
