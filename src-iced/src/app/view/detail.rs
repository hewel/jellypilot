mod overview;

use overview::overview_layout;

use super::image_observer::{observe_image, ImageAxis};
use crate::app::artwork::{ArtworkSurface, ImageSpec, ImageStatus};
use crate::app::detail::{
  card_image_spec, detail_next_up_key, episode_image_spec, hero_image_spec, DETAIL_BACKDROP_KEY,
  DETAIL_LOGO_KEY,
};
use crate::app::message::{DetailMessage, Message, PlaybackMessage};
use crate::app::state::{State, UserDataActionKind};
use crate::i18n::media::{detail_metadata, show_detail_metadata};
use crate::i18n::{Localizer, UiText};
use iced::widget::image::Image;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
  button, column, container, row, scrollable, space, stack, text, Column, Row, Stack,
};
use iced::{gradient, padding};
use iced::{Alignment, Background, ContentFit, Degrees, Element, Fill, Length, Pixels};
use jellypilot_core::cards::logo_display_size;
use jellypilot_core::detail::{detail_episode_key, detail_similar_key, DetailContent};
use jellypilot_core::LoadState;
use jellypilot_media_server::{
  VideoItemDetail, VideoLibraryItem, VideoMediaInfo, VideoSeason, VideoShowDetail, VideoStreamInfo,
};
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;
use jellypilot_ui::fonts::{BODY_FONT, DISPLAY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::control_button;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::skeleton::{
  skeleton_block, skeleton_block_with_radius, skeleton_panel,
};
use jellypilot_ui::{full_radius, poster_card, rounded_image};

/// Jellyfin hero backdrops are 16:9; derive the hero height from its width so
/// the Backdrop renders uncropped.
fn hero_height_for_width(width: f32) -> f32 {
  (width * 9.0 / 16.0).max(1.0)
}
const HERO_LOGO_HEIGHT: f32 = 96.0;
const EPISODE_HERO_LOGO_HEIGHT: f32 = 64.0;
const EPISODE_ART_WIDTH: f32 = 240.0;
const EPISODE_ART_HEIGHT: f32 = 135.0;
const EPISODE_ACTION_WIDTH: f32 = 92.0;
const OVERVIEW_TEXT_SIZE: f32 = 15.0;
const OVERVIEW_COLLAPSED_LINES: f32 = 4.0;
const EPISODE_OVERVIEW_TEXT_SIZE: f32 = 13.0;
const EPISODE_OVERVIEW_COLLAPSED_LINES: f32 = 3.0;
const SIMILAR_CARD_WIDTH: f32 = 160.0;
const SIMILAR_CARD_HEIGHT: f32 = 240.0;
const SIMILAR_SCROLL_HEIGHT: f32 = 302.0;
const HERO_SCRIM_TOP_ALPHA: f32 = 0.0;
/// Non-overview copy below the overview text: the More/Less toggle, actions
/// row, spacings, and the hero's bottom padding.
const SCRIM_TAIL_CHROME: f32 = 10.0;
const SCRIM_TAIL_ALPHA: f32 = 0.97;

pub fn view(state: &State) -> Element<'_, Message> {
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .data
    .content
  {
    LoadState::Idle | LoadState::Loading => detail_skeleton(state, skeleton_phase, reduced_motion),
    LoadState::Failed(error) => detail_failure(state, error),
    LoadState::Ready(content) => detail_ready(state, content, skeleton_phase, reduced_motion),
  }
}

fn detail_ready<'a>(
  state: &'a State,
  content: &'a DetailContent,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let mut page = Column::new().width(Fill).spacing(TOKENS.spacing.s6);
  page = page.push(match content {
    DetailContent::Item(item) => item_hero(state, item, skeleton_phase, reduced_motion),
    DetailContent::Show(show) => show_hero(state, show, skeleton_phase, reduced_motion),
  });

  match content {
    DetailContent::Item(item) => {
      page = page.push(summary(
        state.palette(),
        state.kernel.locale,
        &item.genres,
        &item.metadata.creators,
        &item.metadata.cast,
      ));
      if item.media_info.is_some() {
        page = page.push(media_info_section(
          state.palette(),
          state.kernel.locale,
          item,
        ));
      }
      if item.item_type.eq_ignore_ascii_case("episode") {
        page = page.push(neighbor_section(state, skeleton_phase, reduced_motion));
      } else if item.item_type.eq_ignore_ascii_case("movie") {
        page = page.push(similar_section(state, skeleton_phase, reduced_motion));
      }
    }
    DetailContent::Show(show) => {
      page = page.push(summary(
        state.palette(),
        state.kernel.locale,
        &show.genres,
        &show.metadata.creators,
        &show.metadata.cast,
      ));
      if let Some(next) = &show.next_episode {
        page = page.push(next_up_section(state, next, skeleton_phase, reduced_motion));
      }
      page = page.push(seasons_section(state, show, skeleton_phase, reduced_motion));
      page = page.push(similar_section(state, skeleton_phase, reduced_motion));
    }
  }

  scrollable(page.padding([TOKENS.spacing.s8, TOKENS.spacing.s6]))
    .id(iced::widget::Id::new("detail-page"))
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn item_hero<'a>(
  state: &'a State,
  item: &'a jellypilot_media_server::VideoItemDetail,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let playback_label = if item.can_resume {
    state.t("detail-resume")
  } else {
    state.t("detail-play")
  };
  let position = if item.can_resume {
    PlaybackStartPosition::Resume
  } else {
    PlaybackStartPosition::Beginning
  };
  hero(
    state,
    HeroContent {
      id: &item.id,
      name: &item.name,
      metadata: item_metadata(state.kernel.locale, item),
      overview: item.overview.as_deref(),
      playback_label,
      playback: item
        .can_play
        .then(|| (Playable::Detail(item.clone()), position)),
      played: item.played,
      favorite: item.favorite,
      is_episode: item.item_type.eq_ignore_ascii_case("episode"),
    },
    skeleton_phase,
    reduced_motion,
  )
}

fn show_hero<'a>(
  state: &'a State,
  show: &'a VideoShowDetail,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let playback_label = show.next_episode.as_ref().map_or_else(
    || state.t("detail-play"),
    |episode| {
      state.format(
        if has_resume(episode) {
          "detail-continue-episode"
        } else {
          "detail-play-episode"
        },
        &[(
          "episode",
          episode_label(state.kernel.locale, episode).into(),
        )],
      )
    },
  );
  let playback = show.next_episode.as_ref().map(|episode| {
    (
      Playable::Library(episode.clone()),
      if has_resume(episode) {
        PlaybackStartPosition::Resume
      } else {
        PlaybackStartPosition::Beginning
      },
    )
  });
  hero(
    state,
    HeroContent {
      id: &show.id,
      name: &show.name,
      metadata: show_metadata(state.kernel.locale, show),
      overview: show.overview.as_deref(),
      playback_label,
      playback,
      played: show.played,
      favorite: show.favorite,
      is_episode: false,
    },
    skeleton_phase,
    reduced_motion,
  )
}

struct HeroContent<'a> {
  id: &'a str,
  name: &'a str,
  metadata: String,
  overview: Option<&'a str>,
  playback_label: String,
  playback: Option<(Playable, PlaybackStartPosition)>,
  played: bool,
  favorite: bool,
  is_episode: bool,
}

fn hero<'a>(
  state: &'a State,
  content: HeroContent<'a>,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  overview_layout(
    content.overview,
    TOKENS.spacing.s6 * 2.0,
    OVERVIEW_TEXT_SIZE,
    move |width, measured_height| {
      hero_at_width(
        state,
        &content,
        width,
        measured_height,
        skeleton_phase,
        reduced_motion,
      )
    },
  )
}

fn hero_at_width<'a>(
  state: &'a State,
  content: &HeroContent<'a>,
  width: f32,
  measured_height: f32,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  let name = content.name;
  let overview = content.overview.filter(|value| !value.trim().is_empty());
  let collapsed_height = overview_collapsed_height(OVERVIEW_TEXT_SIZE, OVERVIEW_COLLAPSED_LINES);
  let overview_expandable = overview_is_expandable(
    measured_height,
    OVERVIEW_TEXT_SIZE,
    OVERVIEW_COLLAPSED_LINES,
  );
  let overview_expanded = overview_expandable
    && state
      .full
      .as_ref()
      .expect("FullUi required")
      .detail
      .data
      .overview_expanded;
  let base_hero_height = hero_height_for_width(width);
  let hero_height = if overview_expanded {
    base_hero_height + (measured_height - collapsed_height).max(0.0)
  } else {
    base_hero_height + 1.0
  };

  // The Backdrop keeps its 16:9 height when the overview expands; only the
  // scrim stretches, so the extra text lands on a solid gradient tail.
  let backdrop = if let Some(spec) = hero_spec(state, DETAIL_BACKDROP_KEY) {
    artwork(
      state,
      Some(spec),
      name,
      (Fill, Length::Fixed(base_hero_height)),
      ArtworkKind::Hero,
      skeleton_phase,
      reduced_motion,
    )
  } else {
    space::vertical()
      .width(Fill)
      .height(Length::Fixed(base_hero_height))
      .into()
  };
  // The scrim always uses the expanded-style solid tail: a near-opaque zone
  // rising to the visible overview's top, so no copy sits on bare image in
  // either the collapsed or the expanded state.
  let visible_overview_height = if overview.is_none() {
    0.0
  } else if overview_expanded {
    measured_height
  } else {
    collapsed_height
  };
  let overview_top =
    ((hero_height - visible_overview_height - SCRIM_TAIL_CHROME) / hero_height).clamp(0.0, 1.0);
  let gradient = gradient::Linear::new(Degrees(180.0))
    .add_stop(
      0.0,
      palette.colors.background.scale_alpha(HERO_SCRIM_TOP_ALPHA),
    )
    .add_stop(
      overview_top,
      palette.colors.background.scale_alpha(SCRIM_TAIL_ALPHA),
    )
    .add_stop(1.0, palette.colors.background.scale_alpha(1.0));
  let scrim = container(space::vertical())
    .width(Fill)
    .height(hero_height)
    .style(move |_| {
      iced::widget::container::Style::default().background(Background::Gradient(gradient.into()))
    });

  let back_enabled = !state.shell.navigation_stack.is_empty();
  let back = control_button(
    Some(Icon::ChevronLeft),
    Some(state.t("detail-back")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .label_size(14.0)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(back_enabled.then_some(Message::Detail(DetailMessage::Back)));
  let mut copy = Column::new()
    .spacing(TOKENS.spacing.s3)
    .width(Fill)
    .push(hero_title(state, name, content.is_episode))
    .push(
      text(content.metadata.clone())
        .size(15)
        .color(palette.text.secondary),
    );

  if let Some(overview) = overview {
    if overview_expandable && !overview_expanded {
      copy = copy.push(
        container(
          text(overview)
            .size(OVERVIEW_TEXT_SIZE)
            .color(palette.text.body),
        )
        .width(Fill)
        .height(collapsed_height)
        .clip(true),
      );
    } else {
      copy = copy.push(
        text(overview)
          .size(OVERVIEW_TEXT_SIZE)
          .color(palette.text.body),
      );
    }
    if overview_expandable {
      let (overview_label, overview_icon) = if overview_expanded {
        (state.t("detail-less"), Icon::ChevronUp)
      } else {
        (state.t("detail-more"), Icon::ChevronDown)
      };
      copy = copy.push(
        control_button(
          Some(overview_icon),
          Some(overview_label),
          ButtonVariant::Text,
        )
        .icon_size(IconSize::Xs)
        .trailing_icon(true)
        .spacing(TOKENS.spacing.s1)
        .padding([5, 8])
        .on_press(Message::Detail(DetailMessage::OverviewToggled)),
      );
    }
  }
  copy = copy.push(detail_actions(
    state,
    content.playback_label.clone(),
    content.playback.clone(),
    content.id,
    content.played,
    content.favorite,
  ));
  let foreground = column![
    back,
    container(copy)
      .width(Fill)
      .height(Fill)
      .align_y(Alignment::End),
  ]
  .spacing(TOKENS.spacing.s5)
  .padding(TOKENS.spacing.s6)
  .width(Fill)
  .height(hero_height);

  // A Stack's base layer inherits the stack's fixed height as both min and
  // max, which would stretch the Backdrop on overview expansion; keeping the
  // Backdrop in an under-layer preserves its fixed 16:9 height.
  Stack::new()
    .push_under(backdrop)
    .push(scrim)
    .push(foreground)
    .width(Fill)
    .height(hero_height)
    .into()
}

fn hero_title<'a>(state: &'a State, name: &'a str, is_episode: bool) -> Element<'a, Message> {
  let title = || -> Element<'a, Message> {
    text(name)
      .font(DISPLAY_FONT)
      .size(45)
      .color(state.palette().text.heading)
      .into()
  };
  let Some(logo) = detail_logo(
    state,
    if is_episode {
      EPISODE_HERO_LOGO_HEIGHT
    } else {
      HERO_LOGO_HEIGHT
    },
  ) else {
    return observe_detail(
      title(),
      state,
      hero_spec(state, DETAIL_LOGO_KEY),
      ImageAxis::Vertical,
    );
  };
  let logo = observe_detail(
    logo,
    state,
    hero_spec(state, DETAIL_LOGO_KEY),
    ImageAxis::Vertical,
  );
  if is_episode {
    column![logo, title()]
      .spacing(TOKENS.spacing.s2)
      .align_x(Alignment::Start)
      .into()
  } else {
    logo
  }
}

fn detail_logo(state: &State, max_height: f32) -> Option<Element<'_, Message>> {
  let cell = state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .artwork
    .get(DETAIL_LOGO_KEY)?;
  let handle = cell.handle()?.clone();
  // See the home hero: the shadow canvas margin is vertical/right-only, so
  // indent the logo on top only and keep its left edge flush with the text.
  let dims = cell.dims().filter(|&(w, h)| w > 0 && h > 0);
  let (logo_width, logo_height) = dims
    .map(|(w, h)| logo_display_size(w, h, max_height))
    .unwrap_or((0.0, max_height));
  let logo_image = Image::new(handle)
    .content_fit(ContentFit::Contain)
    .height(logo_height)
    .width(if logo_width > 0.0 {
      Length::Fixed(logo_width)
    } else {
      Length::Fit
    });
  let logo = container(logo_image)
    .padding(iced::Padding {
      top: logo_height / 4.0,
      ..iced::Padding::ZERO
    })
    .width(Fill)
    .align_x(Alignment::Start);
  let Some(shadow) = cell.logo_shadow() else {
    return Some(logo.into());
  };
  let shadow_height = logo_height * 3.0 / 2.0;
  let shadow_width = dims.map(|(w, h)| (w as f32 + h as f32 / 2.0) * (logo_height / h as f32));
  let shadow_image = Image::new(shadow.clone())
    .content_fit(ContentFit::Contain)
    .height(shadow_height)
    .width(if let Some(width) = shadow_width {
      Length::Fixed(width)
    } else {
      Length::Fit
    });
  Some(
    stack![
      container(shadow_image)
        .width(Fill)
        .align_x(Alignment::Start),
      logo,
    ]
    .into(),
  )
}

fn detail_actions<'a>(
  state: &'a State,
  playback_label: String,
  playback_target: Option<(Playable, PlaybackStartPosition)>,
  item_id: &str,
  played: bool,
  favorite: bool,
) -> Element<'a, Message> {
  let playback_enabled = playback_target.is_some() && state.playback.view.engine_available;
  let playback = control_button(
    Some(Icon::Play),
    Some(playback_label),
    ButtonVariant::Primary,
  )
  .spacing(TOKENS.spacing.s2)
  .padding([8, 16])
  .on_press_maybe(
    playback_target
      .filter(|_| playback_enabled)
      .map(|(item, position)| playback_message(state, item, position)),
  );
  let any_busy = state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .data
    .user_data_busy
    .is_some();
  // The favorited heart stays rose across hover (fixed `favorite` accent);
  // the unfavorited heart is an ordinary Tonal control on `control_button`.
  let favorite_button: Element<'_, Message> = if favorite {
    let mut color = state.palette().colors.favorite;
    if any_busy {
      color.a *= 0.5;
    }
    button(
      row![
        icon_with_color(Icon::HeartFilled, IconSize::Md, color),
        text(state.t("detail-favorited")),
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center),
    )
    .padding([8, 14])
    .on_press_maybe((!any_busy).then_some(Message::Detail(DetailMessage::FavoriteToggled)))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::TonalActive)
    })
    .into()
  } else {
    control_button(
      Some(Icon::Heart),
      Some(state.t("detail-favorite")),
      ButtonVariant::Tonal,
    )
    .spacing(TOKENS.spacing.s2)
    .padding([8, 14])
    .on_press_maybe((!any_busy).then_some(Message::Detail(DetailMessage::FavoriteToggled)))
    .into()
  };
  let watchlist = &state.full.as_ref().expect("FullUi required").personal_lists;
  let watchlisted = watchlist.watchlist_ids.contains(item_id);
  let watchlist_busy = watchlist.busy_items.contains(item_id);
  let watchlist_button = control_button(
    Some(if watchlisted {
      Icon::BookmarkFilled
    } else {
      Icon::Bookmark
    }),
    Some(if watchlisted {
      state.t("detail-watchlisted")
    } else {
      state.t("detail-watchlist-add")
    }),
    if watchlisted {
      ButtonVariant::TonalActive
    } else {
      ButtonVariant::Tonal
    },
  )
  .spacing(TOKENS.spacing.s2)
  .padding([8, 14])
  .on_press_maybe((!watchlist_busy).then_some(Message::Detail(DetailMessage::WatchlistToggled)));
  let (played_icon, played_label, played_variant) = if played {
    (
      Icon::CircleCheck,
      state.t("detail-played"),
      ButtonVariant::TonalActive,
    )
  } else {
    (
      Icon::Circle,
      state.t("detail-mark-played"),
      ButtonVariant::Tonal,
    )
  };
  let played_button = control_button(Some(played_icon), Some(played_label), played_variant)
    .spacing(TOKENS.spacing.s2)
    .padding([8, 14])
    .on_press_maybe((!any_busy).then_some(Message::Detail(DetailMessage::PlayedToggled)));
  let mut actions = Row::new()
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center)
    .push(playback)
    .push(favorite_button)
    .push(watchlist_button)
    .push(played_button);
  if let Some(kind) = state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .data
    .user_data_busy
  {
    actions = actions.push(
      text(match kind {
        UserDataActionKind::Favorite => state.t("detail-updating-favorite"),
        UserDataActionKind::Played => state.t("detail-updating-played"),
      })
      .size(13)
      .color(state.palette().text.metadata),
    );
  }
  let mut content = Column::new().spacing(TOKENS.spacing.s2).push(actions);
  if let Some(error) = &state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .data
    .user_data_error
  {
    content = content.push(
      text(state.kernel.locale.message(error))
        .size(13)
        .color(state.palette().colors.error),
    );
  }
  if let Some(error) = &watchlist.mutation_error {
    content = content.push(
      text(state.kernel.locale.message(error))
        .size(13)
        .color(state.palette().colors.error),
    );
  }
  content.into()
}

fn summary<'a>(
  palette: &ThemePalette,
  locale: Localizer,
  genres: &'a [String],
  creators: &'a [String],
  cast: &'a [String],
) -> Element<'a, Message> {
  if genres.is_empty() && creators.is_empty() && cast.is_empty() {
    return space::vertical().height(0).into();
  }
  let mut columns = Row::new().spacing(TOKENS.spacing.s8).width(Fill);
  if !genres.is_empty() {
    columns = columns.push(summary_column(
      palette,
      locale.text("detail-genres"),
      genres.join(" • "),
    ));
  }
  if !creators.is_empty() {
    columns = columns.push(summary_column(
      palette,
      locale.text("detail-creators"),
      limited_people(locale, creators, 2),
    ));
  }
  if !cast.is_empty() {
    columns = columns.push(summary_column(
      palette,
      locale.text("detail-cast"),
      limited_people(locale, cast, 4),
    ));
  }
  container(columns)
    .padding(TOKENS.spacing.s5)
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
    .into()
}

fn summary_column(
  palette: &ThemePalette,
  label: String,
  values: String,
) -> Element<'static, Message> {
  column![
    text(label).size(12).color(palette.text.metadata),
    text(values).size(14).color(palette.text.secondary),
  ]
  .spacing(TOKENS.spacing.s2)
  .width(Fill)
  .into()
}

fn media_info_section(
  palette: &'static ThemePalette,
  locale: Localizer,
  item: &VideoItemDetail,
) -> Element<'static, Message> {
  let Some(info) = &item.media_info else {
    return space::vertical().height(0).into();
  };
  let mut rows = Column::new().spacing(TOKENS.spacing.s3).width(Fill);
  if let Some(video) = video_info_label(info) {
    rows = rows.push(media_info_row(palette, locale.text("detail-video"), video));
  }
  for stream in &info.audio_streams {
    if let Some(audio) = audio_info_label(locale, stream) {
      rows = rows.push(media_info_row(palette, locale.text("detail-audio"), audio));
    }
  }
  let subtitles = info
    .subtitle_streams
    .iter()
    .filter_map(subtitle_info_label)
    .collect::<Vec<_>>()
    .join(", ");
  if !subtitles.is_empty() {
    rows = rows.push(media_info_row(
      palette,
      locale.text("detail-subtitles"),
      subtitles,
    ));
  }
  if let Some(container_name) = nonempty(info.container.as_deref()) {
    rows = rows.push(media_info_row(
      palette,
      locale.text("detail-container"),
      container_name.to_owned(),
    ));
  }
  if let Some(size_bytes) = info.size_bytes {
    rows = rows.push(media_info_row(
      palette,
      locale.text("detail-size"),
      humanized_size(size_bytes),
    ));
  }
  if let Some(bitrate_bps) = info.bitrate_bps {
    rows = rows.push(media_info_row(
      palette,
      locale.text("detail-bitrate"),
      format!("{:.1} Mbps", bitrate_bps as f64 / 1_000_000.0),
    ));
  }
  let title = section_title(palette, locale.text("detail-media-info"));
  column![
    title,
    container(rows)
      .padding(TOKENS.spacing.s5)
      .width(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas)),
  ]
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn media_info_row(
  palette: &'static ThemePalette,
  label: String,
  value: String,
) -> Element<'static, Message> {
  row![
    text(label).size(12).color(palette.text.metadata).width(120),
    text(value).size(14).color(palette.text.secondary),
  ]
  .spacing(TOKENS.spacing.s4)
  .width(Fill)
  .into()
}

fn video_info_label(info: &VideoMediaInfo) -> Option<String> {
  let mut values = Vec::new();
  if let Some(height) = info.video_height {
    values.push(format!("{height}p"));
  }
  if let Some(codec) = nonempty(info.video_codec.as_deref()) {
    values.push(codec.to_owned());
  }
  if let Some(range) = nonempty(info.video_range.as_deref()) {
    values.push(range.to_owned());
  }
  (!values.is_empty()).then(|| values.join(" "))
}

fn audio_info_label(locale: Localizer, stream: &VideoStreamInfo) -> Option<String> {
  if let Some(title) = nonempty(stream.display_title.as_deref()) {
    return Some(title.to_owned());
  }
  let mut values = Vec::new();
  if let Some(codec) = nonempty(stream.codec.as_deref()) {
    values.push(codec.to_owned());
  }
  if let Some(language) = nonempty(stream.language.as_deref()) {
    values.push(language.to_owned());
  }
  if let Some(channels) = stream.channels {
    values.push(locale.format("detail-channels", &[("count", channels.into())]));
  }
  (!values.is_empty()).then(|| values.join(" "))
}

fn subtitle_info_label(stream: &VideoStreamInfo) -> Option<String> {
  let values = [
    nonempty(stream.language.as_deref()),
    nonempty(stream.codec.as_deref()),
  ]
  .into_iter()
  .flatten()
  .collect::<Vec<_>>();
  (!values.is_empty()).then(|| values.join(" "))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
  value.filter(|value| !value.trim().is_empty())
}

fn humanized_size(size_bytes: u64) -> String {
  const MIB: f64 = 1024.0 * 1024.0;
  const GIB: f64 = MIB * 1024.0;
  if size_bytes as f64 >= GIB {
    format!("{:.1} GiB", size_bytes as f64 / GIB)
  } else {
    format!("{:.1} MiB", size_bytes as f64 / MIB)
  }
}

fn seasons_section<'a>(
  state: &'a State,
  show: &'a VideoShowDetail,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let title = section_title(state.palette(), state.t("detail-seasons"));
  if show.seasons.is_empty() {
    return column![
      title,
      status_surface(state.palette(), state.t("detail-no-seasons"))
    ]
    .spacing(TOKENS.spacing.s3)
    .into();
  }
  let loading = matches!(
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .detail
      .data
      .season_episodes,
    LoadState::Loading
  );
  let mut season_buttons = Row::new()
    .spacing(TOKENS.spacing.s2)
    .padding(padding::horizontal(TOKENS.spacing.s5));
  for season in &show.seasons {
    season_buttons = season_buttons.push(season_button(state, season, loading));
  }
  let selector = scrollable(season_buttons)
    .id(iced::widget::Id::new("detail-seasons"))
    .direction(iced::widget::scrollable::Direction::Horizontal(
      iced::widget::scrollable::Scrollbar::new(),
    ))
    .height(48)
    .style(jellypilot_ui::theme::scrollable);
  let episodes = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .data
    .season_episodes
  {
    LoadState::Idle => status_surface(state.palette(), state.t("detail-choose-season")),
    LoadState::Loading => episode_skeletons(skeleton_phase, reduced_motion),
    LoadState::Failed(error) => retryable_surface(
      state.palette(),
      state.kernel.locale,
      error,
      Message::Detail(DetailMessage::RetrySeason),
    ),
    LoadState::Ready(page) if page.episodes.is_empty() => {
      status_surface(state.palette(), state.t("detail-no-episodes"))
    }
    LoadState::Ready(page) => episode_list(state, &page.episodes, skeleton_phase, reduced_motion),
  };
  column![title, selector, episodes]
    .spacing(TOKENS.spacing.s3)
    .into()
}

fn season_button<'a>(
  state: &State,
  season: &'a VideoSeason,
  loading: bool,
) -> Element<'a, Message> {
  let active = state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .data
    .selected_season_id
    .as_deref()
    == Some(season.id.as_str());
  let variant = if active {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  control_button(None, Some(season_label(season).to_owned()), variant)
    .padding([6, 12])
    .on_press_maybe(
      (!loading).then_some(Message::Detail(DetailMessage::SeasonSelected(
        season.id.clone(),
      ))),
    )
    .into()
}

fn neighbor_section(
  state: &State,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'_, Message> {
  let title = section_title(state.palette(), state.t("detail-season-neighbors"));
  let body = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .data
    .season_neighbors
  {
    LoadState::Idle => return space::vertical().height(0).into(),
    LoadState::Loading => episode_skeletons(skeleton_phase, reduced_motion),
    LoadState::Failed(error) => retryable_surface(
      state.palette(),
      state.kernel.locale,
      error,
      Message::Detail(DetailMessage::RetryNeighbors),
    ),
    LoadState::Ready(items) if items.is_empty() => {
      status_surface(state.palette(), state.t("detail-no-neighbors"))
    }
    LoadState::Ready(items) => episode_list(state, items, skeleton_phase, reduced_motion),
  };
  column![title, body].spacing(TOKENS.spacing.s3).into()
}

fn similar_section(
  state: &State,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'_, Message> {
  let items = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .data
    .similar_items
  {
    LoadState::Loading => {
      return column![
        section_title(state.palette(), state.t("detail-similar")),
        similar_skeletons(skeleton_phase, reduced_motion),
      ]
      .spacing(TOKENS.spacing.s3)
      .into();
    }
    LoadState::Ready(items) if !items.is_empty() => items,
    LoadState::Idle | LoadState::Ready(_) | LoadState::Failed(_) => {
      return space::vertical().height(0).into();
    }
  };
  let mut cards = Row::new().spacing(TOKENS.spacing.s3);
  for item in items {
    cards = cards.push(similar_card(state, item, skeleton_phase, reduced_motion));
  }
  let cards = scrollable(cards)
    .id(iced::widget::Id::new("detail-similar"))
    .direction(Direction::Horizontal(Scrollbar::new()))
    .height(SIMILAR_SCROLL_HEIGHT)
    .style(jellypilot_ui::theme::scrollable);
  column![
    section_title(state.palette(), state.t("detail-similar")),
    cards
  ]
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn similar_card<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let key = detail_similar_key(&item.id);
  let poster = artwork(
    state,
    card_image_spec(key, item),
    &item.name,
    (
      Length::Fixed(SIMILAR_CARD_WIDTH),
      Length::Fixed(SIMILAR_CARD_HEIGHT),
    ),
    ArtworkKind::Similar,
    skeleton_phase,
    reduced_motion,
  );
  let copy = column![
    ellipsis_text(&item.name)
      .size(14)
      .color(state.palette().text.heading),
    text(
      item
        .production_year
        .map_or_else(String::new, |year| year.to_string())
    )
    .size(12)
    .color(state.palette().text.metadata),
  ]
  .spacing(TOKENS.spacing.s1)
  .padding(iced::Padding {
    top: TOKENS.spacing.s2,
    right: 0.0,
    bottom: 0.0,
    left: 0.0,
  })
  .width(Fill);
  poster_card(poster, copy)
    .width(SIMILAR_CARD_WIDTH)
    .on_press(Message::OpenDetail(item.clone()))
    .into()
}

fn similar_skeletons<'a>(phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  let mut cards = Row::new().spacing(TOKENS.spacing.s3);
  for _ in 0..4 {
    cards = cards.push(
      column![
        skeleton_block_with_radius(
          SIMILAR_CARD_WIDTH,
          SIMILAR_CARD_HEIGHT,
          full_radius(TOKENS.radii.xl),
          phase,
          reduced_motion,
        ),
        skeleton_block(SIMILAR_CARD_WIDTH - 20.0, 18.0, phase, reduced_motion),
        skeleton_block(54.0, 14.0, phase, reduced_motion),
      ]
      .spacing(TOKENS.spacing.s1),
    );
  }
  scrollable(cards)
    .direction(Direction::Horizontal(Scrollbar::new()))
    .height(SIMILAR_SCROLL_HEIGHT)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn section_title(palette: &'static ThemePalette, label: String) -> Element<'static, Message> {
  container(
    text(label)
      .font(BODY_FONT)
      .size(26)
      .color(palette.text.heading),
  )
  .padding(padding::horizontal(TOKENS.spacing.s5))
  .into()
}

fn next_up_section<'a>(
  state: &'a State,
  episode: &'a VideoLibraryItem,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  column![
    section_title(state.palette(), state.t("detail-next-up")),
    episode_card(state, episode, true, skeleton_phase, reduced_motion),
  ]
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn episode_list<'a>(
  state: &'a State,
  episodes: &'a [VideoLibraryItem],
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let mut cards = Column::new().spacing(TOKENS.spacing.s3).width(Fill);
  for episode in episodes {
    cards = cards.push(episode_card(
      state,
      episode,
      false,
      skeleton_phase,
      reduced_motion,
    ));
  }
  cards.into()
}

fn episode_card<'a>(
  state: &'a State,
  episode: &'a VideoLibraryItem,
  next_up: bool,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  overview_layout(
    episode.overview.as_deref(),
    TOKENS.spacing.s3 * 2.0 + EPISODE_ART_WIDTH + EPISODE_ACTION_WIDTH + TOKENS.spacing.s4 * 2.0,
    EPISODE_OVERVIEW_TEXT_SIZE,
    move |_, measured_height| {
      episode_card_content(
        state,
        episode,
        next_up,
        measured_height,
        skeleton_phase,
        reduced_motion,
      )
    },
  )
}

fn episode_card_content<'a>(
  state: &'a State,
  episode: &'a VideoLibraryItem,
  next_up: bool,
  measured_height: f32,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  let key = if next_up {
    detail_next_up_key(&episode.id)
  } else {
    detail_episode_key(&episode.id)
  };
  let art = artwork(
    state,
    episode_image_spec(
      &state.full.as_ref().expect("FullUi required").detail.data,
      key,
      episode,
    ),
    &episode.name,
    (
      Length::Fixed(EPISODE_ART_WIDTH),
      Length::Fixed(EPISODE_ART_HEIGHT),
    ),
    ArtworkKind::Episode,
    skeleton_phase,
    reduced_motion,
  );
  let mut copy = Column::new().spacing(TOKENS.spacing.s2).width(Fill).push(
    text(format!(
      "{}  {}",
      episode_label(state.kernel.locale, episode),
      episode.name
    ))
    .font(HEADING_FONT)
    .size(18)
    .color(palette.text.heading),
  );
  if let Some(overview) = episode
    .overview
    .as_deref()
    .filter(|overview| !overview.trim().is_empty())
  {
    let collapsed_height =
      overview_collapsed_height(EPISODE_OVERVIEW_TEXT_SIZE, EPISODE_OVERVIEW_COLLAPSED_LINES);
    let expandable = overview_is_expandable(
      measured_height,
      EPISODE_OVERVIEW_TEXT_SIZE,
      EPISODE_OVERVIEW_COLLAPSED_LINES,
    );
    let expanded = expandable
      && state
        .full
        .as_ref()
        .expect("FullUi required")
        .detail
        .data
        .expanded_episode_ids
        .contains(&episode.id);
    if expandable && !expanded {
      copy = copy.push(
        container(
          text(overview)
            .size(EPISODE_OVERVIEW_TEXT_SIZE)
            .color(palette.text.body),
        )
        .width(Fill)
        .height(collapsed_height)
        .clip(true),
      );
    } else {
      copy = copy.push(
        text(overview)
          .size(EPISODE_OVERVIEW_TEXT_SIZE)
          .color(palette.text.body),
      );
    }
    if expandable {
      let (label, icon) = if expanded {
        (state.t("detail-less"), Icon::ChevronUp)
      } else {
        (state.t("detail-more"), Icon::ChevronDown)
      };
      copy = copy.push(
        control_button(Some(icon), Some(label), ButtonVariant::Text)
          .icon_size(IconSize::Xs)
          .trailing_icon(true)
          .spacing(TOKENS.spacing.s1)
          .padding([5, 8])
          .on_press(Message::Detail(DetailMessage::EpisodeOverviewToggled(
            episode.id.clone(),
          ))),
      );
    }
  }
  if let Some(progress) = playback_progress(episode) {
    copy = copy.push(progress_bar(palette, progress));
  }
  let play_label = if has_resume(episode) {
    state.t("detail-resume")
  } else {
    state.t("detail-play")
  };
  let play_enabled = state.playback.view.engine_available;
  let play = control_button(Some(Icon::Play), Some(play_label), ButtonVariant::Primary)
    .icon_size(IconSize::Sm)
    .spacing(TOKENS.spacing.s1_5)
    .padding([6, 12])
    .on_press_maybe(play_enabled.then(|| {
      playback_message(
        state,
        Playable::Library(episode.clone()),
        if has_resume(episode) {
          PlaybackStartPosition::Resume
        } else {
          PlaybackStartPosition::Beginning
        },
      )
    }));
  container(
    row![art, copy, play]
      .spacing(TOKENS.spacing.s4)
      .align_y(Alignment::Center),
  )
  .padding(TOKENS.spacing.s3)
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
  .into()
}

fn playback_message(state: &State, item: Playable, position: PlaybackStartPosition) -> Message {
  Message::Playback(PlaybackMessage::Intent(Box::new(PlaybackIntent::Start {
    item,
    position,
    intro: state.kernel.intro_availability(),
    selection: Box::default(),
  })))
}

#[derive(Clone, Copy)]
enum ArtworkKind {
  Hero,
  Similar,
  Episode,
}

impl ArtworkKind {
  fn initial_size(self) -> u32 {
    match self {
      Self::Hero => 64,
      Self::Similar | Self::Episode => 34,
    }
  }

  fn radius(self) -> iced::border::Radius {
    let radius = match self {
      Self::Similar => TOKENS.radii.xl,
      Self::Hero | Self::Episode => TOKENS.radii.lg,
    };
    full_radius(radius)
  }
}

fn hero_spec(state: &State, key: &str) -> Option<ImageSpec> {
  let LoadState::Ready(content) = &state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .data
    .content
  else {
    return None;
  };
  hero_image_spec(content, key)
}

fn observe_detail<'a>(
  content: Element<'a, Message>,
  state: &State,
  spec: Option<ImageSpec>,
  axis: ImageAxis,
) -> Element<'a, Message> {
  match spec {
    Some(spec) => observe_image(
      content,
      ArtworkSurface::Detail,
      state
        .full
        .as_ref()
        .expect("FullUi required")
        .detail
        .artwork
        .epoch(),
      spec,
      axis,
    ),
    None => content,
  }
}

fn artwork<'a>(
  state: &'a State,
  spec: Option<ImageSpec>,
  name: &'a str,
  size: (Length, Length),
  kind: ArtworkKind,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let content = render_artwork(
    state,
    spec.as_ref().map(|spec| spec.key.as_str()),
    name,
    size,
    kind,
    phase,
    reduced_motion,
  );
  let axis = if matches!(kind, ArtworkKind::Similar) {
    ImageAxis::Horizontal
  } else {
    ImageAxis::Vertical
  };
  observe_detail(content, state, spec, axis)
}

fn render_artwork<'a>(
  state: &'a State,
  key: Option<&str>,
  name: &'a str,
  (width, height): (Length, Length),
  kind: ArtworkKind,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  let initial_size = kind.initial_size();
  let radius = kind.radius();
  let cell = key.and_then(|key| {
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .detail
      .artwork
      .get(key)
  });
  if let Some(cell) = cell {
    if let Some(handle) = cell.handle() {
      return rounded_image(handle.clone(), radius)
        .content_fit(ContentFit::Cover)
        .width(width)
        .height(height)
        .into();
    }
  }
  match cell.map(|cell| cell.state) {
    // The server carries no image for this slot, so no load was ever planned:
    // settle on a neutral placeholder instead of shimmering forever.
    None if key.is_none() => artwork_placeholder(
      palette,
      name,
      initial_size,
      width,
      height,
      radius,
      palette.text.metadata,
    ),
    Some(ImageStatus::Failed) => artwork_placeholder(
      palette,
      name,
      initial_size,
      width,
      height,
      radius,
      palette.colors.warning,
    ),
    _ => skeleton_panel(
      width,
      height,
      palette.colors.surfaceContainerLowest,
      radius,
      phase,
      reduced_motion,
    )
    .into(),
  }
}

fn artwork_placeholder<'a>(
  palette: &'static ThemePalette,
  name: &str,
  initial_size: u32,
  width: Length,
  height: Length,
  radius: iced::border::Radius,
  color: iced::Color,
) -> Element<'a, Message> {
  let initial = name
    .trim()
    .chars()
    .next()
    .map(|character| character.to_uppercase().collect::<String>())
    .unwrap_or_else(|| "•".to_owned());
  let icon_dim = (initial_size as f32).max(28.0);
  container(
    column![
      icon_with_color(Icon::Movie, icon_dim, color),
      text(initial)
        .font(HEADING_FONT)
        .size(initial_size.min(28))
        .color(color),
    ]
    .spacing(TOKENS.spacing.s1)
    .align_x(Alignment::Center),
  )
  .width(width)
  .height(height)
  .center_x(Fill)
  .center_y(Fill)
  .style(move |_theme| container::Style {
    background: Some(iced::Background::Color(
      palette.colors.surfaceContainerLowest,
    )),
    border: iced::Border {
      smoothing: jellypilot_ui::widgets::container::SURFACE_SMOOTHING,
      radius,
      width: 0.0,
      color: iced::Color::TRANSPARENT,
    },
    ..container::Style::default()
  })
  .into()
}

fn detail_skeleton(
  state: &State,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'_, Message> {
  let back_enabled = !state.shell.navigation_stack.is_empty();
  let back = control_button(
    Some(Icon::ChevronLeft),
    Some(state.t("detail-back")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(back_enabled.then_some(Message::Detail(DetailMessage::Back)));
  let body = column![
    back,
    column![
      skeleton_block(360.0, HERO_LOGO_HEIGHT, skeleton_phase, reduced_motion),
      skeleton_block(240.0, 18.0, skeleton_phase, reduced_motion),
      skeleton_block(680.0, 96.0, skeleton_phase, reduced_motion),
      skeleton_block(430.0, 42.0, skeleton_phase, reduced_motion),
    ]
    .spacing(TOKENS.spacing.s4),
  ]
  .spacing(TOKENS.spacing.s5)
  .padding(TOKENS.spacing.s6);
  container(body).width(Fill).height(Fill).into()
}

fn detail_failure<'a>(state: &State, error: &'a UiText) -> Element<'a, Message> {
  let back_enabled = !state.shell.navigation_stack.is_empty();
  let back = control_button(
    Some(Icon::ChevronLeft),
    Some(state.t("detail-back")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(back_enabled.then_some(Message::Detail(DetailMessage::Back)));
  let retry = control_button(
    Some(Icon::Refresh),
    Some(state.t("detail-retry")),
    ButtonVariant::Primary,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 12])
  .on_press(Message::Detail(DetailMessage::Retry));
  container(
    column![
      back,
      text(state.t("detail-load-heading"))
        .font(DISPLAY_FONT)
        .size(28)
        .color(state.palette().text.heading),
      text(state.kernel.locale.message(error))
        .size(14)
        .color(state.palette().colors.error),
      retry,
    ]
    .spacing(TOKENS.spacing.s3),
  )
  .padding(TOKENS.spacing.s6)
  .width(Fill)
  .height(Fill)
  .into()
}

fn retryable_surface<'a>(
  palette: &ThemePalette,
  locale: Localizer,
  error: &'a UiText,
  retry: Message,
) -> Element<'a, Message> {
  container(
    row![
      text(locale.message(error))
        .size(13)
        .color(palette.colors.error),
      control_button(
        Some(Icon::Refresh),
        Some(locale.text("detail-retry")),
        ButtonVariant::Tonal,
      )
      .icon_size(IconSize::Xs)
      .spacing(TOKENS.spacing.s1)
      .padding([6, 10])
      .on_press(retry),
    ]
    .spacing(TOKENS.spacing.s3)
    .align_y(Alignment::Center),
  )
  .padding(TOKENS.spacing.s3)
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
  .into()
}

fn status_surface<'a>(palette: &'static ThemePalette, message: String) -> Element<'a, Message> {
  container(text(message).size(14).color(palette.text.metadata))
    .padding(TOKENS.spacing.s4)
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
    .into()
}

fn episode_skeletons<'a>(skeleton_phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  let mut rows = Column::new().spacing(TOKENS.spacing.s3).width(Fill);
  for _ in 0..3 {
    rows = rows.push(
      row![
        skeleton_block(
          EPISODE_ART_WIDTH,
          EPISODE_ART_HEIGHT,
          skeleton_phase,
          reduced_motion,
        ),
        column![
          skeleton_block(320.0, 20.0, skeleton_phase, reduced_motion),
          skeleton_block(620.0, 54.0, skeleton_phase, reduced_motion),
        ]
        .spacing(TOKENS.spacing.s3),
      ]
      .spacing(TOKENS.spacing.s4),
    );
  }
  rows.into()
}

fn progress_bar<'a>(palette: &'static ThemePalette, progress: f64) -> Element<'a, Message> {
  let filled = (progress.round() as u16).min(100);
  let remaining = 100_u16.saturating_sub(filled);
  let mut bar = Row::new().width(Fill).height(4);
  if filled > 0 {
    bar = bar.push(
      container(space::horizontal())
        .width(Length::FillPortion(filled))
        .height(4)
        .style(|_| iced::widget::container::Style::default().background(palette.colors.primary)),
    );
  }
  if remaining > 0 {
    bar = bar.push(
      container(space::horizontal())
        .width(Length::FillPortion(remaining))
        .height(4)
        .style(|_| {
          iced::widget::container::Style::default().background(palette.colors.surfaceContainerLow)
        }),
    );
  }
  bar.into()
}

fn playback_progress(item: &VideoLibraryItem) -> Option<f64> {
  if item.played {
    return None;
  }
  if let Some(percentage) = item.played_percentage.filter(|value| value.is_finite()) {
    let percentage = percentage.clamp(0.0, 100.0);
    if percentage > 0.0 && percentage < 100.0 {
      return Some(percentage);
    }
  }
  match (item.resume_position_seconds, item.runtime_seconds) {
    (Some(position), Some(runtime))
      if position.is_finite() && position > 0.0 && runtime.is_finite() && runtime > position =>
    {
      Some((position / runtime * 100.0).clamp(0.0, 100.0))
    }
    _ => None,
  }
}

fn item_metadata(locale: Localizer, item: &jellypilot_media_server::VideoItemDetail) -> String {
  let mut values = vec![detail_metadata(locale, item)];
  if let (Some(series), Some(season), Some(episode)) = (
    item.series_name.as_deref(),
    item.season_number,
    item.episode_number,
  ) {
    values.push(format!("{series} · S{season:02}E{episode:02}"));
  } else if let Some(series) = item.series_name.as_deref() {
    values.push(series.to_owned());
  }
  if let Some(runtime) = item
    .runtime_seconds
    .and_then(|seconds| runtime_label(locale, seconds))
  {
    values.push(runtime);
  }
  if let Some(rating) = item
    .metadata
    .community_rating
    .filter(|rating| rating.is_finite() && (0.0..=10.0).contains(rating))
  {
    values.push(format!("{rating:.1}/10"));
  }
  if let Some(rating) = item.metadata.official_rating.as_deref() {
    values.push(rating.to_owned());
  }
  if item.can_resume {
    if let Some(progress) = item
      .played_percentage
      .filter(|progress| progress.is_finite() && *progress > 0.0 && *progress < 100.0)
    {
      values.push(locale.format(
        "detail-watched-percent",
        &[("percent", format!("{progress:.0}").into())],
      ));
    }
  }
  values.join(" · ")
}

fn show_metadata(locale: Localizer, show: &VideoShowDetail) -> String {
  let mut values = vec![show_detail_metadata(locale, show)];
  if let Some(rating) = show
    .metadata
    .community_rating
    .filter(|rating| rating.is_finite() && (0.0..=10.0).contains(rating))
  {
    values.push(format!("{rating:.1}/10"));
  }
  if let Some(rating) = show.metadata.official_rating.as_deref() {
    values.push(rating.to_owned());
  }
  values.join(" · ")
}

fn runtime_label(locale: Localizer, seconds: f64) -> Option<String> {
  (seconds.is_finite() && seconds > 0.0).then(|| locale.duration(seconds))
}

fn has_resume(item: &VideoLibraryItem) -> bool {
  !item.played
    && item
      .resume_position_seconds
      .is_some_and(|position| position.is_finite() && position > 0.0)
}

fn episode_label(locale: Localizer, episode: &VideoLibraryItem) -> String {
  match (episode.season_number, episode.episode_number) {
    (Some(season), Some(number)) => format!("S{season:02}E{number:02}"),
    _ => locale.text("detail-episode"),
  }
}

fn season_label(season: &VideoSeason) -> &str {
  &season.name
}

fn overview_collapsed_height(text_size: f32, line_count: f32) -> f32 {
  f32::from(jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT.to_absolute(Pixels(text_size))) * line_count
}

fn overview_is_expandable(measured_height: f32, text_size: f32, line_count: f32) -> bool {
  measured_height > overview_collapsed_height(text_size, line_count)
}

fn limited_people(locale: Localizer, people: &[String], limit: usize) -> String {
  let visible = people
    .iter()
    .take(limit)
    .map(String::as_str)
    .collect::<Vec<_>>()
    .join(" • ");
  let extra = people.len().saturating_sub(limit);
  if extra == 0 {
    visible
  } else {
    locale.format(
      "detail-more-people",
      &[("people", visible.into()), ("count", extra.into())],
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn episode_with_progress(
    resume_position_seconds: Option<f64>,
    played_percentage: Option<f64>,
    played: bool,
  ) -> VideoLibraryItem {
    VideoLibraryItem {
      id: "episode-1".to_owned(),
      name: "Pilot".to_owned(),
      item_type: "Episode".to_owned(),
      production_year: None,
      runtime_seconds: Some(1_800.0),
      played,
      favorite: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      logo_image_id: None,
      series_poster_image_id: None,
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_number: Some(1),
      episode_number: Some(1),
      series_id: Some("show-1".to_owned()),
      series_name: Some("Show".to_owned()),
      resume_position_seconds,
      played_percentage,
      overview: None,
      index_number_end: None,
      season_poster_image_id: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
    }
  }

  #[test]
  fn watched_percentage_without_a_positive_offset_does_not_offer_resume() {
    let episode = episode_with_progress(None, Some(50.0), false);

    assert!(!has_resume(&episode));
  }

  #[test]
  fn finite_positive_offset_on_an_unplayed_episode_offers_resume() {
    let episode = episode_with_progress(Some(120.0), None, false);

    assert!(has_resume(&episode));
  }

  #[test]
  fn overview_at_the_collapsed_height_is_not_expandable() {
    let collapsed_height = overview_collapsed_height(OVERVIEW_TEXT_SIZE, OVERVIEW_COLLAPSED_LINES);

    assert!(!overview_is_expandable(
      collapsed_height,
      OVERVIEW_TEXT_SIZE,
      OVERVIEW_COLLAPSED_LINES
    ));
  }

  #[test]
  fn overview_taller_than_the_collapsed_height_is_expandable() {
    let collapsed_height = overview_collapsed_height(OVERVIEW_TEXT_SIZE, OVERVIEW_COLLAPSED_LINES);

    assert!(overview_is_expandable(
      collapsed_height + 1.0,
      OVERVIEW_TEXT_SIZE,
      OVERVIEW_COLLAPSED_LINES
    ));
  }

  #[test]
  fn episode_overview_uses_the_three_line_thirteen_pixel_boundary() {
    let collapsed_height =
      overview_collapsed_height(EPISODE_OVERVIEW_TEXT_SIZE, EPISODE_OVERVIEW_COLLAPSED_LINES);

    assert!(!overview_is_expandable(
      collapsed_height,
      EPISODE_OVERVIEW_TEXT_SIZE,
      EPISODE_OVERVIEW_COLLAPSED_LINES,
    ));
    assert!(overview_is_expandable(
      collapsed_height + 1.0,
      EPISODE_OVERVIEW_TEXT_SIZE,
      EPISODE_OVERVIEW_COLLAPSED_LINES,
    ));
  }
}
