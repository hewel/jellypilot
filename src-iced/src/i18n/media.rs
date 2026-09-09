use jellypilot_core::cards::{episode_card_code, episode_hero_code, is_episode_item};
use jellypilot_media_server::{VideoItemDetail, VideoLibraryItem, VideoShowDetail};

use super::{FluentValue, Localizer};

pub(crate) fn media_type(locale: Localizer, item_type: &str) -> String {
  let id = if item_type.eq_ignore_ascii_case("Movie") {
    "media-type-movie"
  } else if item_type.eq_ignore_ascii_case("Series") {
    "media-type-series"
  } else if item_type.eq_ignore_ascii_case("Episode") {
    "media-type-episode"
  } else if item_type.eq_ignore_ascii_case("Season") {
    "media-type-season"
  } else if item_type.eq_ignore_ascii_case("Video") {
    "media-type-video"
  } else if item_type.eq_ignore_ascii_case("BoxSet") {
    "media-type-box-set"
  } else if item_type.eq_ignore_ascii_case("MusicVideo") {
    "media-type-music-video"
  } else {
    return item_type.to_owned();
  };
  locale.text(id)
}

pub(crate) fn item_caption(locale: Localizer, item: &VideoLibraryItem) -> String {
  let kind = media_type(locale, &item.item_type);
  match item.production_year {
    Some(year) => locale.format(
      "media-item-caption",
      &[("year", year.into()), ("type", kind.into())],
    ),
    None => kind,
  }
}

pub(crate) fn series_year_range(
  locale: Localizer,
  production_year: Option<i32>,
  end_year: Option<i32>,
  continuing: bool,
) -> String {
  let Some(year) = production_year else {
    return String::new();
  };
  if continuing {
    locale.format("media-year-ongoing", &[("year", year.into())])
  } else if let Some(end) = end_year.filter(|end| *end != year) {
    locale.format(
      "media-year-range",
      &[("year", year.into()), ("end", end.into())],
    )
  } else {
    locale.format("media-year", &[("year", year.into())])
  }
}

pub(crate) fn card_subtitle(locale: Localizer, item: &VideoLibraryItem) -> String {
  if item.item_type.eq_ignore_ascii_case("Series") {
    series_year_range(
      locale,
      item.production_year,
      item.end_year,
      item.series_continuing,
    )
  } else if is_episode_item(item) {
    match episode_card_code(item) {
      Some(code) => locale.format(
        "media-episode-caption",
        &[("code", code.into()), ("name", item.name.as_str().into())],
      ),
      None => item.name.clone(),
    }
  } else {
    item.production_year.map_or_else(String::new, |year| {
      locale.format("media-year", &[("year", year.into())])
    })
  }
}

pub(crate) fn runtime_caption(locale: Localizer, runtime_seconds: f64) -> Option<String> {
  (runtime_seconds.is_finite() && runtime_seconds > 0.0).then(|| locale.duration(runtime_seconds))
}

/// Premiere dates are calendar dates; preserve the server's date rather than
/// shifting midnight across days when displaying in the desktop's time zone.
pub(crate) fn episode_premiere_date(locale: Localizer, iso: &str) -> Option<String> {
  let (date, _) = iso.split_once('T')?;
  let mut parts = date.split('-');
  let year = parts.next()?;
  let month = parts.next()?.parse::<u8>().ok()?;
  let day = parts.next()?.parse::<u8>().ok()?;
  if parts.next().is_some()
    || year.parse::<u16>().is_err()
    || !(1..=12).contains(&month)
    || !(1..=31).contains(&day)
  {
    return None;
  }
  Some(locale.format(
    "media-premiere-date",
    &[
      ("year", year.into()),
      ("month", month.to_string().into()),
      ("day", day.to_string().into()),
    ],
  ))
}

pub(crate) fn hero_metadata(locale: Localizer, item: &VideoLibraryItem) -> String {
  let runtime = item
    .runtime_seconds
    .and_then(|seconds| runtime_caption(locale, seconds));
  let episode = episode_hero_code(item);
  match (item.production_year, runtime, episode) {
    (Some(year), Some(runtime), Some(episode)) => locale.format(
      "media-hero-year-runtime-episode",
      &[
        ("year", year.into()),
        ("runtime", runtime.into()),
        ("episode", episode.into()),
      ],
    ),
    (Some(year), Some(runtime), None) => locale.format(
      "media-hero-year-runtime",
      &[("year", year.into()), ("runtime", runtime.into())],
    ),
    (Some(year), None, Some(episode)) => locale.format(
      "media-hero-year-episode",
      &[("year", year.into()), ("episode", episode.into())],
    ),
    (None, Some(runtime), Some(episode)) => locale.format(
      "media-hero-runtime-episode",
      &[("runtime", runtime.into()), ("episode", episode.into())],
    ),
    (Some(year), None, None) => locale.format("media-year", &[("year", year.into())]),
    (None, Some(runtime), None) => runtime,
    (None, None, Some(episode)) => episode,
    (None, None, None) => item_caption(locale, item),
  }
}

pub(crate) fn detail_metadata(locale: Localizer, detail: &VideoItemDetail) -> String {
  metadata(
    locale,
    detail.production_year,
    &detail.item_type,
    &detail.genres,
    detail.favorite,
  )
}

pub(crate) fn show_detail_metadata(locale: Localizer, detail: &VideoShowDetail) -> String {
  metadata(
    locale,
    detail.production_year,
    "Series",
    &detail.genres,
    detail.favorite,
  )
}

fn metadata(
  locale: Localizer,
  year: Option<i32>,
  item_type: &str,
  genres: &[String],
  favorite: bool,
) -> String {
  locale.format(
    "media-detail-metadata",
    &[
      ("has-year", if year.is_some() { "yes" } else { "no" }.into()),
      ("year", year.map_or(FluentValue::None, FluentValue::from)),
      ("type", media_type(locale, item_type).into()),
      (
        "has-genres",
        if genres.is_empty() { "no" } else { "yes" }.into(),
      ),
      ("genres", genres.join(", ").into()),
      ("favorite", if favorite { "yes" } else { "no" }.into()),
    ],
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_core::locale::UiLanguage;

  #[test]
  fn runtime_rejects_invalid_values_before_duration_formatting() {
    for seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -60.0, 0.0] {
      assert_eq!(runtime_caption(Localizer::default(), seconds), None);
    }
  }

  #[test]
  fn metadata_keeps_server_values_across_language_switches() {
    let genres = vec!["科幻 / Science Fiction".to_owned(), "Drama".to_owned()];
    for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
      let text = metadata(
        Localizer::new(language),
        Some(2024),
        "ServerCustomType",
        &genres,
        true,
      );
      assert!(text.contains("2024"));
      assert!(text.contains("ServerCustomType"));
      assert!(text.contains("科幻 / Science Fiction, Drama"));
    }
  }
}
