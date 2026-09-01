use crate::app::message::{HomeMessage, Message, PlaybackMessage};
use crate::app::state::{has_resume_position, ArtworkCell, ArtworkCellState, HomeSection, State};
use iced::gradient;
use iced::widget::canvas::{self, Canvas};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
  button, column, container, mouse_area, row, scrollable, space, stack, text, Column, Row, Stack,
};
use iced::{Alignment, Background, ContentFit, Degrees, Element, Fill};
use jellypilot_core::cards::{hero_headline, hero_metadata, item_caption};
use jellypilot_core::LoadState;
use jellypilot_media_server::VideoLibraryItem;
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{
  icon_for_variant, icon_for_variant_disabled, icon_with_color, Icon, IconSize,
};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::skeleton::{skeleton_block, skeleton_panel};
use jellypilot_ui::{full_radius, poster_card, rounded_image};
const THUMB_FRAME_WIDTH: f32 = 240.0;
const THUMB_FRAME_HEIGHT: f32 = 135.0;
const POSTER_FRAME_WIDTH: f32 = 160.0;
const POSTER_FRAME_HEIGHT: f32 = 240.0;
const HERO_HEIGHT: f32 = 360.0;
const HERO_POSTER_WIDTH: f32 = 160.0;
const HERO_POSTER_HEIGHT: f32 = 240.0;

/// Content width available for home content at a given window width and size class:
/// window width minus the tier-dependent sidebar width, the
/// shell hairline, and the home page horizontal padding.
pub(crate) fn content_width(window_width: f32, class: SizeClass) -> f32 {
  (window_width
    - super::shell::sidebar_width(class)
    - super::shell::HAIRLINE_WIDTH
    - TOKENS.spacing.s8 * 2.0)
    .max(1.0)
}

pub(crate) const fn section_frame_size(section: HomeSection) -> (f32, f32) {
  match section {
    HomeSection::ContinueWatching | HomeSection::NextUp => (THUMB_FRAME_WIDTH, THUMB_FRAME_HEIGHT),
    HomeSection::LatestMovies | HomeSection::LatestEpisodes => {
      (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT)
    }
  }
}

const fn section_scroll_height(section: HomeSection) -> f32 {
  match section {
    HomeSection::ContinueWatching | HomeSection::NextUp => 208.0,
    HomeSection::LatestMovies | HomeSection::LatestEpisodes => 296.0,
  }
}

pub fn view(state: &State) -> Element<'_, Message> {
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();

  let mut content = Column::new()
    .spacing(TOKENS.spacing.s8)
    .padding([TOKENS.spacing.s6, TOKENS.spacing.s8])
    .width(Fill);

  if let Some(item) = state.home.data.featured_item() {
    content = content.push(featured_hero(state, item, skeleton_phase, reduced_motion));
  } else if home_is_loading(state) {
    content = content.push(featured_skeleton(skeleton_phase, reduced_motion));
  }

  for section in HomeSection::ALL {
    if let Some(row) = section_view(state, section, skeleton_phase, reduced_motion) {
      content = content.push(row);
    }
  }

  scrollable(content)
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn home_is_loading(state: &State) -> bool {
  HomeSection::ALL
    .iter()
    .any(|section| matches!(state.home.data.section(*section), LoadState::Loading))
}

fn featured_hero<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  let poster = hero_artwork(
    state,
    state.home.artwork.hero(&item.id),
    &item.name,
    HERO_POSTER_WIDTH,
    HERO_POSTER_HEIGHT,
    skeleton_phase,
    reduced_motion,
  );
  let poster = container(poster).style(move |_| iced::widget::container::Style {
    border: iced::Border {
      radius: full_radius(TOKENS.radii.lg),
      width: 1.0,
      color: palette.colors.outlineVariant,
    },
    ..iced::widget::container::Style::default()
  });

  let play_label = if has_resume_position(item) {
    "Resume"
  } else {
    "Play"
  };
  let play_enabled = state.playback.view.engine_available;
  let play = button(
    row![
      icon_for_variant_disabled(
        Icon::Play,
        IconSize::Md,
        ButtonVariant::Primary,
        !play_enabled,
      ),
      text(play_label),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  )
  .padding([7, 14])
  .on_press_maybe(play_enabled.then(|| play_message(state, item)))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
  });
  let details = button(
    row![
      icon_for_variant(Icon::Info, IconSize::Md, ButtonVariant::Tonal),
      text("Details"),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  )
  .padding([7, 14])
  .on_press(Message::OpenDetail(item.clone()))
  .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal));
  let copy = column![
    text(hero_headline(item))
      .font(SPACE_GROTESK_FONT)
      .size(42)
      .color(palette.colors.onSurface),
    text(hero_metadata(item))
      .size(17)
      .color(palette.colors.onSurfaceVariant),
    row![play, details].spacing(TOKENS.spacing.s2),
  ]
  .spacing(TOKENS.spacing.s3)
  .width(Fill);
  let foreground = container(
    row![poster, copy]
      .spacing(TOKENS.spacing.s6)
      .align_y(Alignment::Center),
  )
  .padding(TOKENS.spacing.s6)
  .width(Fill)
  .height(HERO_HEIGHT)
  .align_y(Alignment::Center);

  let Some(backdrop) = hero_backdrop(state, item) else {
    return container(foreground)
      .width(Fill)
      .height(HERO_HEIGHT)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
      .into();
  };
  let gradient = gradient::Linear::new(Degrees(90.0))
    .add_stop(0.0, palette.colors.background.scale_alpha(0.96))
    .add_stop(1.0, palette.colors.surfaceContainerLowest.scale_alpha(0.18));
  let scrim = container(space::vertical())
    .width(Fill)
    .height(HERO_HEIGHT)
    .style(move |_| iced::widget::container::Style {
      background: Some(Background::Gradient(gradient.into())),
      border: iced::Border {
        radius: full_radius(TOKENS.radii.lg),
        ..iced::Border::default()
      },
      ..iced::widget::container::Style::default()
    });

  container(stack![backdrop, scrim, foreground])
    .width(Fill)
    .height(HERO_HEIGHT)
    .clip(true)
    .style(|_| iced::widget::container::Style {
      border: iced::Border {
        radius: full_radius(TOKENS.radii.lg),
        ..iced::Border::default()
      },
      ..iced::widget::container::Style::default()
    })
    .into()
}

fn featured_skeleton<'a>(phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  let backdrop = skeleton_block(Fill, HERO_HEIGHT, phase, reduced_motion);
  let poster = skeleton_block(HERO_POSTER_WIDTH, HERO_POSTER_HEIGHT, phase, reduced_motion);
  let copy = column![
    skeleton_block(360.0, 44.0, phase, reduced_motion),
    skeleton_block(280.0, 20.0, phase, reduced_motion),
    row![
      skeleton_block(112.0, 38.0, phase, reduced_motion),
      skeleton_block(112.0, 38.0, phase, reduced_motion),
    ]
    .spacing(TOKENS.spacing.s2),
  ]
  .spacing(TOKENS.spacing.s3);
  let foreground = container(
    row![poster, copy]
      .spacing(TOKENS.spacing.s6)
      .align_y(Alignment::Center),
  )
  .padding(TOKENS.spacing.s6)
  .width(Fill)
  .height(HERO_HEIGHT)
  .align_y(Alignment::Center);

  stack![backdrop, foreground]
    .width(Fill)
    .height(HERO_HEIGHT)
    .into()
}

fn section_view(
  state: &State,
  section: HomeSection,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Option<Element<'_, Message>> {
  match state.home.data.section(section) {
    LoadState::Idle => None,
    LoadState::Loading => Some(section_skeleton(
      state.palette(),
      section,
      skeleton_phase,
      reduced_motion,
    )),
    LoadState::Failed(error) => Some(section_error(state.palette(), section.title(), error)),
    LoadState::Ready(items)
      if items.iter().all(|item| {
        state
          .home
          .data
          .featured_item()
          .is_some_and(|featured| featured.id == item.id)
      }) =>
    {
      None
    }
    LoadState::Ready(items) => Some(section_row(
      state,
      section,
      items,
      skeleton_phase,
      reduced_motion,
    )),
  }
}

fn section_row<'a>(
  state: &'a State,
  section: HomeSection,
  items: &'a [VideoLibraryItem],
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let mut cards = Row::new()
    .spacing(TOKENS.spacing.s4)
    .align_y(Alignment::Start);
  let featured_item_id = state.home.data.featured_item().map(|item| item.id.as_str());
  for item in items
    .iter()
    .filter(|item| Some(item.id.as_str()) != featured_item_id)
  {
    cards = cards.push(video_card(
      state,
      section,
      item,
      skeleton_phase,
      reduced_motion,
    ));
  }
  let cards = scrollable(cards)
    .direction(Direction::Horizontal(Scrollbar::new()))
    .height(section_scroll_height(section))
    .style(jellypilot_ui::theme::scrollable);

  column![
    text(section.title())
      .font(SPACE_GROTESK_FONT)
      .size(24)
      .color(state.palette().colors.onSurface),
    cards,
  ]
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn video_card<'a>(
  state: &'a State,
  section: HomeSection,
  item: &'a VideoLibraryItem,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let (frame_width, frame_height) = section_frame_size(section);
  let palette = state.palette();
  let is_action_card = matches!(section, HomeSection::ContinueWatching | HomeSection::NextUp);
  let radius = full_radius(TOKENS.radii.lg);
  let poster = card_artwork(
    state,
    state.home.artwork.card(section, &item.id),
    &item.name,
    (frame_width, frame_height),
    radius,
    skeleton_phase,
    reduced_motion,
  );

  let text_stack = column![
    ellipsis_text(&item.name)
      .size(14)
      .color(palette.colors.onSurface),
    ellipsis_text(item_caption(item))
      .size(12)
      .color(palette.colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);

  if is_action_card {
    let play_enabled = state.playback.view.engine_available;
    let playable_artwork = button(poster)
      .padding(0)
      .width(frame_width)
      .height(frame_height)
      .on_press_maybe(play_enabled.then(|| play_message(state, item)))
      .style(|_, _| iced::widget::button::Style::default());
    let mut artwork_layers = Stack::new()
      .width(frame_width)
      .height(frame_height)
      .push(playable_artwork);
    if let Some(progress) = card_progress(section, item) {
      artwork_layers = artwork_layers.push(
        container(progress_bar(palette, progress, radius))
          .width(Fill)
          .height(Fill)
          .align_y(Alignment::End),
      );
    }
    if state.home.data.hovered_card.as_deref() == Some(item.id.as_str()) {
      let details = button(icon_for_variant(
        Icon::Info,
        IconSize::Xs,
        ButtonVariant::Tonal,
      ))
      .padding(7)
      .on_press(Message::OpenDetail(item.clone()))
      .style(|theme, status| {
        jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal)
      });
      artwork_layers = artwork_layers.push(
        container(details)
          .padding(TOKENS.spacing.s2)
          .width(Fill)
          .height(Fill)
          .align_x(Alignment::End)
          .align_y(Alignment::Start),
      );
    }
    let artwork = container(
      mouse_area(artwork_layers)
        .on_enter(Message::Home(HomeMessage::CardHoverEnter(item.id.clone())))
        .on_exit(Message::Home(HomeMessage::CardHoverExit(item.id.clone()))),
    )
    .width(frame_width)
    .height(frame_height)
    .clip(true)
    .style(move |_| iced::widget::container::Style {
      border: iced::Border {
        radius,
        ..iced::Border::default()
      },
      ..iced::widget::container::Style::default()
    });
    let copy = container(text_stack)
      .padding(iced::Padding {
        top: TOKENS.spacing.s3,
        right: TOKENS.spacing.s4,
        bottom: TOKENS.spacing.s4,
        left: TOKENS.spacing.s4,
      })
      .width(Fill);

    return container(column![artwork, copy].width(Fill))
      .width(frame_width)
      .clip(true)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
      .into();
  }

  let copy = column![
    ellipsis_text(&item.name)
      .size(14)
      .color(palette.colors.onSurface),
    ellipsis_text(item_caption(item))
      .size(12)
      .color(palette.colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s1)
  .padding(iced::Padding {
    top: TOKENS.spacing.s2,
    right: 0.0,
    bottom: 0.0,
    left: 0.0,
  })
  .width(frame_width);

  poster_card(poster, copy)
    .width(frame_width)
    .on_press(Message::OpenDetail(item.clone()))
    .into()
}

fn play_message(state: &State, item: &VideoLibraryItem) -> Message {
  Message::Playback(PlaybackMessage::Intent(PlaybackIntent::Start {
    item: Playable::Library(item.clone()),
    position: if has_resume_position(item) {
      PlaybackStartPosition::Resume
    } else {
      PlaybackStartPosition::Beginning
    },
    intro: state.kernel.intro_availability(),
    selection: Box::default(),
  }))
}
fn hero_backdrop<'a>(state: &'a State, item: &VideoLibraryItem) -> Option<Element<'a, Message>> {
  let cell = state.home.artwork.hero_backdrop(&item.id)?;
  if cell.state != ArtworkCellState::Ready {
    return None;
  }
  let handle = state
    .kernel
    .artwork_handles
    .get(cell.slot, &cell.image_id)?;
  Some(
    // Inset the backdrop by 1px with a matching smaller radius so its rounded
    // edge sits strictly inside the scrim's. iced's image and quad shaders
    // evaluate corner SDFs differently, and coincident arcs let photo pixels
    // leak past the scrim as a dark trace on the page side.
    container(
      rounded_image(handle.clone(), full_radius(TOKENS.radii.lg - 1.0))
        .content_fit(ContentFit::Cover)
        .width(Fill)
        .height(Fill),
    )
    .padding(1.0)
    .width(Fill)
    .height(HERO_HEIGHT)
    .into(),
  )
}

fn hero_artwork<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  name: &'a str,
  width: f32,
  height: f32,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  if let Some(cell) = cell {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.kernel.artwork_handles.get(cell.slot, &cell.image_id) {
        return rounded_image(handle.clone(), full_radius(TOKENS.radii.lg))
          .content_fit(ContentFit::Cover)
          .width(width)
          .height(height)
          .into();
      }
    }
  }

  let failed = cell.is_some_and(|cell| cell.state == ArtworkCellState::Failed);
  if failed {
    let placeholder_color = palette.colors.warning;
    let initial = name
      .trim()
      .chars()
      .next()
      .map(|character| character.to_uppercase().collect::<String>())
      .unwrap_or_else(|| "•".to_owned());
    return container(
      column![
        icon_with_color(Icon::Movie, 42.0, placeholder_color),
        text(initial)
          .font(SPACE_GROTESK_FONT)
          .size(32)
          .color(placeholder_color),
      ]
      .spacing(TOKENS.spacing.s1)
      .align_x(Alignment::Center),
    )
    .width(width)
    .height(height)
    .center_x(Fill)
    .center_y(Fill)
    .style(|_theme| container::Style {
      background: Some(iced::Background::Color(
        palette.colors.surfaceContainerLowest,
      )),
      border: iced::Border {
        radius: full_radius(TOKENS.radii.lg),
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
    palette.colors.surfaceContainerLowest,
    full_radius(TOKENS.radii.lg),
    phase,
    reduced_motion,
  )
  .into()
}

fn card_artwork<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  name: &'a str,
  (width, height): (f32, f32),
  radius: iced::border::Radius,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  if let Some(cell) = cell {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.kernel.artwork_handles.get(cell.slot, &cell.image_id) {
        return rounded_image(handle.clone(), radius)
          .content_fit(ContentFit::Cover)
          .width(width)
          .height(height)
          .into();
      }
    }
  }

  let failed = cell.is_some_and(|cell| cell.state == ArtworkCellState::Failed);
  if failed {
    let placeholder_color = palette.colors.warning;
    let initial = name
      .trim()
      .chars()
      .next()
      .map(|character| character.to_uppercase().collect::<String>())
      .unwrap_or_else(|| "•".to_owned());
    let icon_dim = if width > POSTER_FRAME_WIDTH {
      42.0
    } else {
      32.0
    };
    return container(
      column![
        icon_with_color(Icon::Movie, icon_dim, placeholder_color),
        text(initial)
          .font(SPACE_GROTESK_FONT)
          .size(if width > POSTER_FRAME_WIDTH { 32 } else { 24 })
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
        palette.colors.surfaceContainerLowest,
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
    palette.colors.surfaceContainerLowest,
    radius,
    phase,
    reduced_motion,
  )
  .into()
}
fn card_progress(section: HomeSection, item: &VideoLibraryItem) -> Option<f64> {
  if section != HomeSection::ContinueWatching
    && (section != HomeSection::NextUp || !has_resume_position(item))
  {
    return None;
  }
  if let Some(percentage) = item.played_percentage.filter(|value| value.is_finite()) {
    return Some(percentage.clamp(0.0, 100.0));
  }
  match (item.resume_position_seconds, item.runtime_seconds) {
    (Some(position), Some(runtime))
      if position.is_finite() && position >= 0.0 && runtime.is_finite() && runtime > 0.0 =>
    {
      Some((position / runtime * 100.0).clamp(0.0, 100.0))
    }
    _ => None,
  }
}

const PROGRESS_BAR_HEIGHT: f32 = 8.0;

fn progress_bar<'a>(
  palette: &'static ThemePalette,
  progress: f64,
  radius: iced::border::Radius,
) -> Element<'a, Message> {
  Canvas::new(ProgressOverlay {
    progress: (progress / 100.0).clamp(0.0, 1.0) as f32,
    fill: palette.colors.primary,
    // Translucent track: the unfilled portion reads as a scrim over the
    // artwork instead of an opaque strip (opaque fill stays fully covered).
    track: palette.colors.surfaceContainerLow.scale_alpha(0.5),
    radius,
  })
  .width(Fill)
  .height(PROGRESS_BAR_HEIGHT)
  .into()
}

/// Bottom-edge progress overlay. A bar-height rectangle cannot carry the
/// artwork's corner radius (border radii clamp to half the bar height), so
/// this draws the artwork's full rounded-rect contour tall enough to escape
/// clamping and lets the frame bounds crop everything above the strip: the
/// exposed corners reproduce the artwork's exact arc. The fill is a dedicated
/// path with its own bottom corner radius — `Frame::with_clip` is unreliable
/// across iced 0.14 backends (draft/paste drops or offsets clips), so no clip
/// regions are used at all.
struct ProgressOverlay {
  progress: f32,
  fill: iced::Color,
  track: iced::Color,
  radius: iced::border::Radius,
}

impl canvas::Program<Message> for ProgressOverlay {
  type State = ();

  fn draw(
    &self,
    _state: &Self::State,
    renderer: &iced::Renderer,
    _theme: &iced::Theme,
    bounds: iced::Rectangle,
    _cursor: iced::mouse::Cursor,
  ) -> Vec<canvas::Geometry> {
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let corner = self.radius.bottom_left.max(self.radius.bottom_right);
    let top = PROGRESS_BAR_HEIGHT - 2.0 * corner;
    if corner > 0.0 {
      frame.fill(
        &canvas::Path::rounded_rectangle(
          iced::Point::new(0.0, top),
          iced::Size::new(bounds.width, 2.0 * corner),
          self.radius,
        ),
        self.track,
      );
      if self.progress > 0.0 {
        let fill_radius = iced::border::Radius {
          top_left: 0.0,
          top_right: 0.0,
          bottom_left: self.radius.bottom_left,
          bottom_right: if self.progress >= 1.0 {
            self.radius.bottom_right
          } else {
            0.0
          },
        };
        frame.fill(
          &canvas::Path::rounded_rectangle(
            iced::Point::new(0.0, top),
            iced::Size::new(bounds.width * self.progress, 2.0 * corner),
            fill_radius,
          ),
          self.fill,
        );
      }
    } else {
      frame.fill_rectangle(
        iced::Point::ORIGIN,
        iced::Size::new(bounds.width, PROGRESS_BAR_HEIGHT),
        self.track,
      );
      if self.progress > 0.0 {
        frame.fill_rectangle(
          iced::Point::ORIGIN,
          iced::Size::new(bounds.width * self.progress, PROGRESS_BAR_HEIGHT),
          self.fill,
        );
      }
    }
    vec![frame.into_geometry()]
  }
}

fn section_skeleton<'a>(
  palette: &ThemePalette,
  section: HomeSection,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let (width, height) = section_frame_size(section);
  let mut cards = Row::new().spacing(TOKENS.spacing.s4);
  for _ in 0..5 {
    cards = cards.push(
      column![
        skeleton_block(width, height, phase, reduced_motion),
        skeleton_block(width, 18.0, phase, reduced_motion),
        skeleton_block(width * 0.6, 14.0, phase, reduced_motion),
      ]
      .spacing(TOKENS.spacing.s2),
    );
  }
  column![
    text(section.title())
      .font(SPACE_GROTESK_FONT)
      .size(24)
      .color(palette.colors.onSurface),
    cards,
  ]
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn section_error<'a>(
  palette: &ThemePalette,
  title: &'static str,
  error: &'a str,
) -> Element<'a, Message> {
  let retry = button(text("Retry"))
    .padding([6, 12])
    .on_press(Message::Home(HomeMessage::Retry))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal)
    });
  container(
    column![
      text(title)
        .font(SPACE_GROTESK_FONT)
        .size(24)
        .color(palette.colors.onSurface),
      text(error).size(13).color(palette.colors.error),
      retry,
    ]
    .spacing(TOKENS.spacing.s3),
  )
  .padding(TOKENS.spacing.s4)
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
  .into()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn section_frame_sizes_and_row_heights_match_aspect_ratios() {
    let (cw_w, cw_h) = section_frame_size(HomeSection::ContinueWatching);
    assert_eq!((cw_w, cw_h), (THUMB_FRAME_WIDTH, THUMB_FRAME_HEIGHT));
    assert_eq!(section_scroll_height(HomeSection::ContinueWatching), 208.0);

    let (mov_w, mov_h) = section_frame_size(HomeSection::LatestMovies);
    assert_eq!((mov_w, mov_h), (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT));
    assert_eq!(section_scroll_height(HomeSection::LatestMovies), 296.0);
  }

  #[test]
  fn content_width_standard_matches_pinned_regression_constant() {
    let expected = 1600.0 - 248.0 - super::super::shell::HAIRLINE_WIDTH - TOKENS.spacing.s8 * 2.0;
    assert_eq!(content_width(1600.0, SizeClass::Standard), expected);
    assert_eq!(content_width(1600.0, SizeClass::Standard), 1287.0);
  }

  #[test]
  fn content_width_compact_uses_rail_sidebar() {
    let expected = 1024.0 - 72.0 - super::super::shell::HAIRLINE_WIDTH - TOKENS.spacing.s8 * 2.0;
    assert_eq!(content_width(1024.0, SizeClass::Compact), expected);
    assert_eq!(content_width(1024.0, SizeClass::Compact), 887.0);
  }

  #[test]
  fn content_width_clamps_to_floor_at_narrow_widths() {
    assert_eq!(content_width(0.0, SizeClass::Compact), 1.0);
    assert_eq!(content_width(50.0, SizeClass::Compact), 1.0);
    assert_eq!(content_width(-100.0, SizeClass::Compact), 1.0);
  }

  #[test]
  fn home_view_renders_hero_and_cards_with_loading_and_failed_artwork() {
    let mut state = State::boot(false);
    state.shell.skeleton_phase = 0.5;
    let hero_item = VideoLibraryItem {
      id: "hero-1".to_owned(),
      name: "Hero Movie".to_owned(),
      item_type: "Movie".to_owned(),
      production_year: Some(2024),
      runtime_seconds: Some(7200.0),
      played: false,
      favorite: true,
      artwork_image_id: None,
      backdrop_image_id: Some("img-hero-backdrop".to_owned()),
      series_poster_image_id: None,
      season_number: None,
      episode_number: None,
      series_id: None,
      series_name: None,
      resume_position_seconds: None,
      played_percentage: None,
      overview: Some("Hero overview text".to_owned()),
    };
    let card_item = VideoLibraryItem {
      id: "card-1".to_owned(),
      name: "Card Movie".to_owned(),
      item_type: "Movie".to_owned(),
      production_year: Some(2023),
      runtime_seconds: Some(5400.0),
      played: false,
      favorite: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      season_number: None,
      episode_number: None,
      series_id: None,
      series_name: None,
      resume_position_seconds: Some(2430.0),
      played_percentage: Some(45.0),
      overview: None,
    };
    state
      .home
      .data
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![card_item],
        latest_movies: vec![hero_item],
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    state.home.data.settle_shortcuts(Ok(vec![]));
    let slot_1 = state
      .kernel
      .artwork_binder
      .bind(jellypilot_core::artwork_binder::ArtworkSurface::Home);
    let slot_2 = state
      .kernel
      .artwork_binder
      .bind(jellypilot_core::artwork_binder::ArtworkSurface::Home);
    state.home.artwork.insert_hero_backdrop(
      "hero-1".to_owned(),
      ArtworkCell {
        slot: slot_1,
        image_id: "img-hero-backdrop".to_owned(),
        state: ArtworkCellState::Loading,
      },
    );
    state.home.artwork.insert_card(
      HomeSection::ContinueWatching,
      "card-1".to_owned(),
      ArtworkCell {
        slot: slot_2,
        image_id: "img-card".to_owned(),
        state: ArtworkCellState::Failed,
      },
    );
    let _element = view(&state);
  }
}
