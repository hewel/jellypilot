mod chrome;
mod overview;

use overview::overview_layout;

use super::image_observer::{observe_image, ImageAxis};
use crate::app::artwork::{ArtworkSurface, ImageSpec, ImageStatus};
use crate::app::detail::{
  card_image_spec, cast_image_spec, detail_next_up_key, episode_image_spec, hero_image_spec,
  DETAIL_BACKDROP_KEY, DETAIL_LOGO_KEY,
};
use crate::app::message::{DetailMessage, Message, PlaybackMessage};
use crate::app::state::{State, TrackMenu, UserDataActionKind};
use crate::i18n::media::{
  detail_metadata, episode_premiere_date, item_caption, show_detail_metadata,
};
use crate::i18n::{Localizer, UiText};
use iced::widget::image::Image;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
  column, container, responsive, row, scrollable, space, stack, text, Column, Row,
};
use iced::{gradient, padding};
use iced::{Alignment, Background, ContentFit, Degrees, Element, Fill, Length, Pixels};
use jellypilot_core::detail::{detail_episode_key, detail_similar_key, DetailContent};
use jellypilot_core::LoadState;
use jellypilot_media_server::{
  VideoCastMember, VideoItemDetail, VideoLibraryItem, VideoMediaInfo, VideoSeason, VideoShowDetail,
  VideoStreamInfo,
};
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconControlState, IconSize};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::skeleton::{
  skeleton_block, skeleton_block_with_radius, skeleton_panel,
};
use jellypilot_ui::{full_radius, poster_card, rounded_image};

const HERO_HEIGHT: f32 = 520.0;
const HERO_LOGO_HEIGHT: f32 = 96.0;
const EPISODE_ART_WIDTH: f32 = 300.0;
const EPISODE_ART_HEIGHT: f32 = 169.0;
const EPISODE_ACTION_WIDTH: f32 = 100.0;
const OVERVIEW_TEXT_SIZE: f32 = 16.0;
const OVERVIEW_LINE_HEIGHT: f32 = 24.0;
const EPISODE_OVERVIEW_TEXT_SIZE: f32 = 12.0;
const EPISODE_OVERVIEW_LINE_HEIGHT: f32 = 20.0;
const SIMILAR_CARD_WIDTH: f32 = 150.0;
const SIMILAR_CARD_HEIGHT: f32 = 225.0;
const SIMILAR_SCROLL_HEIGHT: f32 = 283.0;

fn section_inset(width: f32) -> f32 {
  if width < 600.0 {
    18.0
  } else {
    36.0
  }
}

fn detail_section<'a>(content: Element<'a, Message>, inset: f32) -> Element<'a, Message> {
  container(content)
    .padding(iced::Padding {
      top: 28.0,
      right: inset,
      bottom: 0.0,
      left: inset,
    })
    .width(Fill)
    .into()
}

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
  responsive(move |bounds| -> Element<'_, Message> {
    let inset = section_inset(bounds.width);
    let mut page = Column::new().width(Fill);
    page = page.push(match content {
      DetailContent::Item(item) => item_hero(state, item, skeleton_phase, reduced_motion),
      DetailContent::Show(show) => show_hero(state, show, skeleton_phase, reduced_motion),
    });
    let (genres, metadata) = match content {
      DetailContent::Item(item) => (&item.genres, &item.metadata),
      DetailContent::Show(show) => (&show.genres, &show.metadata),
    };
    if !genres.is_empty() || !metadata.creators.is_empty() || !metadata.cast.is_empty() {
      page = page.push(detail_section(
        summary(
          state.palette(),
          state.kernel.locale,
          genres,
          &metadata.creators,
          &metadata.cast,
        ),
        inset,
      ));
    }
    match content {
      DetailContent::Item(item) if item.item_type.eq_ignore_ascii_case("episode") => {
        page = page.push(detail_section(
          neighbor_section(state, skeleton_phase, reduced_motion),
          inset,
        ));
      }
      DetailContent::Show(show) => {
        if let Some(next) = &show.next_episode {
          page = page.push(detail_section(
            next_up_section(state, next, skeleton_phase, reduced_motion),
            inset,
          ));
        }
        page = page.push(detail_section(
          seasons_section(state, show, skeleton_phase, reduced_motion),
          inset,
        ));
      }
      DetailContent::Item(_) => {}
    }
    page = page
      .push(detail_section(
        cast_section(state, &metadata.cast, skeleton_phase, reduced_motion),
        inset,
      ))
      .push(detail_section(
        similar_section(state, skeleton_phase, reduced_motion),
        inset,
      ));
    if let DetailContent::Item(item) = content {
      if item.media_info.is_some() {
        page = page.push(detail_section(
          media_info_section(state.palette(), state.kernel.locale, item),
          inset,
        ));
      }
    }
    scrollable(page.padding(padding::bottom(40)))
      .id(iced::widget::Id::new("detail-page"))
      .width(Fill)
      .height(Fill)
      .style(jellypilot_ui::theme::scrollable)
      .into()
  })
  .height(Length::Fit)
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
      media_info: item.media_info.as_ref(),
      overview: item.overview.as_deref(),
      playback_label,
      playback: item
        .can_play
        .then(|| (Playable::Detail(item.clone()), position)),
      played: item.played,
      favorite: item.favorite,
      is_episode: item.item_type.eq_ignore_ascii_case("episode"),
      is_movie: item.item_type.eq_ignore_ascii_case("movie"),
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
      media_info: None,
      overview: show.overview.as_deref(),
      playback_label,
      playback,
      played: show.played,
      favorite: show.favorite,
      is_episode: false,
      is_movie: false,
    },
    skeleton_phase,
    reduced_motion,
  )
}

/// Metadata line segments: the leading facts, the episode's parent series
/// (a link only with a real server series id), and the trailing facts.
struct DetailMetadata {
  head: String,
  series: Option<MetadataSeries>,
  tail: Vec<String>,
}

struct MetadataSeries {
  name: String,
  link: bool,
}

struct HeroContent<'a> {
  id: &'a str,
  name: &'a str,
  metadata: DetailMetadata,
  /// First-source media facts for the specifications row; movies and episodes
  /// only, absent when the server supplied none.
  media_info: Option<&'a VideoMediaInfo>,
  overview: Option<&'a str>,
  playback_label: String,
  playback: Option<(Playable, PlaybackStartPosition)>,
  played: bool,
  favorite: bool,
  is_episode: bool,
  is_movie: bool,
}

fn hero<'a>(
  state: &'a State,
  content: HeroContent<'a>,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  responsive(move |bounds| -> Element<'_, Message> {
    let inset = section_inset(bounds.width);
    let banner = hero_banner(
      state,
      &content,
      bounds.width,
      skeleton_phase,
      reduced_motion,
    );
    let mut copy = column![metadata_row(state, &content.metadata)]
      .spacing(14)
      .width(Fill);
    if let Some(info) = content.media_info {
      if let Some(specs) = media_specs_row(state, info) {
        copy = copy.push(specs);
      }
    }
    if let Some(overview) = nonempty(content.overview) {
      copy = copy.push(overview_copy(
        state,
        overview,
        OVERVIEW_TEXT_SIZE,
        OVERVIEW_LINE_HEIGHT,
        state
          .full
          .as_ref()
          .expect("FullUi required")
          .detail
          .data
          .overview_expanded,
        Message::Detail(DetailMessage::OverviewToggled),
      ));
    }
    copy = copy.push(detail_actions(
      state,
      content.playback_label.clone(),
      content.playback.clone(),
      content.id,
      content.played,
      content.favorite,
    ));
    column![banner, detail_section(copy.into(), inset)]
      .width(Fill)
      .into()
  })
  .height(Length::Fit)
  .into()
}

fn hero_banner<'a>(
  state: &'a State,
  content: &HeroContent<'a>,
  width: f32,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let backdrop = artwork(
    state,
    hero_spec(state, DETAIL_BACKDROP_KEY),
    content.name,
    (Fill, Length::Fixed(HERO_HEIGHT)),
    ArtworkKind::Hero,
    skeleton_phase,
    reduced_motion,
  );
  let canvas = state.palette().colors.background;
  let bottom = gradient::Linear::new(Degrees(180.0))
    .add_stop(0.0, canvas.scale_alpha(0.30))
    .add_stop(0.45, canvas.scale_alpha(0.28))
    .add_stop(0.78, canvas.scale_alpha(0.72))
    .add_stop(1.0, canvas);
  let left = gradient::Linear::new(Degrees(90.0))
    .add_stop(0.0, canvas.scale_alpha(0.55))
    .add_stop(0.55, canvas.scale_alpha(0.0));
  let scrim = |gradient: gradient::Linear| {
    container(space::vertical())
      .width(Fill)
      .height(Fill)
      .style(move |_| container::Style::default().background(Background::Gradient(gradient.into())))
  };
  let identity = container(hero_title(
    state,
    content.name,
    content.is_episode,
    content.is_movie,
    width,
  ))
  .padding(iced::Padding {
    top: 0.0,
    right: 32.0,
    bottom: if content.is_movie { 21.0 } else { 24.0 },
    left: 32.0,
  })
  .width(Fill)
  .height(Fill)
  .align_y(Alignment::End);
  stack![
    backdrop,
    scrim(left),
    scrim(bottom),
    identity,
    chrome::detail_back(state, width, HERO_HEIGHT),
  ]
  .width(Fill)
  .height(HERO_HEIGHT)
  .into()
}

fn overview_copy<'a>(
  state: &'a State,
  overview: &'a str,
  text_size: f32,
  line_height: f32,
  expanded: bool,
  toggle: Message,
) -> Element<'a, Message> {
  overview_layout(
    Some(overview),
    0.0,
    text_size,
    line_height,
    move |_, measured_height| {
      let expandable = measured_height > line_height * 2.0;
      let clipped = expandable && !expanded;
      let copy = text(overview)
        .size(text_size)
        .line_height(Pixels(line_height))
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
        .color(state.palette().text.secondary)
        .width(Fill);
      // The fade overlay lives outside the animated region so it stays
      // anchored to the visible bottom edge while the measured height moves.
      let copy_block: Element<'_, Message> = if clipped {
        container(copy)
          .height(line_height * 2.0)
          .width(Fill)
          .clip(true)
          .into()
      } else {
        copy.into()
      };
      let fade_overlay: Element<'_, Message> = if clipped {
        let canvas = state.palette().colors.background;
        let fade = gradient::Linear::new(Degrees(90.0))
          .add_stop(0.0, canvas.scale_alpha(0.0))
          .add_stop(1.0, canvas);
        container(
          container(space::horizontal())
            .width(100)
            .height(line_height)
            .style(move |_: &iced::Theme| {
              container::Style::default().background(Background::Gradient(fade.into()))
            }),
        )
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::End)
        .into()
      } else {
        space().into()
      };
      let visible: Element<'_, Message> = stack![
        super::motion::resize(copy_block, expanded as u64, super::motion::Axis::Vertical),
        fade_overlay,
      ]
      .into();
      let mut body = column![visible].spacing(4).width(Fill);
      if expandable {
        body = body.push(
          control_button(
            None,
            Some(state.t(if expanded {
              "detail-less"
            } else {
              "detail-more"
            })),
            ButtonVariant::Text,
          )
          .label_size(12.0)
          .padding([2, 0])
          .min_height(20.0)
          .on_press(toggle.clone()),
        );
      }
      body.into()
    },
  )
}

fn hero_title<'a>(
  state: &'a State,
  name: &'a str,
  is_episode: bool,
  is_movie: bool,
  width: f32,
) -> Element<'a, Message> {
  let title = || {
    text(name)
      .font(DISPLAY_FONT)
      .size(if width < 600.0 { 30 } else { 40 })
      .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
      .color(state.palette().text.heading)
      .width(Fill)
  };
  let max_width = (width - 64.0)
    .max(1.0)
    .min(if is_movie { 320.0 } else { 300.0 });
  let max_height = if is_movie { 99.0 } else { HERO_LOGO_HEIGHT };
  let logo = state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .artwork
    .get(DETAIL_LOGO_KEY)
    .and_then(|cell| {
      let handle = cell.handle()?;
      let (logo_width, logo_height) = cell
        .dims()
        .filter(|&(w, h)| w > 0 && h > 0)
        .map(|(w, h)| {
          let scale = (max_width / w as f32).min(max_height / h as f32);
          (w as f32 * scale, h as f32 * scale)
        })
        .unwrap_or((max_width, max_height));
      Some(
        container(
          Image::new(handle.clone())
            .content_fit(ContentFit::Contain)
            .width(logo_width)
            .height(logo_height),
        )
        .width(Fill)
        .align_x(Alignment::Start)
        .into(),
      )
    });
  let identity: Element<'a, Message> = match logo {
    Some(logo) if is_episode => column![logo, title()].spacing(8).into(),
    Some(logo) => logo,
    None => title().into(),
  };
  observe_detail(
    identity,
    state,
    hero_spec(state, DETAIL_LOGO_KEY),
    ImageAxis::Vertical,
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
  .icon_size(IconSize::Custom(15.0))
  .spacing(8.0)
  .padding([8, 16])
  .min_height(36.0)
  .label_size(14.0)
  .on_press_maybe(
    playback_target
      .filter(|_| playback_enabled)
      .map(|(item, position)| playback_message(state, item, position)),
  );
  let any_busy =
    crate::app::collections::busy(state.full.as_ref().expect("FullUi required"), item_id);
  // The favorited heart stays rose across hover (fixed `favorite` accent);
  // the unfavorited heart is an ordinary Tonal control on `control_button`.
  let favorite_button: Element<'_, Message> = if favorite {
    control_button_content(
      move |status| {
        let palette = state.palette();
        let disabled = matches!(status, IconControlState::Disabled);
        row![
          icon_with_color(
            Icon::HeartFilled,
            IconSize::Custom(15.0),
            if disabled {
              palette.text.muted
            } else {
              palette.colors.favorite
            }
          ),
          text(state.t("detail-favorited"))
            .size(14)
            .line_height(Pixels(18.0))
            .color(if disabled {
              palette.text.muted
            } else {
              palette.colors.onControlHover
            }),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
      },
      ButtonVariant::TonalActive,
    )
    .padding([9, 16])
    .min_height(36.0)
    .on_press_maybe((!any_busy).then_some(Message::Detail(DetailMessage::FavoriteToggled)))
    .into()
  } else {
    control_button(
      Some(Icon::Heart),
      Some(state.t("detail-favorite")),
      ButtonVariant::Tonal,
    )
    .icon_size(IconSize::Custom(15.0))
    .label_size(14.0)
    .min_height(36.0)
    .spacing(8.0)
    .padding([8, 16])
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
  .icon_size(IconSize::Custom(15.0))
  .label_size(14.0)
  .min_height(36.0)
  .spacing(8.0)
  .padding([8, 16])
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
    .icon_size(IconSize::Custom(15.0))
    .label_size(14.0)
    .min_height(36.0)
    .spacing(8.0)
    .padding([8, 16])
    .on_press_maybe((!any_busy).then_some(Message::Detail(DetailMessage::PlayedToggled)));
  let mut actions = Row::new()
    .spacing(10)
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
  let mut content = Column::new()
    .spacing(TOKENS.spacing.s2)
    .push(actions.wrap());
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
  palette: &'static ThemePalette,
  locale: Localizer,
  genres: &'a [String],
  creators: &'a [String],
  cast: &'a [VideoCastMember],
) -> Element<'a, Message> {
  responsive(move |bounds| -> Element<'_, Message> {
    let mut left = Column::new().spacing(14).width(Fill);
    if !genres.is_empty() {
      left = left.push(summary_column(
        palette,
        locale.text("detail-genres"),
        genres.join(" · "),
      ));
    }
    if !creators.is_empty() {
      left = left.push(summary_column(
        palette,
        locale.text("detail-creators"),
        creators.join(" · "),
      ));
    }
    let names = cast
      .iter()
      .take(4)
      .map(|member| member.name.as_str())
      .collect::<Vec<_>>()
      .join(" · ");
    let names = if cast.len() > 4 {
      locale.format(
        "detail-more-people",
        &[("people", names.into()), ("count", (cast.len() - 4).into())],
      )
    } else {
      names
    };
    let right = summary_column(palette, locale.text("detail-cast"), names);
    if cast.is_empty() {
      return left.into();
    }
    if genres.is_empty() && creators.is_empty() {
      return right;
    }
    if bounds.width < 720.0 {
      column![left, right].spacing(20).width(Fill).into()
    } else {
      row![container(left).width(360), right]
        .spacing(40)
        .width(Fill)
        .into()
    }
  })
  .height(Length::Fit)
  .into()
}

fn cast_section<'a>(
  state: &'a State,
  cast: &'a [VideoCastMember],
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  if cast.is_empty() {
    return column![
      section_title(state.palette(), state.t("detail-cast-heading")),
      status_surface(state.palette(), state.t("detail-no-cast")),
    ]
    .spacing(14)
    .into();
  }
  let mut people = Row::new().spacing(24);
  for (index, member) in cast.iter().enumerate() {
    let portrait = artwork(
      state,
      cast_image_spec(index, member),
      &member.name,
      (Length::Fixed(72.0), Length::Fixed(72.0)),
      ArtworkKind::Cast,
      phase,
      reduced_motion,
    );
    let mut person = column![
      portrait,
      text(&member.name)
        .size(12)
        .font(HEADING_FONT)
        .line_height(Pixels(16.0))
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
        .align_x(Alignment::Center)
        .color(state.palette().text.secondary)
        .width(Fill),
    ]
    .width(96)
    .spacing(8)
    .align_x(Alignment::Center);
    if let Some(role) = nonempty(member.role.as_deref()) {
      person = person.push(
        text(role)
          .size(12)
          .line_height(Pixels(15.0))
          .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
          .align_x(Alignment::Center)
          .color(state.palette().text.metadata)
          .width(Fill),
      );
    }
    people = people.push(person);
  }
  column![
    section_title(state.palette(), state.t("detail-cast-heading")),
    scrollable(people.padding(padding::bottom(12)))
      .id(iced::widget::Id::new("detail-cast"))
      .direction(Direction::Horizontal(Scrollbar::new()))
      .width(Fill)
      .height(Length::Fit)
      .style(jellypilot_ui::theme::scrollable),
  ]
  .spacing(14)
  .width(Fill)
  .into()
}

fn summary_column(
  palette: &ThemePalette,
  label: String,
  values: String,
) -> Element<'static, Message> {
  column![
    text(label)
      .size(12)
      .line_height(Pixels(16.0))
      .color(palette.text.metadata),
    text(values)
      .size(14)
      .line_height(Pixels(22.0))
      .color(palette.text.secondary),
  ]
  .spacing(TOKENS.spacing.s2)
  .width(Fill)
  .into()
}

/// The metadata line: leading facts, the episode's parent series as a
/// keyboard-operable link when a real server series id exists, then trailing
/// facts. Segments wrap as units at narrow widths.
fn metadata_row<'a>(state: &'a State, metadata: &DetailMetadata) -> Element<'a, Message> {
  let palette = state.palette();
  let segment = |value: &str| -> Element<'a, Message> {
    text(value.to_owned())
      .size(14)
      .line_height(Pixels(18.0))
      .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
      .color(palette.text.body)
      .into()
  };
  let mut line = Row::new().align_y(Alignment::Center);
  if !metadata.head.is_empty() {
    line = line.push(segment(&metadata.head));
  }
  if let Some(series) = &metadata.series {
    line = line.push(segment(" · "));
    if series.link {
      let name = series.name.clone();
      line = line.push(
        control_button_content(
          move |status| {
            let color = match status {
              IconControlState::Rest => palette.text.body,
              IconControlState::Hovered => palette.text.heading,
              IconControlState::Disabled => palette.text.muted,
            };
            row![
              text(name.clone())
                .size(14)
                .line_height(Pixels(18.0))
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .color(color),
              icon_with_color(Icon::ChevronRight, IconSize::Custom(12.0), color),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
          },
          ButtonVariant::Text,
        )
        .padding([0, 0])
        .on_press(Message::Detail(DetailMessage::OpenSeries)),
      );
    } else {
      line = line.push(segment(&series.name));
    }
  }
  for fact in &metadata.tail {
    line = line.push(segment(" · ")).push(segment(fact));
  }
  line.wrap().vertical_spacing(0.0).into()
}

/// The Media Specifications row between metadata and overview: one chip per
/// known fact of the first media source, wrapping at narrow widths. Unknown
/// fields are omitted rather than invented.
fn media_specs_row<'a>(state: &'a State, info: &'a VideoMediaInfo) -> Option<Element<'a, Message>> {
  let palette = state.palette();
  let locale = state.kernel.locale;
  let mut chips = Row::new()
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center);
  let mut any = false;
  if let Some(height) = info.video_height.filter(|height| *height > 0) {
    chips = chips.push(spec_chip(palette, format!("{height}p")));
    any = true;
  }
  if let Some(range) = nonempty(info.video_range.as_deref()) {
    chips = chips.push(spec_chip(palette, range.to_owned()));
    any = true;
  }
  if let Some(codec) = nonempty(info.video_codec.as_deref()) {
    chips = chips.push(spec_chip(palette, codec.to_owned()));
    any = true;
  }
  if let Some(rate) = info
    .video_frame_rate
    .filter(|rate| rate.is_finite() && *rate > 0.0)
  {
    chips = chips.push(spec_chip(palette, frame_rate_label(rate)));
    any = true;
  }
  // Track facts exist only when the chosen source's stream metadata was
  // supplied; an unknown stream list is never presented as a confirmed zero.
  if info.streams_known {
    chips = chips.push(track_chip(
      state,
      TrackMenu::Audio,
      &info.audio_streams,
      locale.text("detail-no-audio"),
    ));
    chips = chips.push(track_chip(
      state,
      TrackMenu::Subtitles,
      &info.subtitle_streams,
      locale.text("detail-no-subtitles"),
    ));
    any = true;
  }
  if info.media_source_count > 1 {
    chips = chips.push(
      text(locale.format(
        "detail-media-version",
        &[("count", info.media_source_count.into())],
      ))
      .size(12)
      .line_height(Pixels(16.0))
      .color(palette.text.metadata),
    );
    any = true;
  }
  any.then(|| chips.wrap().vertical_spacing(TOKENS.spacing.s2).into())
}

/// Frame rate chip text: up to three decimals, trailing zeros trimmed so
/// integer rates read "24 fps" rather than "24.000 fps".
fn frame_rate_label(rate: f32) -> String {
  let rate = format!("{rate:.3}");
  let rate = rate.trim_end_matches('0').trim_end_matches('.');
  format!("{rate} fps")
}

/// A non-interactive specification chip: the Paper 22px row treatment with
/// 8px horizontal padding and 6px corners.
fn spec_chip<'a>(palette: &'static ThemePalette, label: String) -> Element<'a, Message> {
  container(
    text(label)
      .size(12)
      .line_height(Pixels(16.0))
      .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
      .color(palette.colors.onControl),
  )
  .padding([3, 8])
  .align_y(Alignment::Center)
  .style(move |_| container::Style {
    background: Some(Background::Color(palette.colors.surfaceContainerHigh)),
    border: iced::Border {
      radius: TOKENS.radii.md.into(),
      color: palette.colors.borderSubtle,
      width: 1.0,
      ..iced::Border::default()
    },
    ..container::Style::default()
  })
  .into()
}

/// The audio or subtitle chip: a read-only track-list trigger summarizing the
/// default (or first) track plus the count of other tracks of that type.
fn track_chip<'a>(
  state: &'a State,
  menu: TrackMenu,
  streams: &'a [VideoStreamInfo],
  empty_label: String,
) -> Element<'a, Message> {
  let palette = state.palette();
  if streams.is_empty() {
    return spec_chip(palette, empty_label);
  }
  let summary = track_summary(state.kernel.locale, streams);
  let icon = match menu {
    TrackMenu::Audio => Icon::AudioTrack,
    TrackMenu::Subtitles => Icon::Subtitles,
  };
  let trigger = control_button_content(
    move |status| {
      let color = match status {
        IconControlState::Rest => palette.colors.onControl,
        IconControlState::Hovered => palette.colors.onControlHover,
        IconControlState::Disabled => palette.text.muted,
      };
      row![
        icon_with_color(icon, IconSize::Custom(12.0), color),
        text(summary.clone())
          .size(12)
          .line_height(Pixels(16.0))
          .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
          .color(color),
      ]
      .spacing(6)
      .align_y(Alignment::Center)
      .into()
    },
    ButtonVariant::Pill,
  )
  .padding([3, 8])
  .radius(TOKENS.radii.md)
  .on_press(Message::Detail(DetailMessage::TrackMenuToggled(menu)));
  let open = state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .track_menu_open
    == Some(menu);
  // The popover still draws this content while its exit animation settles.
  let content = scrollable(track_list(palette, state.kernel.locale, streams))
    .height(Length::Fit.max(280.0))
    .style(jellypilot_ui::theme::scrollable);
  jellypilot_ui::overlay::popover(
    trigger,
    content,
    open,
    jellypilot_ui::overlay::PopoverOptions {
      width: Some(320.0),
      appearance: jellypilot_ui::overlay::PopoverAppearance::TrackList,
      ..Default::default()
    },
    Message::Detail(DetailMessage::TrackMenuDismissed),
  )
}

/// The read-only track rows inside a specification popover: language, format
/// and the file's default marker, without selection or playback controls.
fn track_list<'a>(
  palette: &'static ThemePalette,
  locale: Localizer,
  streams: &'a [VideoStreamInfo],
) -> Element<'a, Message> {
  let mut rows = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
  for stream in streams {
    let mut row = Row::new()
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center)
      .width(Fill);
    row = row.push(
      text(track_label(locale, stream))
        .size(12)
        .line_height(Pixels(16.0))
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
        .color(palette.text.body)
        .width(Fill),
    );
    if stream.is_default {
      row = row.push(
        text(locale.text("detail-default-track"))
          .size(11)
          .color(palette.text.metadata),
      );
    }
    rows = rows.push(row);
  }
  rows.into()
}

/// One track's label: the server-formatted title when present, otherwise the
/// composed language/codec/channel facts. Missing language is labeled unknown.
fn track_label(locale: Localizer, stream: &VideoStreamInfo) -> String {
  let language = nonempty(stream.language.as_deref())
    .map(|code| language_label(locale, code))
    .unwrap_or_else(|| locale.text("common-unknown"));
  if let Some(title) = nonempty(stream.display_title.as_deref()) {
    return if nonempty(stream.language.as_deref()).is_some() {
      title.to_owned()
    } else {
      format!("{language} · {title}")
    };
  }
  let mut values = vec![language];
  if let Some(codec) = nonempty(stream.codec.as_deref()) {
    values.push(codec.to_owned());
  }
  if let Some(layout) = nonempty(stream.channel_layout.as_deref()) {
    values.push(layout.to_owned());
  } else if let Some(channels) = stream.channels {
    values.push(locale.format("detail-channels", &[("count", channels.into())]));
  }
  values.join(" ")
}

/// The chip summary: the explicitly default track when the source marks one,
/// otherwise the first track in server order, plus the count of other tracks
/// of the same type in this source.
fn track_summary(locale: Localizer, streams: &[VideoStreamInfo]) -> String {
  let summary = streams
    .iter()
    .find(|stream| stream.is_default)
    .or_else(|| streams.first())
    .map(|stream| track_label(locale, stream))
    .unwrap_or_default();
  let others = streams.len().saturating_sub(1);
  if others > 0 {
    format!("{summary} +{others}")
  } else {
    summary
  }
}

fn media_info_section<'a>(
  palette: &'static ThemePalette,
  locale: Localizer,
  item: &'a VideoItemDetail,
) -> Element<'a, Message> {
  let Some(info) = &item.media_info else {
    return space::vertical().height(0).into();
  };
  let mut rows: Vec<(String, String)> = Vec::new();
  if let Some(video) = video_info_label(info) {
    rows.push((locale.text("detail-video"), video));
  }
  for stream in &info.audio_streams {
    if let Some(audio) = audio_info_label(locale, stream) {
      rows.push((locale.text("detail-audio"), audio));
    }
  }
  let subtitles = info
    .subtitle_streams
    .iter()
    .filter_map(subtitle_info_label)
    .collect::<Vec<_>>()
    .join(", ");
  if !subtitles.is_empty() {
    rows.push((locale.text("detail-subtitles"), subtitles));
  }
  if let Some(container_name) = nonempty(info.container.as_deref()) {
    rows.push((locale.text("detail-container"), container_name.to_owned()));
  }
  if let Some(size_bytes) = info.size_bytes {
    rows.push((locale.text("detail-size"), humanized_size(size_bytes)));
  }
  if let Some(bitrate_bps) = info.bitrate_bps {
    rows.push((
      locale.text("detail-bitrate"),
      format!("{:.1} Mbps", bitrate_bps as f64 / 1_000_000.0),
    ));
  }
  let title = section_title(palette, locale.text("detail-media-info"));
  // The body shares the section's content edge; rows align label and value
  // columns at wide widths and stack them when narrow.
  column![
    title,
    responsive(move |bounds| -> Element<'_, Message> {
      let narrow = bounds.width < 600.0;
      let mut aligned = Column::new().spacing(TOKENS.spacing.s3).width(Fill);
      for row in rows.iter() {
        aligned = aligned.push(media_info_row(
          palette,
          row.0.clone(),
          row.1.clone(),
          narrow,
        ));
      }
      aligned.into()
    })
    .height(Length::Fit),
  ]
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn media_info_row(
  palette: &'static ThemePalette,
  label: String,
  value: String,
  narrow: bool,
) -> Element<'static, Message> {
  let label = text(label)
    .size(12)
    .line_height(Pixels(16.0))
    .color(palette.text.metadata);
  let value = text(value)
    .size(14)
    .line_height(Pixels(20.0))
    .color(palette.text.secondary)
    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
    .width(Fill);
  if narrow {
    column![label, value]
      .spacing(TOKENS.spacing.s1)
      .width(Fill)
      .into()
  } else {
    row![label.width(160), value]
      .spacing(TOKENS.spacing.s4)
      .width(Fill)
      .into()
  }
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
  if let Some(layout) = nonempty(stream.channel_layout.as_deref()) {
    values.push(layout.to_owned());
  } else if let Some(channels) = stream.channels {
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
  let full = state.full.as_ref().expect("FullUi required");
  let selected = show
    .seasons
    .iter()
    .find(|season| full.detail.data.selected_season_id.as_deref() == Some(season.id.as_str()));
  let trigger = control_button(
    Some(Icon::ChevronDown),
    Some(selected.map_or_else(
      || state.t("detail-choose-season"),
      |season| season.name.clone(),
    )),
    ButtonVariant::Pill,
  )
  .id("detail-season-trigger")
  .trailing_icon(true)
  .spacing(7.0)
  .padding([8, 12])
  .min_height(32.0)
  .label_size(12.0)
  .on_press_maybe((!loading).then_some(Message::Detail(DetailMessage::SeasonMenuToggled)));
  let options = Column::with_children(
    show
      .seasons
      .iter()
      .map(|season| season_button(state, season, loading)),
  )
  .spacing(4)
  .width(Fill);
  let selector = jellypilot_ui::overlay::popover(
    container(trigger).width(Length::Fit.max(320.0)),
    scrollable(options)
      .height(Length::Fit.max(280.0))
      .style(jellypilot_ui::theme::scrollable),
    full.detail.season_menu_open,
    jellypilot_ui::overlay::PopoverOptions {
      width: Some(280.0),
      ..Default::default()
    },
    Message::Detail(DetailMessage::SeasonMenuDismissed),
  );
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
  column![title, selector, episodes].spacing(14).into()
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
    ButtonVariant::PillActive
  } else {
    ButtonVariant::Pill
  };
  control_button(None, Some(season_label(season).to_owned()), variant)
    .padding([8, 12])
    .min_height(32.0)
    .label_size(12.0)
    .width(Fill)
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
    LoadState::Failed(error) => {
      return column![
        section_title(state.palette(), state.t("detail-similar")),
        retryable_surface(
          state.palette(),
          state.kernel.locale,
          error,
          Message::Detail(DetailMessage::Retry)
        ),
      ]
      .spacing(14)
      .into();
    }
    LoadState::Idle | LoadState::Ready(_) => {
      return column![
        section_title(state.palette(), state.t("detail-similar")),
        status_surface(state.palette(), state.t("detail-no-similar")),
      ]
      .spacing(14)
      .into();
    }
  };
  let mut cards = Row::new().spacing(18);
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
  .spacing(14)
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
    control_button_content(
      move |_| ellipsis_text(&item.name)
        .size(12)
        .font(HEADING_FONT)
        .color(state.palette().text.heading)
        .into(),
      ButtonVariant::Text,
    )
    .padding(0)
    .min_height(16.0)
    .width(Fill)
    .on_press(Message::OpenDetail(Box::new(item.clone()))),
    text(item_caption(state.kernel.locale, item))
      .size(12)
      .line_height(Pixels(14.0))
      .color(state.palette().text.metadata),
  ]
  .spacing(8)
  .padding(iced::Padding {
    top: TOKENS.spacing.s2,
    right: 0.0,
    bottom: 0.0,
    left: 0.0,
  })
  .width(Fill);
  poster_card(poster, copy)
    .width(SIMILAR_CARD_WIDTH)
    .on_press(Message::OpenDetail(Box::new(item.clone())))
    .into()
}

fn similar_skeletons<'a>(phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  let mut cards = Row::new().spacing(18);
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
  text(label)
    .font(HEADING_FONT)
    .size(20)
    .line_height(Pixels(24.0))
    .color(palette.text.heading)
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
  .spacing(14)
  .into()
}

fn episode_list<'a>(
  state: &'a State,
  episodes: &'a [VideoLibraryItem],
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let mut cards = Column::new().spacing(20).width(Fill);
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
  responsive(move |bounds| -> Element<'_, Message> {
    let narrow = bounds.width < 720.0;
    let art_width = EPISODE_ART_WIDTH.min(bounds.width);
    let art_height = art_width * EPISODE_ART_HEIGHT / EPISODE_ART_WIDTH;
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
      (Length::Fixed(art_width), Length::Fixed(art_height)),
      ArtworkKind::Episode,
      skeleton_phase,
      reduced_motion,
    );
    let mut art = stack![art].width(art_width).height(art_height);
    if let Some(progress) = playback_progress(episode) {
      art = art.push(
        container(progress_bar(palette, progress))
          .padding(iced::Padding {
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
            left: 1.0,
          })
          .width(Fill)
          .height(Fill)
          .align_y(Alignment::End),
      );
    }
    if episode.played {
      art = art.push(
        container(icon_with_color(
          Icon::CircleCheck,
          IconSize::Custom(24.0),
          palette.colors.tertiary,
        ))
        .padding(8)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::End),
      );
    }
    let art = poster_card(art, space::vertical().height(0))
      .width(art_width)
      .on_press(Message::OpenDetail(Box::new(episode.clone())));
    let mut copy = column![control_button_content(
      move |_| text(format!(
        "{}  {}",
        episode_label(state.kernel.locale, episode),
        episode.name
      ))
      .font(HEADING_FONT)
      .size(16)
      .line_height(Pixels(20.0))
      .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
      .color(palette.text.heading)
      .width(Fill)
      .into(),
      ButtonVariant::Text,
    )
    .padding(0)
    .min_height(20.0)
    .width(Fill)
    .on_press(Message::OpenDetail(Box::new(episode.clone()))),]
    .spacing(8)
    .width(Fill);
    let metadata = episode_metadata(state.kernel.locale, episode);
    if !metadata.is_empty() {
      copy = copy.push(
        text(metadata)
          .size(12)
          .line_height(Pixels(16.0))
          .color(palette.text.metadata),
      );
    }
    if let Some(overview) = nonempty(episode.overview.as_deref()) {
      let expanded = state
        .full
        .as_ref()
        .expect("FullUi required")
        .detail
        .data
        .expanded_episode_ids
        .contains(&episode.id);
      copy = copy.push(overview_copy(
        state,
        overview,
        EPISODE_OVERVIEW_TEXT_SIZE,
        EPISODE_OVERVIEW_LINE_HEIGHT,
        expanded,
        Message::Detail(DetailMessage::EpisodeOverviewToggled(episode.id.clone())),
      ));
    }
    let play = control_button(
      Some(if episode.played {
        Icon::Refresh
      } else {
        Icon::Play
      }),
      Some(state.t(if episode.played {
        "detail-replay"
      } else if has_resume(episode) {
        "detail-episode-resume"
      } else {
        "detail-play"
      })),
      if episode.played {
        ButtonVariant::Tonal
      } else {
        ButtonVariant::Primary
      },
    )
    .icon_size(IconSize::Custom(14.0))
    .spacing(8.0)
    .padding([7, 12])
    .min_height(32.0)
    .label_size(12.0)
    .radius(TOKENS.radii.lg)
    .on_press_maybe(state.playback.view.engine_available.then(|| {
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
    if narrow {
      column![art, copy, play].spacing(14).width(Fill).into()
    } else {
      row![
        art,
        copy,
        container(play)
          .width(EPISODE_ACTION_WIDTH)
          .align_x(Alignment::End)
      ]
      .spacing(20)
      .align_y(Alignment::Center)
      .width(Fill)
      .into()
    }
  })
  .height(Length::Fit)
  .into()
}

fn episode_metadata(locale: Localizer, episode: &VideoLibraryItem) -> String {
  let mut values = Vec::new();
  if let Some(runtime) = episode
    .runtime_seconds
    .and_then(|seconds| runtime_label(locale, seconds))
  {
    values.push(runtime);
  }
  let remaining = episode
    .runtime_seconds
    .zip(episode.resume_position_seconds)
    .filter(|(runtime, position)| {
      has_resume(episode) && runtime.is_finite() && *runtime > *position
    })
    .map(|(runtime, position)| {
      locale.format(
        "detail-remaining",
        &[("duration", locale.duration(runtime - position).into())],
      )
    });
  if let Some(remaining) = remaining {
    values.push(remaining);
  } else if let Some(date) = episode
    .premiere_date
    .as_deref()
    .and_then(|date| episode_premiere_date(locale, date))
  {
    values.push(date);
  }
  values.join(" · ")
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
  Cast,
}

impl ArtworkKind {
  fn initial_size(self) -> u32 {
    match self {
      Self::Hero => 64,
      Self::Similar | Self::Episode => 34,
      Self::Cast => 24,
    }
  }

  fn radius(self) -> iced::border::Radius {
    let radius = match self {
      Self::Similar | Self::Episode => TOKENS.radii.xl,
      Self::Hero => TOKENS.radii.none,
      Self::Cast => TOKENS.radii.full,
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
  let content = if matches!(kind, ArtworkKind::Hero) {
    content
  } else {
    let palette = state.palette();
    stack![
      content,
      container(space::vertical())
        .width(Fill)
        .height(Fill)
        .style(move |_| container::Style {
          border: iced::Border {
            color: palette.colors.imageOutline,
            width: 1.0,
            radius: kind.radius(),
            smoothing: jellypilot_ui::widgets::container::SURFACE_SMOOTHING,
          },
          ..container::Style::default()
        }),
    ]
    .width(size.0)
    .height(size.1)
    .into()
  };
  let axis = if matches!(kind, ArtworkKind::Similar | ArtworkKind::Cast) {
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
  .center_x(width)
  .center_y(height)
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
  responsive(move |bounds| -> Element<'_, Message> {
    let inset = section_inset(bounds.width);
    let content_width = (bounds.width - inset * 2.0).max(1.0);
    let banner = stack![
      skeleton_block_with_radius(
        Fill,
        HERO_HEIGHT,
        full_radius(0.0),
        skeleton_phase,
        reduced_motion
      ),
      container(skeleton_block(
        300.0_f32.min(content_width),
        HERO_LOGO_HEIGHT,
        skeleton_phase,
        reduced_motion
      ))
      .padding(iced::Padding {
        top: 0.0,
        right: 32.0,
        bottom: 24.0,
        left: 32.0
      })
      .width(Fill)
      .height(Fill)
      .align_y(Alignment::End),
      chrome::detail_back(state, bounds.width, HERO_HEIGHT),
    ]
    .width(Fill)
    .height(HERO_HEIGHT);
    let actions = row![
      skeleton_block(120, 36, skeleton_phase, reduced_motion),
      skeleton_block(100, 36, skeleton_phase, reduced_motion),
      skeleton_block(130, 36, skeleton_phase, reduced_motion),
      skeleton_block(140, 36, skeleton_phase, reduced_motion),
    ]
    .spacing(10)
    .wrap();
    let intro = column![
      skeleton_block(content_width.min(500.0), 18, skeleton_phase, reduced_motion),
      skeleton_block(Fill, 48, skeleton_phase, reduced_motion),
      actions,
    ]
    .spacing(14)
    .width(Fill);
    let info = row![
      skeleton_block(content_width.min(360.0), 46, skeleton_phase, reduced_motion),
      skeleton_block(content_width.min(400.0), 46, skeleton_phase, reduced_motion),
    ]
    .spacing(40)
    .wrap();
    let mut cast = Row::new().spacing(24);
    for _ in 0..6 {
      cast = cast.push(
        column![
          skeleton_block_with_radius(
            72,
            72,
            full_radius(TOKENS.radii.full),
            skeleton_phase,
            reduced_motion
          ),
          skeleton_block(90, 16, skeleton_phase, reduced_motion),
          skeleton_block(70, 15, skeleton_phase, reduced_motion),
        ]
        .width(96)
        .spacing(8)
        .align_x(Alignment::Center),
      );
    }
    let cast = column![
      skeleton_block(160, 24, skeleton_phase, reduced_motion),
      scrollable(cast.padding(padding::bottom(12)))
        .direction(Direction::Horizontal(Scrollbar::new()))
        .style(jellypilot_ui::theme::scrollable),
    ]
    .spacing(14);
    let similar = column![
      skeleton_block(180, 24, skeleton_phase, reduced_motion),
      similar_skeletons(skeleton_phase, reduced_motion),
    ]
    .spacing(14);
    scrollable(
      column![
        banner,
        detail_section(intro.into(), inset),
        detail_section(info.into(), inset),
        detail_section(cast.into(), inset),
        detail_section(similar.into(), inset),
      ]
      .padding(padding::bottom(40))
      .width(Fill),
    )
    .id(iced::widget::Id::new("detail-page"))
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
  })
  .height(Length::Fit)
  .into()
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

fn episode_skeletons<'a>(phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  responsive(move |bounds| -> Element<'_, Message> {
    let mut rows = Column::new().spacing(20).width(Fill);
    for _ in 0..3 {
      let art_width = EPISODE_ART_WIDTH.min(bounds.width);
      let art = skeleton_block(
        art_width,
        art_width * EPISODE_ART_HEIGHT / EPISODE_ART_WIDTH,
        phase,
        reduced_motion,
      );
      let copy = column![
        skeleton_block(Fill, 20, phase, reduced_motion),
        skeleton_block(140, 16, phase, reduced_motion),
        skeleton_block(Fill, 40, phase, reduced_motion),
      ]
      .spacing(8)
      .width(Fill);
      let play = skeleton_block(78, 32, phase, reduced_motion);
      let item: Element<'_, Message> = if bounds.width < 720.0 {
        column![art, copy, play].spacing(14).width(Fill).into()
      } else {
        row![art, copy, play]
          .spacing(20)
          .width(Fill)
          .align_y(Alignment::Center)
          .into()
      };
      rows = rows.push(item);
    }
    rows.into()
  })
  .height(Length::Fit)
  .into()
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
        .style(|_| container::Style {
          background: Some(palette.colors.primary.into()),
          border: iced::Border {
            radius: iced::border::Radius {
              top_left: 0.0,
              top_right: TOKENS.radii.full,
              bottom_right: TOKENS.radii.full,
              bottom_left: 0.0,
            },
            ..iced::Border::default()
          },
          ..container::Style::default()
        }),
    );
  }
  if remaining > 0 {
    bar = bar.push(
      container(space::horizontal())
        .width(Length::FillPortion(remaining))
        .height(4)
        .style(|_| {
          iced::widget::container::Style::default()
            .background(palette.text.heading.scale_alpha(0.18))
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

fn item_metadata(
  locale: Localizer,
  item: &jellypilot_media_server::VideoItemDetail,
) -> DetailMetadata {
  let series = nonempty(item.series_name.as_deref()).map(|name| MetadataSeries {
    name: name.to_owned(),
    // Only a real server series id makes the name navigable; without it the
    // name stays plain text rather than a broken link.
    link: item
      .series_id
      .as_deref()
      .is_some_and(|id| !id.trim().is_empty()),
  });
  let mut tail = Vec::new();
  if let (Some(season), Some(episode)) = (item.season_number, item.episode_number) {
    tail.push(format!("S{season:02}E{episode:02}"));
  }
  if let Some(runtime) = item
    .runtime_seconds
    .and_then(|seconds| runtime_label(locale, seconds))
  {
    tail.push(runtime);
  }
  if let Some(rating) = item
    .metadata
    .community_rating
    .filter(|rating| rating.is_finite() && (0.0..=10.0).contains(rating))
  {
    tail.push(format!("{rating:.1}/10"));
  }
  if let Some(rating) = item.metadata.official_rating.as_deref() {
    tail.push(rating.to_owned());
  }
  if let Some(language) = item.original_language.as_deref() {
    tail.push(locale.format(
      "detail-original-language",
      &[("language", language_label(locale, language).into())],
    ));
  }
  if item.can_resume {
    if let Some(progress) = item
      .played_percentage
      .filter(|progress| progress.is_finite() && *progress > 0.0 && *progress < 100.0)
    {
      tail.push(locale.format(
        "detail-watched-percent",
        &[("percent", format!("{progress:.0}").into())],
      ));
    }
  }
  DetailMetadata {
    head: detail_metadata(locale, item),
    series,
    tail,
  }
}

fn show_metadata(locale: Localizer, show: &VideoShowDetail) -> DetailMetadata {
  let mut tail = Vec::new();
  if let Some(rating) = show
    .metadata
    .community_rating
    .filter(|rating| rating.is_finite() && (0.0..=10.0).contains(rating))
  {
    tail.push(format!("{rating:.1}/10"));
  }
  if let Some(rating) = show.metadata.official_rating.as_deref() {
    tail.push(rating.to_owned());
  }
  if let Some(language) = show.original_language.as_deref() {
    tail.push(locale.format(
      "detail-original-language",
      &[("language", language_label(locale, language).into())],
    ));
  }
  DetailMetadata {
    head: show_detail_metadata(locale, show),
    series: None,
    tail,
  }
}

fn runtime_label(locale: Localizer, seconds: f64) -> Option<String> {
  (seconds.is_finite() && seconds > 0.0).then(|| locale.duration(seconds))
}

/// Localized display name for a language code; falls back to the raw server
/// value for languages without a name entry.
fn language_label(locale: Localizer, code: &str) -> String {
  let normalized = jellypilot_media_server::normalize_language(code);
  let id = match normalized.as_deref() {
    Some("ar") => "settings-subtitle-arabic",
    Some("zh") => "settings-subtitle-chinese",
    Some("en") => "settings-subtitle-english",
    Some("fr") => "settings-subtitle-french",
    Some("de") => "settings-subtitle-german",
    Some("hi") => "settings-subtitle-hindi",
    Some("it") => "settings-subtitle-italian",
    Some("ja") => "settings-subtitle-japanese",
    Some("ko") => "settings-subtitle-korean",
    Some("pt") => "settings-subtitle-portuguese",
    Some("ru") => "settings-subtitle-russian",
    Some("es") => "settings-subtitle-spanish",
    _ => return code.to_owned(),
  };
  locale.text(id)
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

#[cfg(test)]
mod tests {
  use super::*;

  fn hero_state() -> State {
    use crate::app::state::{FullUi, LoginState, SettingsState};
    use crate::app::{accounts, kernel::Kernel, playback, shell};
    use jellypilot_auth::login::ConnectionPhase;
    use jellypilot_core::config::SettingsStore;
    use std::sync::Arc;

    let settings = SettingsStore::default();
    let login = crate::app::login::Surface {
      flow: LoginState::from_settings(settings.snapshot()),
      quick_connect_task: None,
    };
    let settings_view = SettingsState::from_settings(settings.snapshot());
    let mut request_gate = jellypilot_core::request_gate::RequestGate::default();
    let playback = playback::Surface::new(&mut request_gate);
    State {
      kernel: Kernel {
        settings,
        locale: Localizer::default(),
        diagnostics: Default::default(),
        auth_store: Default::default(),
        request_gate,
        client: None,
        connection: ConnectionPhase::SignedOut,
        connected_identity: None,
        active_profile: None,
        notice: None,
        active_toast: None,
        next_toast_id: 0,
        tray: None,
        artwork_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
        avatar_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
        profile_avatars: Default::default(),
      },
      image_diagnostics: Default::default(),
      system_theme: iced::theme::Mode::None,
      login,
      settings: crate::app::settings::Surface {
        view: settings_view,
      },
      instance: None,
      full: Some(FullUi::default()),
      playback,
      shell: shell::Surface::new(false),
      watchlist: Default::default(),
      accounts: accounts::Surface::new(),
      motion: Default::default(),
    }
  }

  fn headless_renderer() -> iced::Renderer {
    use iced::advanced::{renderer, renderer::Headless};
    iced::futures::executor::block_on(iced::Renderer::new(
      renderer::Settings {
        font: iced::Font::DEFAULT,
        text_size: 16.0.into(),
        line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    ))
    .expect("headless renderer")
  }

  fn layout_element(
    mut element: Element<'_, Message>,
    renderer: &iced::Renderer,
    width: f32,
  ) -> iced::advanced::layout::Node {
    use iced::advanced::{layout, widget};
    let mut tree = widget::Tree::new(&element);
    tree.diff(element.as_widget_mut());
    element.as_widget_mut().layout(
      &mut tree,
      renderer,
      &layout::Limits::new(iced::Size::ZERO, iced::Size::new(width, f32::INFINITY)),
    )
  }

  fn media_track(title: &str, is_default: bool) -> VideoStreamInfo {
    VideoStreamInfo {
      codec: Some("aac".to_owned()),
      language: Some("eng".to_owned()),
      channels: Some(2),
      channel_layout: Some("stereo".to_owned()),
      is_default,
      display_title: Some(title.to_owned()),
    }
  }

  #[test]
  fn track_summary_prefers_file_default_without_merging_same_language_tracks() {
    let state = hero_state();
    let tracks = [
      media_track("Commentary", false),
      media_track("Original", true),
      media_track("Description", false),
    ];
    assert_eq!(track_summary(state.kernel.locale, &tracks), "Original +2");
    assert_eq!(
      track_summary(state.kernel.locale, &[tracks[0].clone(), tracks[2].clone()]),
      "Commentary +1"
    );
  }

  #[test]
  fn long_specification_labels_fit_their_controls_and_information_rows() {
    let renderer = headless_renderer();
    let state = hero_state();
    let title = "AnUnbrokenServerSuppliedTrackOrSeriesIdentifier".repeat(8);
    let tracks = [media_track(&title, false)];
    let width = 180.0;
    let measure = |text_value: String, size, line_height, width| {
      layout_element(
        text(text_value)
          .size(size)
          .line_height(Pixels(line_height))
          .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
          .into(),
        &renderer,
        width,
      )
      .size()
      .height
    };
    let chip = layout_element(
      track_chip(&state, TrackMenu::Audio, &tracks, String::new()),
      &renderer,
      width,
    );
    assert!(
      chip.size().height >= measure(title.clone(), 12.0, 16.0, width - 34.0) + 6.0,
      "all wrapped track text must remain inside its interactive chip"
    );
    let metadata = DetailMetadata {
      head: String::new(),
      series: Some(MetadataSeries {
        name: title.clone(),
        link: true,
      }),
      tail: Vec::new(),
    };
    let link = layout_element(metadata_row(&state, &metadata), &renderer, width);
    assert!(
      link.size().height >= measure(title.clone(), 14.0, 18.0, width - 16.0),
      "the parent-series link must grow with its complete label"
    );
    let list = layout_element(
      track_list(state.palette(), state.kernel.locale, &tracks),
      &renderer,
      width,
    );
    assert!(
      list.size().height >= measure(title.clone(), 12.0, 16.0, width),
      "unbroken track identifiers must wrap inside the vertical list"
    );
    let info = layout_element(
      media_info_row(state.palette(), "Audio".to_owned(), title.clone(), true),
      &renderer,
      width,
    );
    assert!(
      info.size().height >= measure(title, 14.0, 20.0, width) + 20.0,
      "narrow media information must retain the label and entire value"
    );
    for node in [chip, link, list, info] {
      assert!(node.size().width <= width);
    }
  }

  #[test]
  fn track_popover_opens_from_keyboard_and_dismisses_without_selecting_a_track() {
    use iced::advanced::widget::operation::focusable;
    use iced::keyboard::{key, Event as KeyEvent, Key, Location, Modifiers};
    use iced_runtime::user_interface::{Cache, UserInterface};

    let mut renderer = headless_renderer();
    let mut state = hero_state();
    let tracks = [
      media_track("Original", true),
      media_track("Commentary", false),
    ];
    let bounds = iced::Size::new(200.0, 160.0);
    let mut cache = Cache::new();
    for (open, key_name, physical) in [
      (false, key::Named::Enter, key::Code::Enter),
      (true, key::Named::Escape, key::Code::Escape),
    ] {
      state.full.as_mut().unwrap().detail.track_menu_open = open.then_some(TrackMenu::Audio);
      let mut ui = UserInterface::build(
        track_chip(&state, TrackMenu::Audio, &tracks, String::new()),
        bounds,
        cache,
        &mut renderer,
      );
      let mut focus: Box<dyn iced::advanced::widget::Operation> = Box::new(focusable::focus_next());
      loop {
        ui.operate(&renderer, focus.as_mut());
        match focus.finish() {
          iced::advanced::widget::operation::Outcome::Chain(next) => focus = next,
          _ => break,
        }
      }
      let mut bus = iced::advanced::shell::Bus::new();
      ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[iced::Event::Keyboard(KeyEvent::KeyPressed {
          key: Key::Named(key_name),
          modified_key: Key::Named(key_name),
          physical_key: key::Physical::Code(physical),
          location: Location::Standard,
          modifiers: Modifiers::NONE,
          text: None,
          repeat: false,
        })],
        iced::mouse::Cursor::Unavailable,
        &mut renderer,
        &mut bus,
      );
      let messages: Vec<_> = bus.drain().map(|(message, _)| message).collect();
      if open {
        assert!(matches!(
          messages.as_slice(),
          [Message::Detail(DetailMessage::TrackMenuDismissed)]
        ));
      } else {
        assert!(matches!(
          messages.as_slice(),
          [Message::Detail(DetailMessage::TrackMenuToggled(
            TrackMenu::Audio
          ))]
        ));
      }
      cache = ui.into_cache();
    }
  }

  #[test]
  fn closing_track_popover_keeps_its_size_until_exit_finishes() {
    use iced::advanced::{layout, shell, widget, Layout, Shell};
    use iced::{Event, Point, Rectangle, Size, Vector};

    let renderer = headless_renderer();
    let mut state = hero_state();
    let tracks = [
      media_track("Original", true),
      media_track("Commentary", false),
    ];
    let bounds = Size::new(1568.0, 1018.0);
    let viewport = Rectangle::with_size(bounds);
    let limits = layout::Limits::new(Size::ZERO, bounds);
    state.full.as_mut().unwrap().detail.track_menu_open = Some(TrackMenu::Audio);
    let mut open = track_chip(&state, TrackMenu::Audio, &tracks, String::new());
    let mut tree = widget::Tree::new(open.as_widget());
    open.as_widget_mut().diff(&mut tree);
    let node = open
      .as_widget_mut()
      .layout(&mut tree, &renderer, &limits)
      .move_to(Point::new(525.0, 650.0));
    let open_size = open
      .as_widget_mut()
      .overlay(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &viewport,
        Vector::ZERO,
      )
      .into_iter()
      .next()
      .expect("open track panel")
      .as_overlay_mut()
      .layout(&renderer, bounds)
      .size();
    drop(open);

    state.full.as_mut().unwrap().detail.track_menu_open = None;
    let mut closing = track_chip(&state, TrackMenu::Audio, &tracks, String::new());
    closing.as_widget_mut().diff(&mut tree);
    let node = closing
      .as_widget_mut()
      .layout(&mut tree, &renderer, &limits)
      .move_to(Point::new(525.0, 650.0));
    let closing_size = closing
      .as_widget_mut()
      .overlay(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &viewport,
        Vector::ZERO,
      )
      .into_iter()
      .next()
      .expect("panel remains during exit")
      .as_overlay_mut()
      .layout(&renderer, bounds)
      .size();
    assert_eq!(
      closing_size, open_size,
      "dismissal must not replace the panel with a viewport-height spacer"
    );

    let mut bus = shell::Bus::new();
    closing.as_widget_mut().update(
      &mut tree,
      &Event::Window(iced::window::Event::RedrawRequested(
        iced::time::Instant::now() + TOKENS.durations.ms200,
      )),
      Layout::new(&node),
      iced::mouse::Cursor::Unavailable,
      &renderer,
      &mut Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut bus),
      &viewport,
    );
    assert!(
      closing
        .as_widget_mut()
        .overlay(
          &mut tree,
          Layout::new(&node),
          &renderer,
          &viewport,
          Vector::ZERO
        )
        .is_empty(),
      "the panel must disappear once its exit finishes"
    );
  }

  #[test]
  fn expansion_moves_actions_below_copy_without_resizing_the_hero() {
    let renderer = headless_renderer();
    let mut state = hero_state();
    let overview =
      "A long voyage through unfamiliar lands reveals a forgotten civilization. ".repeat(30);
    for (is_episode, is_movie) in [(false, true), (false, false), (true, false)] {
      for width in [360.0, 720.0, 1220.0] {
        let mut sizes = Vec::new();
        for expanded in [false, true, false] {
          state.full.as_mut().unwrap().detail.data.overview_expanded = expanded;
          let node = layout_element(
            hero(
              &state,
              HeroContent {
                id: "movie",
                name: "A long title for the international extended edition",
                metadata: DetailMetadata {
                  head: "2026 · Adventure".to_owned(),
                  series: None,
                  tail: Vec::new(),
                },
                media_info: None,
                overview: Some(&overview),
                playback_label: state.t("detail-play"),
                playback: None,
                played: false,
                favorite: false,
                is_episode,
                is_movie,
              },
              0.0,
              true,
            ),
            &renderer,
            width,
          );
          let column = &node.children()[0];
          let banner = &column.children()[0];
          let below = &column.children()[1];
          assert_eq!(banner.size(), iced::Size::new(width, HERO_HEIGHT));
          assert!(below.bounds().y >= HERO_HEIGHT);
          let copy = &below.children()[0];
          let actions = copy.children().last().unwrap();
          let reference = layout_element(
            detail_actions(&state, state.t("detail-play"), None, "movie", false, false),
            &renderer,
            width - section_inset(width) * 2.0,
          );
          assert_eq!(
            actions.size(),
            reference.size(),
            "actions retain their full hit targets"
          );
          assert!(
            actions.bounds().y >= copy.children()[1].bounds().y + copy.children()[1].size().height
          );
          sizes.push(node.size().height);
        }
        assert!(sizes[1] > sizes[0], "expanded text must remain reachable");
        assert_eq!(
          sizes[0], sizes[2],
          "collapsing restores the original layout"
        );
      }
    }
  }

  fn episode_with_progress(
    resume_position_seconds: Option<f64>,
    played_percentage: Option<f64>,
    played: bool,
  ) -> VideoLibraryItem {
    VideoLibraryItem {
      community_rating: None,
      episode_count: None,
      last_played_date: None,
      id: "episode-1".to_owned(),
      name: "Pilot".to_owned(),
      item_type: "Episode".to_owned(),
      production_year: None,
      premiere_date: None,
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
  fn episode_actions_reflow_and_keep_their_real_playback_start_position() {
    use iced::advanced::{layout, mouse, widget, Layout, Shell};
    use iced::{Event, Rectangle, Size};
    let renderer = headless_renderer();
    let mut state = hero_state();
    for (played, offset, expected) in [
      (false, None, PlaybackStartPosition::Beginning),
      (false, Some(120.0), PlaybackStartPosition::Resume),
      (true, Some(120.0), PlaybackStartPosition::Beginning),
    ] {
      let episode = episode_with_progress(offset, Some(40.0), played);
      for width in [324.0, 1148.0] {
        for available in [true, false] {
          state.playback.view.engine_available = available;
          let mut element = episode_card(&state, &episode, false, 0.0, true);
          let mut tree = widget::Tree::new(&element);
          tree.diff(element.as_widget_mut());
          let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY)),
          );
          assert!(node.size().height.is_finite());
          let root = Layout::new(&node).children().next().unwrap();
          let mut action = root.children().last().unwrap();
          if width >= 720.0 {
            action = action.children().next().unwrap();
          }
          let bounds = action.bounds();
          assert!(bounds.width > 0.0 && bounds.x + bounds.width <= width);
          assert_eq!(bounds.height, 32.0);
          let cursor = mouse::Cursor::Available(bounds.center());
          let mut bus = iced::advanced::shell::Bus::new();
          for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
          ] {
            element.as_widget_mut().update(
              &mut tree,
              &Event::Mouse(event),
              Layout::new(&node),
              cursor,
              &renderer,
              &mut Shell::new(
                &iced::window::Headless,
                iced::advanced::shell::Waker::noop(),
                &mut bus,
              ),
              &Rectangle::with_size(node.size()),
            );
          }
          let messages: Vec<_> = bus.drain().map(|(message, _)| message).collect();
          if available {
            assert!(matches!(messages.as_slice(),
              [Message::Playback(PlaybackMessage::Intent(intent))]
                if matches!(intent.as_ref(), PlaybackIntent::Start { position, .. } if *position == expected)
            ));
          } else {
            assert!(
              messages.is_empty(),
              "missing engine must leave the displayed action disabled"
            );
          }
        }
      }
    }
  }

  #[test]
  fn episode_expansion_survives_unrelated_layout_and_reflows_without_clipping_actions() {
    use iced::advanced::{layout, widget};
    let renderer = headless_renderer();
    let mut tree = widget::Tree::empty();
    let mut state = hero_state();
    let mut episode = episode_with_progress(None, None, false);
    episode.overview =
      Some("A long account of the journey and the consequences of each decision. ".repeat(40));
    state
      .full
      .as_mut()
      .unwrap()
      .detail
      .data
      .expanded_episode_ids
      .insert(episode.id.clone());
    for width in [1148.0, 324.0, 1148.0] {
      let mut element = episode_card(&state, &episode, false, 0.0, true);
      tree.diff(element.as_widget_mut());
      let expanded = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(iced::Size::ZERO, iced::Size::new(width, f32::INFINITY)),
      );
      let content = &expanded.children()[0];
      let art = &content.children()[0];
      let copy = &content.children()[1];
      let action = &content.children()[2];
      assert_eq!(art.size().width, 300.0);
      assert!(copy.size().height > 169.0);
      assert!(action.bounds().y + action.size().height <= expanded.size().height);
    }
  }

  #[test]
  fn cast_carousel_wraps_unbroken_names_without_expanding_the_page() {
    let renderer = headless_renderer();
    let state = hero_state();
    let cast = vec![VideoCastMember {
      name: "AnUnbrokenInternationalCastMemberName".repeat(4),
      role: Some("A character with several names and titles".repeat(3)),
      image_id: None,
    }];
    let node = layout_element(cast_section(&state, &cast, 0.0, true), &renderer, 324.0);
    assert_eq!(node.size().width, 324.0);
    assert!(node.size().height.is_finite());
    let person = &node.children()[1].children()[0].children()[0];
    let name = &person.children()[1];
    let role = &person.children()[2];
    assert!(name.size().width <= 96.0 && role.size().width <= 96.0);
    assert!(
      name.size().height > 16.0,
      "the full unbroken name must wrap rather than truncate"
    );
    assert!(role.bounds().y >= name.bounds().y + name.size().height);
    assert!(role.bounds().y + role.size().height <= person.size().height);
  }
}
