use crate::app::message::{HomeMessage, Message, PlaybackMessage};
use crate::app::state::{
  has_resume_position, ArtworkCell, ArtworkCellState, HomeRow, HomeSection, State,
};
use iced::gradient;
use iced::widget::canvas::{self, Canvas};
use iced::widget::image::Image;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
  button, column, container, mouse_area, responsive, row, scrollable, space, stack, text, Column,
  Row, Stack,
};
use iced::{Alignment, Background, ContentFit, Degrees, Element, Fill, Length};
use jellypilot_core::cards::{
  card_subtitle, card_title, hero_headline, hero_metadata, logo_display_size,
};
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
const POSTER_FRAME_WIDTH: f32 = 168.0;
const POSTER_FRAME_HEIGHT: f32 = 240.0;
/// Jellyfin hero backdrops are 16:9; derive the hero height from its width so
/// the Backdrop renders uncropped.
fn hero_height_for_width(width: f32) -> f32 {
  (width * 9.0 / 16.0).max(1.0)
}
const HERO_LOGO_HEIGHT: f32 = 96.0;

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
  if section.is_action() {
    (THUMB_FRAME_WIDTH, THUMB_FRAME_HEIGHT)
  } else {
    (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT)
  }
}

const fn section_scroll_height(section: HomeSection) -> f32 {
  if section.is_action() {
    208.0
  } else {
    296.0
  }
}

pub fn view(state: &State) -> Element<'_, Message> {
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();

  let mut content = Column::new()
    .spacing(TOKENS.spacing.s8)
    .padding([TOKENS.spacing.s6, TOKENS.spacing.s8])
    .width(Fill);

  let featured_item = state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .featured_item();
  if featured_item.is_some() || home_is_loading(state) {
    content = content.push(responsive(move |bounds| {
      if let Some(item) = featured_item {
        featured_hero(state, item, bounds.width)
      } else {
        featured_skeleton(skeleton_phase, reduced_motion, bounds.width)
      }
    }));
  }

  for row in state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .rows()
  {
    if let Some(section) = section_view(state, row, skeleton_phase, reduced_motion) {
      content = content.push(section);
    }
  }

  scrollable(content)
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn home_is_loading(state: &State) -> bool {
  state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .rows()
    .iter()
    .any(|row| matches!(row.items, LoadState::Loading))
}

fn featured_hero<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  width: f32,
) -> Element<'a, Message> {
  let palette = state.palette();
  let hero_height = hero_height_for_width(width);
  let headline = hero_artwork(
    state,
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .home
      .artwork
      .hero(&item.id),
    item,
  );
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
    headline,
    text(hero_metadata(item))
      .size(17)
      .color(palette.colors.onSurfaceVariant),
    row![play, details].spacing(TOKENS.spacing.s2),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_x(Alignment::Start)
  .width(Fill);
  let foreground = container(copy)
    .padding(TOKENS.spacing.s6)
    .width(Fill)
    .height(hero_height)
    .align_y(Alignment::End);

  let Some(backdrop) = hero_backdrop(state, item, hero_height) else {
    return container(foreground)
      .width(Fill)
      .height(hero_height)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
      .into();
  };
  let gradient = gradient::Linear::new(Degrees(180.0))
    .add_stop(0.0, palette.colors.surfaceContainerLowest.scale_alpha(0.4))
    .add_stop(1.0, palette.colors.surfaceContainerLowest.scale_alpha(0.95));
  let scrim = container(space::vertical())
    .width(Fill)
    .height(hero_height)
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
    .height(hero_height)
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

fn featured_skeleton<'a>(phase: f32, reduced_motion: bool, width: f32) -> Element<'a, Message> {
  let hero_height = hero_height_for_width(width);
  let backdrop = skeleton_block(Fill, hero_height, phase, reduced_motion);
  let copy = column![
    skeleton_block(360.0, HERO_LOGO_HEIGHT, phase, reduced_motion),
    skeleton_block(280.0, 20.0, phase, reduced_motion),
    row![
      skeleton_block(112.0, 38.0, phase, reduced_motion),
      skeleton_block(112.0, 38.0, phase, reduced_motion),
    ]
    .spacing(TOKENS.spacing.s2),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_x(Alignment::Start);
  let foreground = container(copy)
    .padding(TOKENS.spacing.s6)
    .width(Fill)
    .height(hero_height)
    .align_y(Alignment::Center);

  stack![backdrop, foreground]
    .width(Fill)
    .height(hero_height)
    .into()
}

fn section_view<'a>(
  state: &'a State,
  row: &'a HomeRow,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Option<Element<'a, Message>> {
  match &row.items {
    LoadState::Idle => None,
    LoadState::Loading => Some(section_skeleton(
      state.palette(),
      row,
      skeleton_phase,
      reduced_motion,
    )),
    LoadState::Failed(error) => Some(section_error(state.palette(), &row.title, error)),
    LoadState::Ready(items)
      if items.iter().all(|item| {
        state
          .full
          .as_ref()
          .expect("FullUi required")
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
      row,
      items,
      skeleton_phase,
      reduced_motion,
    )),
  }
}

fn section_row<'a>(
  state: &'a State,
  home_row: &'a HomeRow,
  items: &'a [VideoLibraryItem],
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let mut cards = Row::new()
    .spacing(TOKENS.spacing.s4)
    .align_y(Alignment::Start);
  let featured_item_id = state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .featured_item()
    .map(|item| item.id.as_str());
  for item in items
    .iter()
    .filter(|item| Some(item.id.as_str()) != featured_item_id)
  {
    cards = cards.push(video_card(
      state,
      home_row.section,
      item,
      skeleton_phase,
      reduced_motion,
    ));
  }
  let cards = scrollable(cards)
    .direction(Direction::Horizontal(Scrollbar::new()))
    .height(section_scroll_height(home_row.section))
    .style(jellypilot_ui::theme::scrollable);

  column![
    text(&home_row.title)
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
  let is_action_card = section.is_action();
  let radius = full_radius(TOKENS.radii.lg);
  let poster = card_artwork(
    state,
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .home
      .artwork
      .card(section, &item.id),
    card_title(item),
    (frame_width, frame_height),
    radius,
    skeleton_phase,
    reduced_motion,
  );

  let text_stack = column![
    ellipsis_text(card_title(item))
      .size(14)
      .color(palette.colors.onSurface),
    ellipsis_text(card_subtitle(item))
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
      let frosted_strip = state
        .full
        .as_ref()
        .expect("FullUi required")
        .home
        .artwork
        .card(section, &item.id)
        .and_then(|cell| {
          state
            .kernel
            .artwork_handles
            .frosted_strip(cell.slot, &cell.image_id)
        })
        .cloned();
      let frosted = frosted_strip.is_some();
      if let Some(strip) = frosted_strip {
        artwork_layers = artwork_layers.push(
          container(
            Image::new(strip)
              .width(Fill)
              .height(PROGRESS_BAR_HEIGHT)
              .content_fit(ContentFit::Fill),
          )
          .width(Fill)
          .height(Fill)
          .align_y(Alignment::End),
        );
      }
      artwork_layers = artwork_layers.push(
        container(progress_bar(palette, progress, radius, frosted))
          .width(Fill)
          .height(Fill)
          .align_y(Alignment::End),
      );
    }
    if state
      .full
      .as_ref()
      .expect("FullUi required")
      .home
      .data
      .hovered_card
      .as_deref()
      == Some(item.id.as_str())
    {
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
        right: 0.0,
        bottom: TOKENS.spacing.s4,
        left: 0.0,
      })
      .width(Fill);

    return container(column![artwork, copy].width(Fill))
      .width(frame_width)
      .clip(true)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
      .into();
  }

  let copy = column![
    ellipsis_text(card_title(item))
      .size(14)
      .color(palette.colors.onSurface),
    ellipsis_text(card_subtitle(item))
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
  let mut artwork_layers = Stack::new()
    .width(frame_width)
    .height(frame_height)
    .push(poster);
  if let Some(badge) = count_badge(palette, section, item) {
    artwork_layers = artwork_layers.push(
      container(badge)
        .padding(TOKENS.spacing.s2)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::Start),
    );
  }

  poster_card(artwork_layers, copy)
    .width(frame_width)
    .on_press(Message::OpenDetail(item.clone()))
    .into()
}

fn unplayed_badge_text(section: HomeSection, item: &VideoLibraryItem) -> Option<String> {
  if !section.is_latest() || !item.item_type.eq_ignore_ascii_case("Series") {
    return None;
  }
  match item.unplayed_item_count {
    Some(count @ 1..100) => Some(count.to_string()),
    Some(100..) => Some("99+".to_owned()),
    None | Some(0) => None,
  }
}

fn count_badge<'a>(
  palette: &'static ThemePalette,
  section: HomeSection,
  item: &VideoLibraryItem,
) -> Option<Element<'a, Message>> {
  let label = unplayed_badge_text(section, item)?;
  Some(
    container(text(label).size(12).color(palette.colors.onPrimary))
      .padding([3, 7])
      .style(move |_| container::Style {
        background: Some(Background::Color(palette.colors.primary)),
        text_color: Some(palette.colors.onPrimary),
        border: iced::Border {
          radius: full_radius(TOKENS.radii.md),
          ..iced::Border::default()
        },
        ..container::Style::default()
      })
      .into(),
  )
}

fn play_message(state: &State, item: &VideoLibraryItem) -> Message {
  Message::Playback(PlaybackMessage::Intent(Box::new(PlaybackIntent::Start {
    item: Playable::Library(item.clone()),
    position: if has_resume_position(item) {
      PlaybackStartPosition::Resume
    } else {
      PlaybackStartPosition::Beginning
    },
    intro: state.kernel.intro_availability(),
    selection: Box::default(),
  })))
}
fn hero_backdrop<'a>(
  state: &'a State,
  item: &VideoLibraryItem,
  height: f32,
) -> Option<Element<'a, Message>> {
  let cell = state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .artwork
    .hero_backdrop(&item.id)?;
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
    .height(height)
    .into(),
  )
}

fn hero_artwork<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  item: &'a VideoLibraryItem,
) -> Element<'a, Message> {
  if let Some(cell) = cell {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.kernel.artwork_handles.get(cell.slot, &cell.image_id) {
        let dims = state
          .kernel
          .artwork_handles
          .dims(cell.slot, &cell.image_id)
          .filter(|&(w, h)| w > 0 && h > 0);
        let (logo_width, logo_height) = dims
          .map(|(w, h)| logo_display_size(w, h, HERO_LOGO_HEIGHT))
          .unwrap_or((0.0, HERO_LOGO_HEIGHT));
        let logo_image = Image::new(handle.clone())
          .content_fit(ContentFit::Contain)
          .expand(logo_width <= 0.0)
          .height(logo_height)
          .width(if logo_width > 0.0 {
            Length::Fixed(logo_width)
          } else {
            Length::Shrink
          });
        // The baked shadow canvas carries a transparent margin (height/4 on
        // top/bottom/right, a constant 3/2 render ratio); indent the logo on
        // top only so the glyph overlaps its shadow while the left edge stays
        // flush with the text below.
        let logo = container(logo_image).padding(iced::Padding {
          top: logo_height / 4.0,
          ..iced::Padding::ZERO
        });
        let Some(shadow) = state
          .kernel
          .artwork_handles
          .logo_shadow(cell.slot, &cell.image_id)
        else {
          return logo.into();
        };
        let shadow_height = logo_height * 3.0 / 2.0;
        let shadow_width =
          dims.map(|(w, h)| (w as f32 + h as f32 / 2.0) * (logo_height / h as f32));
        let shadow_image = Image::new(shadow.clone())
          .content_fit(ContentFit::Contain)
          .expand(shadow_width.is_none())
          .height(shadow_height)
          .width(if let Some(width) = shadow_width {
            Length::Fixed(width)
          } else {
            Length::Shrink
          });
        return stack![container(shadow_image), logo].into();
      }
    }
  }

  text(hero_headline(item))
    .font(SPACE_GROTESK_FONT)
    .size(42)
    .color(state.palette().colors.onSurface)
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

pub(crate) const PROGRESS_BAR_HEIGHT: f32 = 8.0;

fn progress_bar<'a>(
  palette: &'static ThemePalette,
  progress: f64,
  radius: iced::border::Radius,
  frosted: bool,
) -> Element<'a, Message> {
  Canvas::new(ProgressOverlay {
    progress: (progress / 100.0).clamp(0.0, 1.0) as f32,
    // The fill stays translucent as well, so the frosted strip shows through
    // while the watched portion remains clearly distinguished by hue.
    fill: palette.colors.primary.scale_alpha(0.8),
    track: if frosted {
      palette.colors.surfaceContainerLowest.scale_alpha(0.4)
    } else {
      palette.colors.surfaceContainerLow.scale_alpha(0.5)
    },
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
  row: &'a HomeRow,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let (width, height) = section_frame_size(row.section);
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
    text(&row.title)
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
  title: &'a str,
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

    let (mov_w, mov_h) = section_frame_size(HomeSection::Latest(0));
    assert_eq!((mov_w, mov_h), (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT));
    assert_eq!(section_scroll_height(HomeSection::Latest(0)), 296.0);
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
      logo_image_id: None,
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
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_number: None,
      episode_number: None,
      series_id: None,
      series_name: None,
      resume_position_seconds: None,
      played_percentage: None,
      overview: Some("Hero overview text".to_owned()),
      index_number_end: None,
      season_poster_image_id: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
    };
    let card_item = VideoLibraryItem {
      logo_image_id: None,
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
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_number: None,
      episode_number: None,
      series_id: None,
      series_name: None,
      resume_position_seconds: Some(2430.0),
      played_percentage: Some(45.0),
      overview: None,
      index_number_end: None,
      season_poster_image_id: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
    };
    state.full.as_mut().unwrap().home.data.settle_video_home(Ok(
      jellypilot_media_server::VideoHome {
        continue_watching: vec![card_item],
        next_up: vec![hero_item],
      },
    ));
    state
      .full
      .as_mut()
      .unwrap()
      .home
      .data
      .settle_shortcuts(Ok(vec![]));
    let slot_1 = state
      .kernel
      .artwork_binder
      .bind(jellypilot_core::artwork_binder::ArtworkSurface::Home);
    let slot_2 = state
      .kernel
      .artwork_binder
      .bind(jellypilot_core::artwork_binder::ArtworkSurface::Home);
    state
      .full
      .as_mut()
      .unwrap()
      .home
      .artwork
      .insert_hero_backdrop(
        "hero-1".to_owned(),
        ArtworkCell {
          slot: slot_1,
          image_id: "img-hero-backdrop".to_owned(),
          state: ArtworkCellState::Loading,
        },
      );
    state.full.as_mut().unwrap().home.artwork.insert_card(
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

  #[test]
  fn unplayed_badge_only_formats_nonzero_latest_series_counts() {
    let mut item = VideoLibraryItem {
      logo_image_id: None,
      id: "series-1".to_owned(),
      name: "Series".to_owned(),
      item_type: "Series".to_owned(),
      production_year: Some(2020),
      runtime_seconds: None,
      played: false,
      favorite: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_poster_image_id: None,
      season_number: None,
      episode_number: None,
      index_number_end: None,
      series_id: None,
      series_name: None,
      end_year: None,
      series_continuing: true,
      unplayed_item_count: Some(7),
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
    };

    assert_eq!(
      unplayed_badge_text(HomeSection::Latest(0), &item).as_deref(),
      Some("7")
    );
    item.unplayed_item_count = Some(100);
    assert_eq!(
      unplayed_badge_text(HomeSection::Latest(0), &item).as_deref(),
      Some("99+")
    );
    item.unplayed_item_count = Some(0);
    assert!(unplayed_badge_text(HomeSection::Latest(0), &item).is_none());
    item.unplayed_item_count = Some(5);
    assert!(unplayed_badge_text(HomeSection::ContinueWatching, &item).is_none());
    item.item_type = "Movie".to_owned();
    assert!(unplayed_badge_text(HomeSection::Latest(0), &item).is_none());
  }
}
