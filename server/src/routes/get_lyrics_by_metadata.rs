use axum::{extract::{Query, State}, Json};
use serde::{Deserialize,Serialize};
use std::{collections::VecDeque, sync::Arc, time::Duration};
use crate::{entities::{missing_track::MissingTrack, track::SimpleTrack}, errors::ApiError, repositories::track_repository::get_track_by_metadata, AppState};
use axum_macros::debug_handler;

#[derive(Deserialize)]
pub struct QueryParams {
  track_name: String,
  artist_name: String,
  album_name: String,
  duration: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackResponse {
  id: i64,
  name: Option<String>,
  track_name: Option<String>,
  artist_name: Option<String>,
  album_name: Option<String>,
  duration: Option<f64>,
  instrumental: bool,
  plain_lyrics: Option<String>,
  synced_lyrics: Option<String>,
}

#[debug_handler]
pub async fn route(Query(params): Query<QueryParams>, State(state): State<Arc<AppState>>) -> Result<Json<TrackResponse>, ApiError> {
  let maybe_track = {
    let mut conn = state.pool.get()?;
    get_track_by_metadata(
      &params.track_name,
      &params.artist_name,
      &params.album_name,
      params.duration,
      &mut conn,
    )?
  };

  match maybe_track {
    Some(track) => {
      Ok(Json(create_response(track)))
    }
    None => {
      let missing_track = MissingTrack {
        name: params.track_name.trim().to_owned(),
        artist_name: params.artist_name.trim().to_owned(),
        album_name: params.album_name.trim().to_owned(),
        duration: params.duration,
      };

      {
        let mut cache_lock = state.cache.lock().await;
        let mut queue_lock = state.queue.lock().await;
        let is_queued_recently = cache_lock.contains_key(&format!("missing_track:{}", missing_track));

        if !is_queued_recently {
          cache_lock.insert(format!("missing_track:{}", missing_track), "1".to_owned(), Duration::from_secs(60 * 5));
          send_to_queue(missing_track, &mut *queue_lock).await;
        }
      }

      Err(ApiError::TrackNotFoundError)
    }
  }
}

fn create_response(track: SimpleTrack) -> TrackResponse {
  let plain_lyrics = match track.last_lyrics {
    Some(ref lyrics) => lyrics.plain_lyrics.to_owned(),
    None => None
  };

  let synced_lyrics = match track.last_lyrics {
    Some(ref lyrics) => lyrics.synced_lyrics.to_owned(),
    None => None
  };

  let instrumental = match track.last_lyrics {
    Some(ref lyrics) => lyrics.instrumental.to_owned(),
    None => false
  };

  TrackResponse {
    id: track.id,
    name: track.name.to_owned(),
    track_name: track.name.to_owned(),
    artist_name: track.artist_name.to_owned(),
    album_name: track.album_name.to_owned(),
    duration: track.duration,
    instrumental,
    plain_lyrics,
    synced_lyrics,
  }
}

async fn send_to_queue(missing_track: MissingTrack, queue: &mut VecDeque<MissingTrack>) {
  tracing::info!(
    message = format!("sending missing track to queue"),
    track_name = missing_track.name,
    artist_name = missing_track.artist_name,
    album_name = missing_track.album_name,
    duration = missing_track.duration,
  );
  queue.push_back(missing_track);
}
