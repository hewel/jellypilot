use crate::app::message::{DetailMessage, Message, PlaybackMessage};
use crate::app::state::{ArtworkCell, ArtworkCellState, State, UserDataActionKind};
use iced::advanced::graphics::text::Paragraph as GraphicsParagraph;
use iced::advanced::text::paragraph::Paragraph;
use iced::advanced::{text as advanced_text, Text as AdvancedText};
use iced::gradient;
use iced::widget::{
  button, column, container, responsive, row, scrollable, space, stack, text, Column, Row,
};
use iced::{
  alignment, Alignment, Background, ContentFit, Degrees, Element, Fill, Font, Length, Pixels, Size,
};
use jellypilot_core::detail::{
  detail_episode_key, detail_metadata, show_detail_metadata, DetailContent,
};
use jellypilot_core::LoadState;
use jellypilot_media_server::{VideoLibraryItem, VideoSeason, VideoShowDetail};
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{
  icon_for_variant, icon_for_variant_disabled, icon_with_color, Icon, IconSize,
};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::skeleton::{skeleton_block, skeleton_panel};
use jellypilot_ui::{full_radius, rounded_image};

const HERO_HEIGHT: f32 = 430.0;
const POSTER_WIDTH: f32 = 220.0;
const POSTER_HEIGHT: f32 = 330.0;
const EPISODE_ART_WIDTH: f32 = 240.0;
const EPISODE_ART_HEIGHT: f32 = 135.0;
const OVERVIEW_TEXT_SIZE: f32 = 15.0;
const OVERVIEW_COLLAPSED_LINES: f32 = 4.0;
const DETAIL_POSTER_KEY: &str = "detail-poster";
const DETAIL_BACKDROP_KEY: &str = "detail-backdrop";

pub fn view(state: &State) -> Element<'_, Message> {
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  match &state.detail.data.content {
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
        &item.genres,
        &item.metadata.creators,
        &item.metadata.cast,
      ));
      if item.item_type.eq_ignore_ascii_case("episode") {
        page = page.push(neighbor_section(state, skeleton_phase, reduced_motion));
      }
    }
    DetailContent::Show(show) => {
      page = page.push(summary(
        &show.genres,
        &show.metadata.creators,
        &show.metadata.cast,
      ));
      if let Some(next) = &show.next_episode {
        page = page.push(next_up_section(state, next, skeleton_phase, reduced_motion));
      }
      page = page.push(seasons_section(state, show, skeleton_phase, reduced_motion));
    }
  }

  scrollable(page.padding([TOKENS.spacing.s8, TOKENS.spacing.s6]))
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
  let playback_label = if item.can_resume { "Resume" } else { "Play" };
  let position = if item.can_resume {
    PlaybackStartPosition::Resume
  } else {
    PlaybackStartPosition::Beginning
  };
  hero(
    state,
    HeroContent {
      name: &item.name,
      metadata: item_metadata(item),
      overview: item.overview.as_deref(),
      playback_label: playback_label.to_owned(),
      playback: item
        .can_play
        .then(|| (Playable::Detail(item.clone()), position)),
      played: item.played,
      favorite: item.favorite,
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
    || "Play".to_owned(),
    |episode| {
      let action = if has_resume(episode) {
        "Continue"
      } else {
        "Play"
      };
      format!("{action} {}", episode_label(episode))
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
      name: &show.name,
      metadata: show_metadata(show),
      overview: show.overview.as_deref(),
      playback_label,
      playback,
      played: show.played,
      favorite: show.favorite,
    },
    skeleton_phase,
    reduced_motion,
  )
}

struct HeroContent<'a> {
  name: &'a str,
  metadata: String,
  overview: Option<&'a str>,
  playback_label: String,
  playback: Option<(Playable, PlaybackStartPosition)>,
  played: bool,
  favorite: bool,
}

fn hero<'a>(
  state: &'a State,
  content: HeroContent<'a>,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  responsive(move |bounds| {
    hero_at_width(
      state,
      &content,
      bounds.width,
      skeleton_phase,
      reduced_motion,
    )
  })
  .height(Length::Shrink)
  .into()
}

fn hero_at_width<'a>(
  state: &'a State,
  content: &HeroContent<'a>,
  width: f32,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let name = content.name;
  let overview = content.overview.filter(|value| !value.trim().is_empty());
  let copy_width = (width - (TOKENS.spacing.s6 * 2.0) - POSTER_WIDTH - TOKENS.spacing.s8).max(1.0);
  let collapsed_height = overview_collapsed_height();
  let measured_height = overview.map_or(0.0, |value| overview_height(value, copy_width));
  let overview_expandable = overview_is_expandable(measured_height, collapsed_height);
  let overview_expanded = overview_expandable && state.detail.data.overview_expanded;
  let hero_height = if overview_expanded {
    HERO_HEIGHT + (measured_height - collapsed_height).max(0.0)
  } else {
    HERO_HEIGHT
  };

  let backdrop = artwork(
    state,
    DETAIL_BACKDROP_KEY,
    name,
    (Fill, Length::Fixed(hero_height)),
    64,
    skeleton_phase,
    reduced_motion,
  );
  let gradient = gradient::Linear::new(Degrees(180.0))
    .add_stop(0.0, TOKENS.colors.surfaceContainerLowest.scale_alpha(0.0))
    .add_stop(1.0, TOKENS.colors.surfaceContainerLowest.scale_alpha(0.85));
  let scrim = container(space::vertical())
    .width(Fill)
    .height(hero_height)
    .style(move |_| {
      iced::widget::container::Style::default().background(Background::Gradient(gradient.into()))
    });

  let back_enabled = !state.shell.navigation_stack.is_empty();
  let back = button(
    row![
      icon_for_variant_disabled(
        Icon::ChevronLeft,
        IconSize::Sm,
        ButtonVariant::Tonal,
        !back_enabled,
      ),
      text("Back").size(14),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([6, 10])
  .on_press_maybe(back_enabled.then_some(Message::Detail(DetailMessage::Back)))
  .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal));
  let poster = artwork(
    state,
    DETAIL_POSTER_KEY,
    name,
    (Length::Fixed(POSTER_WIDTH), Length::Fixed(POSTER_HEIGHT)),
    54,
    skeleton_phase,
    reduced_motion,
  );
  let mut copy = Column::new()
    .spacing(TOKENS.spacing.s3)
    .width(Fill)
    .push(
      text(content.metadata.clone())
        .size(15)
        .color(TOKENS.colors.onSurfaceVariant),
    )
    .push(
      text(name)
        .font(SPACE_GROTESK_FONT)
        .size(45)
        .color(TOKENS.colors.onSurface),
    );

  if let Some(overview) = overview {
    if overview_expandable && !overview_expanded {
      copy = copy.push(
        container(
          text(overview)
            .size(OVERVIEW_TEXT_SIZE)
            .color(TOKENS.colors.onSurfaceVariant),
        )
        .width(Fill)
        .height(collapsed_height)
        .clip(true),
      );
    } else {
      copy = copy.push(
        text(overview)
          .size(OVERVIEW_TEXT_SIZE)
          .color(TOKENS.colors.onSurfaceVariant),
      );
    }
    if overview_expandable {
      let (overview_label, overview_icon) = if overview_expanded {
        ("Less", Icon::ChevronUp)
      } else {
        ("More", Icon::ChevronDown)
      };
      copy = copy.push(
        button(
          row![
            text(overview_label),
            icon_for_variant(overview_icon, IconSize::Xs, ButtonVariant::Text),
          ]
          .spacing(TOKENS.spacing.s1)
          .align_y(Alignment::Center),
        )
        .padding([5, 8])
        .on_press(Message::Detail(DetailMessage::OverviewToggled))
        .style(|theme, status| {
          jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text)
        }),
      );
    }
  }
  copy = copy.push(detail_actions(
    state,
    content.playback_label.clone(),
    content.playback.clone(),
    content.played,
    content.favorite,
  ));

  let foreground = column![
    back,
    row![poster, copy]
      .spacing(TOKENS.spacing.s8)
      .align_y(Alignment::End),
  ]
  .spacing(TOKENS.spacing.s5)
  .padding(TOKENS.spacing.s6)
  .width(Fill)
  .height(hero_height);

  stack![backdrop, scrim, foreground]
    .width(Fill)
    .height(hero_height)
    .into()
}

fn detail_actions<'a>(
  state: &'a State,
  playback_label: String,
  playback_target: Option<(Playable, PlaybackStartPosition)>,
  played: bool,
  favorite: bool,
) -> Element<'a, Message> {
  let playback_enabled = playback_target.is_some() && state.playback.view.engine_available;
  let playback = button(
    row![
      icon_for_variant_disabled(
        Icon::Play,
        IconSize::Md,
        ButtonVariant::Primary,
        !playback_enabled,
      ),
      text(playback_label),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  )
  .padding([8, 16])
  .on_press_maybe(
    playback_target
      .filter(|_| playback_enabled)
      .map(|(item, position)| playback_message(state, item, position)),
  )
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
  });
  let any_busy = state.detail.data.user_data_busy.is_some();
  let (fav_icon, fav_label, fav_variant) = if favorite {
    (Icon::HeartFilled, "Favorited", ButtonVariant::TonalActive)
  } else {
    (Icon::Heart, "Favorite", ButtonVariant::Tonal)
  };
  let favorite_button = button(
    row![
      icon_for_variant_disabled(fav_icon, IconSize::Md, fav_variant, any_busy),
      text(fav_label),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  )
  .padding([8, 14])
  .on_press_maybe((!any_busy).then_some(Message::Detail(DetailMessage::FavoriteToggled)))
  .style(move |theme, status| jellypilot_ui::theme::button_variant(theme, status, fav_variant));
  let (played_icon, played_label, played_variant) = if played {
    (Icon::CircleCheck, "Played", ButtonVariant::TonalActive)
  } else {
    (Icon::Circle, "Mark played", ButtonVariant::Tonal)
  };
  let played_button = button(
    row![
      icon_for_variant_disabled(played_icon, IconSize::Md, played_variant, any_busy),
      text(played_label),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  )
  .padding([8, 14])
  .on_press_maybe((!any_busy).then_some(Message::Detail(DetailMessage::PlayedToggled)))
  .style(move |theme, status| jellypilot_ui::theme::button_variant(theme, status, played_variant));
  let mut actions = Row::new()
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center)
    .push(playback)
    .push(favorite_button)
    .push(played_button);
  if let Some(kind) = state.detail.data.user_data_busy {
    actions = actions.push(
      text(match kind {
        UserDataActionKind::Favorite => "Updating favorite…",
        UserDataActionKind::Played => "Updating played state…",
      })
      .size(13)
      .color(TOKENS.colors.onSurfaceVariant),
    );
  }
  let mut content = Column::new().spacing(TOKENS.spacing.s2).push(actions);
  if let Some(error) = &state.detail.data.user_data_error {
    content = content.push(text(error).size(13).color(TOKENS.colors.error));
  }
  content.into()
}

fn summary<'a>(
  genres: &'a [String],
  creators: &'a [String],
  cast: &'a [String],
) -> Element<'a, Message> {
  if genres.is_empty() && creators.is_empty() && cast.is_empty() {
    return space::vertical().height(0).into();
  }
  let mut columns = Row::new().spacing(TOKENS.spacing.s8).width(Fill);
  if !genres.is_empty() {
    columns = columns.push(summary_column("Genres", genres.join(" • ")));
  }
  if !creators.is_empty() {
    columns = columns.push(summary_column("Creators", limited_people(creators, 2)));
  }
  if !cast.is_empty() {
    columns = columns.push(summary_column("Cast", limited_people(cast, 4)));
  }
  container(columns)
    .padding(TOKENS.spacing.s5)
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
    .into()
}

fn summary_column(label: &'static str, values: String) -> Element<'static, Message> {
  column![
    text(label).size(12).color(TOKENS.colors.onSurfaceVariant),
    text(values).size(14).color(TOKENS.colors.onSurface),
  ]
  .spacing(TOKENS.spacing.s2)
  .width(Fill)
  .into()
}

fn seasons_section<'a>(
  state: &'a State,
  show: &'a VideoShowDetail,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let title = text("Seasons")
    .font(SPACE_GROTESK_FONT)
    .size(26)
    .color(TOKENS.colors.onSurface);
  if show.seasons.is_empty() {
    return column![title, status_surface("No seasons available")]
      .spacing(TOKENS.spacing.s3)
      .into();
  }
  let loading = matches!(state.detail.data.season_episodes, LoadState::Loading);
  let mut season_buttons = Row::new().spacing(TOKENS.spacing.s2);
  for season in &show.seasons {
    season_buttons = season_buttons.push(season_button(state, season, loading));
  }
  let selector = scrollable(season_buttons)
    .direction(iced::widget::scrollable::Direction::Horizontal(
      iced::widget::scrollable::Scrollbar::new(),
    ))
    .height(48)
    .style(jellypilot_ui::theme::scrollable);
  let episodes = match &state.detail.data.season_episodes {
    LoadState::Idle => status_surface("Choose a season"),
    LoadState::Loading => episode_skeletons(skeleton_phase, reduced_motion),
    LoadState::Failed(error) => {
      retryable_surface(error, Message::Detail(DetailMessage::RetrySeason))
    }
    LoadState::Ready(page) if page.episodes.is_empty() => {
      status_surface("Jellyfin returned no episodes for this season.")
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
  let active = state.detail.data.selected_season_id.as_deref() == Some(season.id.as_str());
  button(text(season_label(season)))
    .padding([6, 12])
    .on_press_maybe(
      (!loading).then_some(Message::Detail(DetailMessage::SeasonSelected(
        season.id.clone(),
      ))),
    )
    .style(move |theme, status| {
      jellypilot_ui::theme::button_variant(
        theme,
        status,
        if active {
          ButtonVariant::TonalActive
        } else {
          ButtonVariant::Tonal
        },
      )
    })
    .into()
}

fn neighbor_section(
  state: &State,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'_, Message> {
  let title = text("More from this season")
    .font(SPACE_GROTESK_FONT)
    .size(26)
    .color(TOKENS.colors.onSurface);
  let body = match &state.detail.data.season_neighbors {
    LoadState::Idle => return space::vertical().height(0).into(),
    LoadState::Loading => episode_skeletons(skeleton_phase, reduced_motion),
    LoadState::Failed(error) => {
      retryable_surface(error, Message::Detail(DetailMessage::RetryNeighbors))
    }
    LoadState::Ready(items) if items.is_empty() => {
      status_surface("No neighboring episodes are available.")
    }
    LoadState::Ready(items) => episode_list(state, items, skeleton_phase, reduced_motion),
  };
  column![title, body].spacing(TOKENS.spacing.s3).into()
}
fn next_up_section<'a>(
  state: &'a State,
  episode: &'a VideoLibraryItem,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  column![
    text("Next Up")
      .font(SPACE_GROTESK_FONT)
      .size(26)
      .color(TOKENS.colors.onSurface),
    episode_card(state, episode, skeleton_phase, reduced_motion),
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
    cards = cards.push(episode_card(state, episode, skeleton_phase, reduced_motion));
  }
  cards.into()
}

fn episode_card<'a>(
  state: &'a State,
  episode: &'a VideoLibraryItem,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let key = detail_episode_key(&episode.id);
  let art = artwork(
    state,
    &key,
    &episode.name,
    (
      Length::Fixed(EPISODE_ART_WIDTH),
      Length::Fixed(EPISODE_ART_HEIGHT),
    ),
    34,
    skeleton_phase,
    reduced_motion,
  );
  let mut copy = Column::new().spacing(TOKENS.spacing.s2).width(Fill).push(
    text(format!("{}  {}", episode_label(episode), episode.name))
      .font(SPACE_GROTESK_FONT)
      .size(18)
      .color(TOKENS.colors.onSurface),
  );
  if let Some(overview) = episode
    .overview
    .as_deref()
    .filter(|overview| !overview.trim().is_empty())
  {
    copy = copy.push(
      text(overview)
        .size(13)
        .color(TOKENS.colors.onSurfaceVariant),
    );
  }
  if let Some(progress) = playback_progress(episode) {
    copy = copy.push(progress_bar(progress));
  }
  let play_label = if has_resume(episode) {
    "Resume"
  } else {
    "Play"
  };
  let play_enabled = state.playback.view.engine_available;
  let play = button(
    row![
      icon_for_variant_disabled(
        Icon::Play,
        IconSize::Sm,
        ButtonVariant::Primary,
        !play_enabled,
      ),
      text(play_label),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
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
  }))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
  });
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
  Message::Playback(PlaybackMessage::Intent(PlaybackIntent::Start {
    item,
    position,
    intro: state.kernel.intro_availability(),
    selection: Box::default(),
  }))
}

fn artwork<'a>(
  state: &'a State,
  key: &str,
  name: &'a str,
  (width, height): (Length, Length),
  initial_size: u32,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let radius = full_radius(TOKENS.radii.lg);
  let cell = state.detail.artwork.get(key);
  if let Some(ArtworkCell {
    slot,
    image_id,
    state: ArtworkCellState::Ready,
  }) = cell
  {
    if let Some(handle) = state.kernel.artwork_handles.get(*slot, image_id) {
      return rounded_image(handle.clone(), radius)
        .content_fit(ContentFit::Cover)
        .width(width)
        .height(height)
        .into();
    }
  }
  let failed = cell.is_some_and(|cell| cell.state == ArtworkCellState::Failed);
  if failed {
    let placeholder_color = TOKENS.colors.warning;
    let initial = name
      .trim()
      .chars()
      .next()
      .map(|character| character.to_uppercase().collect::<String>())
      .unwrap_or_else(|| "•".to_owned());
    let icon_dim = (initial_size as f32).max(28.0);
    return container(
      column![
        icon_with_color(Icon::Movie, icon_dim, placeholder_color),
        text(initial)
          .font(SPACE_GROTESK_FONT)
          .size(initial_size.min(28))
          .color(placeholder_color),
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
        TOKENS.colors.surfaceContainerLowest,
      )),
      border: iced::Border {
        radius,
        width: 0.0,
        color: iced::Color::TRANSPARENT,
      },
      ..container::Style::default()
    })
    .into();
  }

  skeleton_panel(
    width,
    height,
    TOKENS.colors.surfaceContainerLowest,
    radius,
    phase,
    reduced_motion,
  )
  .into()
}

fn detail_skeleton(
  state: &State,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'_, Message> {
  let back_enabled = !state.shell.navigation_stack.is_empty();
  let back = button(
    row![
      icon_for_variant_disabled(
        Icon::ChevronLeft,
        IconSize::Sm,
        ButtonVariant::Tonal,
        !back_enabled,
      ),
      text("Back"),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([6, 10])
  .on_press_maybe(back_enabled.then_some(Message::Detail(DetailMessage::Back)))
  .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal));
  let body = column![
    back,
    row![
      skeleton_block(POSTER_WIDTH, POSTER_HEIGHT, skeleton_phase, reduced_motion),
      column![
        skeleton_block(240.0, 18.0, skeleton_phase, reduced_motion),
        skeleton_block(520.0, 48.0, skeleton_phase, reduced_motion),
        skeleton_block(680.0, 96.0, skeleton_phase, reduced_motion),
        skeleton_block(430.0, 42.0, skeleton_phase, reduced_motion),
      ]
      .spacing(TOKENS.spacing.s4),
    ]
    .spacing(TOKENS.spacing.s8),
  ]
  .spacing(TOKENS.spacing.s5)
  .padding(TOKENS.spacing.s6);
  container(body).width(Fill).height(Fill).into()
}

fn detail_failure<'a>(state: &State, error: &'a str) -> Element<'a, Message> {
  let back_enabled = !state.shell.navigation_stack.is_empty();
  let back = button(
    row![
      icon_for_variant_disabled(
        Icon::ChevronLeft,
        IconSize::Sm,
        ButtonVariant::Tonal,
        !back_enabled,
      ),
      text("Back"),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([6, 10])
  .on_press_maybe(back_enabled.then_some(Message::Detail(DetailMessage::Back)))
  .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal));
  let retry = button(
    row![
      icon_for_variant(Icon::Refresh, IconSize::Sm, ButtonVariant::Primary),
      text("Retry"),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([6, 12])
  .on_press(Message::Detail(DetailMessage::Retry))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
  });
  container(
    column![
      back,
      text("Could not load detail")
        .font(SPACE_GROTESK_FONT)
        .size(28)
        .color(TOKENS.colors.onSurface),
      text(error).size(14).color(TOKENS.colors.error),
      retry,
    ]
    .spacing(TOKENS.spacing.s3),
  )
  .padding(TOKENS.spacing.s6)
  .width(Fill)
  .height(Fill)
  .into()
}

fn retryable_surface<'a>(error: &'a str, retry: Message) -> Element<'a, Message> {
  container(
    row![
      text(error).size(13).color(TOKENS.colors.error),
      button(
        row![
          icon_for_variant(Icon::Refresh, IconSize::Xs, ButtonVariant::Tonal),
          text("Retry"),
        ]
        .spacing(TOKENS.spacing.s1)
        .align_y(Alignment::Center),
      )
      .padding([6, 10])
      .on_press(retry)
      .style(|theme, status| {
        jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal)
      }),
    ]
    .spacing(TOKENS.spacing.s3)
    .align_y(Alignment::Center),
  )
  .padding(TOKENS.spacing.s3)
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
  .into()
}

fn status_surface(message: &str) -> Element<'_, Message> {
  container(text(message).size(14).color(TOKENS.colors.onSurfaceVariant))
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

fn progress_bar<'a>(progress: f64) -> Element<'a, Message> {
  let filled = (progress.round() as u16).min(100);
  let remaining = 100_u16.saturating_sub(filled);
  let mut bar = Row::new().width(Fill).height(4);
  if filled > 0 {
    bar = bar.push(
      container(space::horizontal())
        .width(Length::FillPortion(filled))
        .height(4)
        .style(|_| iced::widget::container::Style::default().background(TOKENS.colors.primary)),
    );
  }
  if remaining > 0 {
    bar = bar.push(
      container(space::horizontal())
        .width(Length::FillPortion(remaining))
        .height(4)
        .style(|_| {
          iced::widget::container::Style::default().background(TOKENS.colors.surfaceContainerLow)
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

fn item_metadata(item: &jellypilot_media_server::VideoItemDetail) -> String {
  let mut values = vec![detail_metadata(item)];
  if let (Some(series), Some(season), Some(episode)) = (
    item.series_name.as_deref(),
    item.season_number,
    item.episode_number,
  ) {
    values.push(format!("{series} · S{season:02}E{episode:02}"));
  } else if let Some(series) = item.series_name.as_deref() {
    values.push(series.to_owned());
  }
  if let Some(runtime) = item.runtime_seconds.and_then(runtime_label) {
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
      values.push(format!("{progress:.0}% watched"));
    }
  }
  values.join(" · ")
}

fn show_metadata(show: &VideoShowDetail) -> String {
  let mut values = vec![show_detail_metadata(show)];
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

fn runtime_label(seconds: f64) -> Option<String> {
  if !seconds.is_finite() || seconds <= 0.0 {
    return None;
  }
  let minutes = (seconds / 60.0).round() as u64;
  let hours = minutes / 60;
  let remainder = minutes % 60;
  if hours == 0 {
    Some(format!("{minutes} min"))
  } else if remainder == 0 {
    Some(format!("{hours} hr"))
  } else {
    Some(format!("{hours} hr {remainder} min"))
  }
}

fn has_resume(item: &VideoLibraryItem) -> bool {
  !item.played
    && item
      .resume_position_seconds
      .is_some_and(|position| position.is_finite() && position > 0.0)
}

fn episode_label(episode: &VideoLibraryItem) -> String {
  match (episode.season_number, episode.episode_number) {
    (Some(season), Some(number)) => format!("S{season:02}E{number:02}"),
    _ => "Episode".to_owned(),
  }
}

fn season_label(season: &VideoSeason) -> &str {
  &season.name
}

fn overview_height(overview: &str, width: f32) -> f32 {
  let size = Pixels(OVERVIEW_TEXT_SIZE);
  let paragraph = GraphicsParagraph::with_text(AdvancedText {
    content: overview,
    bounds: Size::new(width, f32::INFINITY),
    size,
    line_height: advanced_text::LineHeight::default(),
    font: Font::DEFAULT,
    align_x: advanced_text::Alignment::Default,
    align_y: alignment::Vertical::Top,
    shaping: advanced_text::Shaping::default(),
    wrapping: advanced_text::Wrapping::Word,
  });
  paragraph.min_height()
}

fn overview_collapsed_height() -> f32 {
  f32::from(advanced_text::LineHeight::default().to_absolute(Pixels(OVERVIEW_TEXT_SIZE)))
    * OVERVIEW_COLLAPSED_LINES
}

fn overview_is_expandable(measured_height: f32, collapsed_height: f32) -> bool {
  measured_height > collapsed_height
}

fn limited_people(people: &[String], limit: usize) -> String {
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
    format!("{visible}  +{extra} more")
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
      series_poster_image_id: None,
      season_number: Some(1),
      episode_number: Some(1),
      series_id: Some("show-1".to_owned()),
      series_name: Some("Show".to_owned()),
      resume_position_seconds,
      played_percentage,
      overview: None,
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
    let collapsed_height = overview_collapsed_height();

    assert!(!overview_is_expandable(collapsed_height, collapsed_height));
  }

  #[test]
  fn overview_taller_than_the_collapsed_height_is_expandable() {
    let collapsed_height = overview_collapsed_height();

    assert!(overview_is_expandable(
      collapsed_height + 1.0,
      collapsed_height
    ));
  }

  #[test]
  fn overview_measurement_uses_the_available_width() {
    let overview = "A detailed overview with enough words to wrap across several lines when the \
      available width is narrow, while fitting into fewer lines when more width is available.";

    assert!(overview_height(overview, 120.0) > overview_height(overview, 800.0));
  }

  #[test]
  fn detail_view_renders_in_loading_state() {
    let mut state = State::boot(false);
    state.shell.skeleton_phase = 0.42;
    state.detail.data.content = LoadState::Loading;
    let _element = view(&state);
  }

  #[test]
  fn detail_seasons_and_neighbors_render_episode_skeletons_when_loading() {
    {
      let mut state = State::boot(false);
      state.shell.skeleton_phase = 0.75;
      state.detail.data.content = LoadState::Ready(DetailContent::Show(VideoShowDetail {
        id: "show-1".to_owned(),
        name: "Show 1".to_owned(),
        overview: None,
        production_year: None,
        genres: Vec::new(),
        played: false,
        favorite: false,
        can_play: true,
        artwork_image_id: None,
        backdrop_image_id: None,
        next_episode: None,
        seasons: vec![VideoSeason {
          id: "season-1".to_owned(),
          name: "Season 1".to_owned(),
          season_number: Some(1),
          played: false,
          favorite: false,
          artwork_image_id: None,
        }],
        metadata: Default::default(),
      }));
      state.detail.data.season_episodes = LoadState::Loading;
      let _element = view(&state);
    }

    {
      let mut state = State::boot(false);
      state.shell.skeleton_phase = 0.75;
      let item = jellypilot_media_server::VideoItemDetail {
        id: "ep-1".to_owned(),
        name: "Ep 1".to_owned(),
        item_type: "Episode".to_owned(),
        overview: None,
        production_year: None,
        runtime_seconds: None,
        series_id: None,
        series_name: None,
        season_number: Some(1),
        episode_number: Some(1),
        genres: Vec::new(),
        played: false,
        favorite: false,
        played_percentage: None,
        resume_position_seconds: None,
        can_resume: false,
        can_play: true,
        artwork_image_id: None,
        backdrop_image_id: None,
        series_poster_image_id: None,
        metadata: Default::default(),
      };
      state.detail.data.content = LoadState::Ready(DetailContent::Item(item));
      state.detail.data.season_neighbors = LoadState::Loading;
      let _element = view(&state);
    }
  }

  #[test]
  fn detail_view_renders_hero_and_episodes_with_loading_and_failed_artwork() {
    let mut state = State::boot(false);
    state.shell.skeleton_phase = 0.5;
    let item = jellypilot_media_server::VideoItemDetail {
      id: "ep-1".to_owned(),
      name: "Episode 1".to_owned(),
      item_type: "Episode".to_owned(),
      overview: Some("Episode overview".to_owned()),
      production_year: Some(2024),
      runtime_seconds: Some(3600.0),
      series_id: Some("series-1".to_owned()),
      series_name: Some("Series 1".to_owned()),
      season_number: Some(1),
      episode_number: Some(1),
      genres: vec!["Sci-Fi".to_owned()],
      played: false,
      favorite: false,
      played_percentage: None,
      resume_position_seconds: None,
      can_resume: false,
      can_play: true,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      metadata: Default::default(),
    };
    let neighbor_item = VideoLibraryItem {
      id: "ep-2".to_owned(),
      name: "Episode 2".to_owned(),
      item_type: "Episode".to_owned(),
      overview: None,
      production_year: Some(2024),
      runtime_seconds: Some(3600.0),
      played: false,
      favorite: false,
      artwork_image_id: None,
      series_poster_image_id: None,
      season_number: Some(1),
      episode_number: Some(2),
      series_id: Some("series-1".to_owned()),
      series_name: Some("Series 1".to_owned()),
      resume_position_seconds: None,
      played_percentage: None,
    };
    let slot_1 = state
      .kernel
      .artwork_binder
      .bind(jellypilot_core::artwork_binder::ArtworkSurface::Detail);
    let slot_2 = state
      .kernel
      .artwork_binder
      .bind(jellypilot_core::artwork_binder::ArtworkSurface::Detail);
    let slot_3 = state
      .kernel
      .artwork_binder
      .bind(jellypilot_core::artwork_binder::ArtworkSurface::Detail);
    state.detail.artwork.insert(
      DETAIL_BACKDROP_KEY.to_owned(),
      ArtworkCell {
        slot: slot_1,
        image_id: "img-backdrop".to_owned(),
        state: ArtworkCellState::Loading,
      },
    );
    state.detail.artwork.insert(
      DETAIL_POSTER_KEY.to_owned(),
      ArtworkCell {
        slot: slot_2,
        image_id: "img-poster".to_owned(),
        state: ArtworkCellState::Failed,
      },
    );
    state.detail.artwork.insert(
      "detail-episode:ep-2".to_owned(),
      ArtworkCell {
        slot: slot_3,
        image_id: "img-ep2".to_owned(),
        state: ArtworkCellState::Loading,
      },
    );
    state.detail.data.content = LoadState::Ready(DetailContent::Item(item));
    state.detail.data.season_neighbors = LoadState::Ready(vec![neighbor_item]);
    let _element = view(&state);
  }
}
