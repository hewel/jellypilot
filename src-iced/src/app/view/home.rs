use crate::app::message::{HomeMessage, Message, PlaybackMessage};
use crate::app::state::{ArtworkCell, ArtworkCellState, HomeRow, HomeSection, State};
use iced::advanced::widget;
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
  card_subtitle, card_title, hero_headline, hero_metadata, is_episode_item, logo_display_size,
  runtime_caption,
};
use jellypilot_core::home_hero::has_resume_position;
use jellypilot_core::LoadState;
use jellypilot_media_server::VideoLibraryItem;
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::overlay::{focus_tooltip, TooltipOptions};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::skeleton::{skeleton_block, skeleton_panel};
use jellypilot_ui::{full_radius, poster_card, rounded_image};
const THUMB_FRAME_WIDTH: f32 = 240.0;
const THUMB_FRAME_HEIGHT: f32 = 135.0;
const POSTER_FRAME_WIDTH: f32 = 168.0;
const POSTER_FRAME_HEIGHT: f32 = 240.0;
/// Budget against the actual Home viewport, after all docked player controls.
const HERO_HEIGHT_RATIO: f32 = 0.62;
const HERO_MAX_HEIGHT: f32 = 520.0;
const SECTION_TITLE_SIZE: f32 = 24.0;
const HERO_LOGO_MAX_HEIGHT: f32 = 96.0;
/// Hero Selection Rail geometry: compact 16:9 stills, deliberately
/// subordinate to the 240x135 direct-resume cards.
const RAIL_IMAGE_WIDTH: f32 = 80.0;
const RAIL_IMAGE_HEIGHT: f32 = 45.0;
const RAIL_CARD_WIDTH: f32 = RAIL_IMAGE_WIDTH + TOKENS.spacing.s0_5 * 2.0 + 4.0;
const RAIL_ROW_HEIGHT: f32 = 68.0;
const HERO_MIN_TEXT_ZONE: f32 = 240.0;

fn hero_height(viewport_height: f32, has_continue_watching: bool) -> f32 {
  let preferred = (viewport_height * HERO_HEIGHT_RATIO).min(HERO_MAX_HEIGHT);
  if !has_continue_watching {
    return preferred;
  }
  let continuation = section_scroll_height(HomeSection::ContinueWatching)
    + SECTION_TITLE_SIZE * 1.3
    + TOKENS.spacing.s3;
  let page_spacing = TOKENS.spacing.s4 + TOKENS.spacing.s2;
  preferred.min((viewport_height - continuation - page_spacing).max(0.0))
}

/// The Title Logo scales with the hero so short windows keep room for the
/// selection rail and the Continue Watching row.
fn hero_logo_height(hero_height: f32) -> f32 {
  (hero_height * 0.2).clamp(40.0, HERO_LOGO_MAX_HEIGHT)
}

fn hero_rail_scroll_id() -> iced::widget::Id {
  iced::widget::Id::new("home-hero-rail")
}

/// Center the selected card using the rail's measured viewport, not an
/// estimate of button widths. Only the rail scrolls; focus stays untouched.
pub(crate) fn reveal_hero_selection(index: usize) -> iced::Task<Message> {
  struct Reveal {
    index: usize,
    id: widget::Id,
  }
  impl<T> widget::Operation<T> for Reveal {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation<T>)) {
      visit(self);
    }

    fn scrollable(
      &mut self,
      id: Option<&widget::Id>,
      bounds: iced::Rectangle,
      _content: iced::Rectangle,
      _translation: iced::Vector,
      state: &mut dyn widget::operation::Scrollable,
    ) {
      if id == Some(&self.id) {
        let step = RAIL_CARD_WIDTH + TOKENS.spacing.s3;
        state.scroll_to(widget::operation::scrollable::AbsoluteOffset {
          x: Some((self.index as f32 * step - (bounds.width - RAIL_CARD_WIDTH) / 2.0).max(0.0)),
          y: None,
        });
      }
    }
  }
  widget::operate(Reveal {
    index,
    id: hero_rail_scroll_id(),
  })
}

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
  responsive(move |bounds| home_content(state, bounds)).into()
}

fn home_content(state: &State, viewport: iced::Size) -> Element<'_, Message> {
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  let has_continue_watching = state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .row(HomeSection::ContinueWatching)
    .is_some_and(|row| matches!(&row.items, LoadState::Ready(items) if !items.is_empty()));
  let hero_height = hero_height(viewport.height, has_continue_watching);

  let mut content = Column::new().width(Fill);

  let featured = state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .featured_item()
    .map(|item| {
      (
        item,
        state
          .full
          .as_ref()
          .expect("FullUi required")
          .home
          .data
          .featured_section(),
      )
    });
  let mut scrim_start = hero_height;
  if featured.is_some() || home_is_loading(state) {
    content = content.push(if let Some((item, section)) = featured {
      let (hero, metadata_top) = featured_hero(
        state,
        item,
        section.unwrap_or(HomeSection::ContinueWatching),
        viewport.width,
        hero_height,
        skeleton_phase,
        reduced_motion,
      );
      scrim_start = metadata_top;
      hero
    } else {
      featured_skeleton(skeleton_phase, reduced_motion, hero_height)
    });
  }

  let mut rows = Column::new()
    .spacing(TOKENS.spacing.s4)
    .padding(iced::Padding {
      top: TOKENS.spacing.s4,
      right: TOKENS.spacing.s8,
      bottom: TOKENS.spacing.s2,
      left: TOKENS.spacing.s8,
    })
    .width(Fill);

  for row in state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .rows()
  {
    if let Some(section) = section_view(state, row, skeleton_phase, reduced_motion) {
      rows = rows.push(section);
    }
  }

  let (background, backdrop_height) = hero_imagery(
    state,
    featured.and_then(|(item, _)| {
      state
        .full
        .as_ref()
        .expect("FullUi required")
        .home
        .artwork
        .hero_backdrop(&item.id)
    }),
    viewport.width,
    scrim_start,
  );
  // Only the foreground determines row positions. The image can extend
  // below it, but must not shrink to fit a short page or push resume down.
  // Keep the spacer's Shrink width: Row drops Fixed(0)-width children.
  let page = Stack::new()
    .push_under(background)
    .push(
      row![
        content.push(rows),
        space::vertical().height(backdrop_height),
      ]
      .width(Fill),
    )
    .width(Fill);
  scrollable(page)
    .id(widget::Id::new("home-page"))
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
  section: HomeSection,
  width: f32,
  hero_height: f32,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> (Element<'a, Message>, f32) {
  let palette = state.palette();
  let home = &state.full.as_ref().expect("FullUi required").home;
  let text_zone = ((width - TOKENS.spacing.s8 * 2.0) * 0.4).clamp(HERO_MIN_TEXT_ZONE, 480.0);
  let metadata_size = if hero_height < 200.0 { 12.0 } else { 14.0 };
  let secondary = hero_secondary_line(state, section, item);
  let metadata_height = if secondary.is_empty() {
    0.0
  } else {
    metadata_size * 1.3 + TOKENS.spacing.s0_5
  };
  let headline_height =
    (hero_height - TOKENS.spacing.s4 * 2.0 - metadata_height - TOKENS.spacing.s3 - 40.0)
      .max(0.0)
      .min(hero_logo_height(hero_height) * 1.5);
  let copy_height = headline_height + metadata_height + TOKENS.spacing.s3 + 40.0;
  let headline = hero_artwork(
    state,
    home.artwork.hero(&item.id),
    item,
    hero_logo_height(hero_height),
    iced::Size::new(text_zone, headline_height),
    (hero_height * 0.15)
      .clamp(20.0, 42.0)
      .min(headline_height / 1.3),
  );
  let mut info = Column::new()
    .spacing(TOKENS.spacing.s0_5)
    .align_x(Alignment::Start)
    .push(headline);
  if !secondary.is_empty() {
    info = info.push(
      ellipsis_text(secondary)
        .size(metadata_size)
        .color(palette.text.secondary),
    );
  }

  let play_label = if has_resume_position(item) {
    "Resume"
  } else {
    "Play"
  };
  let play_enabled = state.playback.view.engine_available;
  let play = control_button(
    Some(Icon::Play),
    Some(play_label.to_owned()),
    ButtonVariant::Primary,
  )
  .spacing(TOKENS.spacing.s2)
  .padding([7, 14])
  .min_height(40.0)
  .id(iced::widget::Id::new("home-hero-play"))
  .on_press_maybe(play_enabled.then(|| play_message(state, item)));
  let details = control_button(
    Some(Icon::Info),
    Some("Details".to_owned()),
    ButtonVariant::Tonal,
  )
  .spacing(TOKENS.spacing.s2)
  .padding([7, 14])
  .min_height(40.0)
  .id(iced::widget::Id::new("home-hero-details"))
  .on_press(Message::OpenDetail(item.clone()));
  info = info.push(
    container(row![play, details].spacing(TOKENS.spacing.s2)).padding(iced::Padding {
      top: TOKENS.spacing.s2,
      ..iced::Padding::ZERO
    }),
  );

  let candidates: Vec<(HomeSection, &VideoLibraryItem)> = home.data.hero_candidates().collect();
  let selected_index = candidates
    .iter()
    .position(|(_, candidate)| candidate.id == item.id)
    .unwrap_or(0);
  let mut selection = Row::new()
    .spacing(TOKENS.spacing.s3)
    .align_y(Alignment::Center)
    .width(Fill);
  // One candidate is a static hero: no rail, no navigation controls.
  if candidates.len() > 1 {
    let mut cards = Row::new()
      .spacing(TOKENS.spacing.s3)
      .align_y(Alignment::Start);
    for (index, (candidate_section, candidate)) in candidates.iter().enumerate() {
      cards = cards.push(hero_rail_card(
        state,
        *candidate_section,
        candidate,
        index == selected_index,
        skeleton_phase,
        reduced_motion,
      ));
    }
    let rail = scrollable(cards)
      .id(hero_rail_scroll_id())
      .direction(Direction::Horizontal(Scrollbar::new()))
      .width(Fill)
      .height(RAIL_ROW_HEIGHT)
      .style(jellypilot_ui::theme::scrollable);
    let previous_id = selected_index
      .checked_sub(1)
      .and_then(|index| candidates.get(index))
      .map(|(_, candidate)| candidate.id.clone());
    let next_id = candidates
      .get(selected_index + 1)
      .map(|(_, candidate)| candidate.id.clone());
    let previous = control_button(Some(Icon::ChevronLeft), None, ButtonVariant::Text)
      .icon_size(IconSize::Sm)
      .padding(6)
      .width(Length::Fixed(40.0))
      .min_height(40.0)
      .id(iced::widget::Id::new("home-hero-rail-prev"))
      .on_press_maybe(previous_id.map(|id| Message::Home(HomeMessage::HeroSelected(id))));
    let next = control_button(Some(Icon::ChevronRight), None, ButtonVariant::Text)
      .icon_size(IconSize::Sm)
      .padding(6)
      .width(Length::Fixed(40.0))
      .min_height(40.0)
      .id(iced::widget::Id::new("home-hero-rail-next"))
      .on_press_maybe(next_id.map(|id| Message::Home(HomeMessage::HeroSelected(id))));
    selection = selection.push(rail).push(previous).push(next);
  }

  let foreground = container(
    container(info)
      .width(text_zone)
      .height(copy_height)
      .align_y(Alignment::End),
  )
  .padding(iced::Padding {
    left: TOKENS.spacing.s8,
    bottom: TOKENS.spacing.s4,
    ..iced::Padding::ZERO
  })
  .width(Fill)
  .height(Fill)
  .align_y(Alignment::End);
  let selection = container(selection)
    .padding(iced::Padding {
      left: TOKENS.spacing.s8 + text_zone + TOKENS.spacing.s6,
      right: TOKENS.spacing.s8,
      bottom: TOKENS.spacing.s4,
      top: 0.0,
    })
    .width(Fill)
    .height(Fill)
    .align_y(Alignment::End);

  let hero = container(stack![foreground, selection])
    .width(Fill)
    .height(hero_height)
    .clip(true);
  (
    hero.into(),
    hero_height - TOKENS.spacing.s4 - 40.0 - TOKENS.spacing.s3 - metadata_height,
  )
}

fn featured_skeleton<'a>(
  phase: f32,
  reduced_motion: bool,
  hero_height: f32,
) -> Element<'a, Message> {
  let backdrop = skeleton_block(Fill, hero_height, phase, reduced_motion);
  let copy = column![
    skeleton_block(360.0, hero_logo_height(hero_height), phase, reduced_motion),
    skeleton_block(280.0, 18.0, phase, reduced_motion),
    row![
      skeleton_block(112.0, 34.0, phase, reduced_motion),
      skeleton_block(112.0, 34.0, phase, reduced_motion),
    ]
    .spacing(TOKENS.spacing.s2),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_x(Alignment::Start);
  let foreground = container(copy)
    .padding(iced::Padding {
      top: TOKENS.spacing.s2,
      right: TOKENS.spacing.s8,
      bottom: TOKENS.spacing.s4,
      left: TOKENS.spacing.s8,
    })
    .width(Fill)
    .height(hero_height)
    .align_y(Alignment::End);

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
    LoadState::Ready(items) if items.is_empty() => None,
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
  for item in items {
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
      .size(SECTION_TITLE_SIZE)
      .color(state.palette().text.heading),
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
  let cell = state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .artwork
    .card(section, &item.id);

  let text_stack = column![
    ellipsis_text(card_title(item))
      .size(14)
      .color(palette.text.heading),
    ellipsis_text(card_subtitle(item))
      .size(12)
      .color(palette.text.metadata),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);

  if is_action_card {
    let play_enabled = state.playback.view.engine_available;
    // A ControlButton (not the plain iced button) gives the direct-resume
    // surface a stable widget id and keyboard activation; the custom style
    // keeps the artwork chrome-free so only the keyboard focus ring draws.
    let playable_artwork = control_button_content(
      move |_| -> Element<'a, Message> {
        card_artwork(
          state,
          cell,
          card_title(item),
          (frame_width, frame_height),
          radius,
          skeleton_phase,
          reduced_motion,
        )
      },
      ButtonVariant::Text,
    )
    .padding(0)
    .width(Length::Fixed(frame_width))
    .min_height(frame_height)
    .id(iced::widget::Id::from(format!(
      "home-card-play-{}-{}",
      section.index(),
      item.id
    )))
    .on_press_maybe(play_enabled.then(|| play_message(state, item)))
    .style(artwork_button_style);
    let mut artwork_layers = Stack::new()
      .width(frame_width)
      .height(frame_height)
      .push(playable_artwork);
    if let Some(progress) = card_progress(section, item) {
      let frosted_strip = cell.and_then(|cell| {
        state
          .kernel
          .artwork_handles
          .frosted_strip(cell.slot, &cell.image_id)
      });
      let frosted = frosted_strip.is_some();
      if let Some(strip) = frosted_strip {
        artwork_layers = artwork_layers.push(
          container(
            Image::new(strip.clone())
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
      let details = control_button(Some(Icon::Info), None, ButtonVariant::Tonal)
        .icon_size(IconSize::Xs)
        .padding(7)
        .on_press(Message::OpenDetail(item.clone()));
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

  let poster = card_artwork(
    state,
    cell,
    card_title(item),
    (frame_width, frame_height),
    radius,
    skeleton_phase,
    reduced_motion,
  );

  let copy = column![
    ellipsis_text(card_title(item))
      .size(14)
      .color(palette.text.heading),
    ellipsis_text(card_subtitle(item))
      .size(12)
      .color(palette.text.metadata),
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
/// A full-width, aspect-preserving page underlay. The transparent-to-solid
/// vertical fade follows the detail Hero's readable-copy/solid-tail pattern.
fn hero_imagery<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  width: f32,
  scrim_start: f32,
) -> (Element<'a, Message>, f32) {
  let empty = || (space::horizontal().height(0).into(), 0.0);
  let Some(cell) = cell.filter(|cell| cell.state == ArtworkCellState::Ready) else {
    return empty();
  };
  let Some(handle) = state.kernel.artwork_handles.get(cell.slot, &cell.image_id) else {
    return empty();
  };
  let Some((image_width, image_height)) = state
    .kernel
    .artwork_handles
    .dims(cell.slot, &cell.image_id)
    .filter(|&(width, height)| width > 0 && height > 0)
  else {
    return empty();
  };
  let height = width * image_height as f32 / image_width as f32;
  let image = container(
    Image::new(handle.clone())
      .content_fit(ContentFit::Contain)
      .width(Fill)
      .height(height),
  )
  .id(widget::Id::new("home-backdrop"))
  .width(Fill)
  .height(height);
  let background = state.palette().colors.background;
  let fade = gradient::Linear::new(Degrees(180.0))
    .add_stop(0.0, background.scale_alpha(0.0))
    .add_stop(
      (scrim_start / height.max(1.0)).clamp(0.0, 1.0),
      background.scale_alpha(0.97),
    )
    .add_stop(1.0, background);
  (
    stack![image, hero_fade(fade)]
      .width(Fill)
      .height(height)
      .into(),
    height,
  )
}

fn hero_artwork<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  item: &'a VideoLibraryItem,
  ref_height: f32,
  bounds: iced::Size,
  text_size: f32,
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
          .map(|(w, h)| logo_display_size(w, h, ref_height))
          .unwrap_or((0.0, ref_height));
        // Include the baked shadow margin when fitting wide and tall logos.
        let scale = (bounds.width / (logo_width + logo_height / 2.0).max(1.0))
          .min(bounds.height / (logo_height * 1.5).max(1.0))
          .min(1.0);
        let logo_width = logo_width * scale;
        let logo_height = logo_height * scale;
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

  ellipsis_text(hero_headline(item))
    .font(SPACE_GROTESK_FONT)
    .size(text_size)
    .color(state.palette().text.heading)
    .into()
}

/// Secondary hero line: episode identity ("S3:E5 - Title") plus runtime for
/// episodes, the year/runtime line otherwise. Latest-content heroes are
/// prefixed with the row title so the latest fallback never masquerades as
/// Continue Watching or Next Up.
fn hero_secondary_line(state: &State, section: HomeSection, item: &VideoLibraryItem) -> String {
  let mut parts = Vec::with_capacity(3);
  if section.is_latest() {
    if let Some(row) = state
      .full
      .as_ref()
      .expect("FullUi required")
      .home
      .data
      .row(section)
    {
      parts.push(row.title.clone());
    }
  }
  if is_episode_item(item) {
    let subtitle = card_subtitle(item);
    if !subtitle.is_empty() {
      parts.push(subtitle);
    }
    if let Some(runtime) = item.runtime_seconds.and_then(runtime_caption) {
      parts.push(runtime);
    }
  } else {
    parts.push(hero_metadata(item));
  }
  parts.join(" · ")
}

/// Concrete episode identity and honest action/source role for the rail tooltip.
fn rail_label(section: HomeSection, item: &VideoLibraryItem) -> String {
  let role = if has_resume_position(item) {
    "Resume"
  } else if section == HomeSection::NextUp {
    "Next Up"
  } else if section == HomeSection::ContinueWatching {
    "Continue Watching"
  } else {
    "Latest"
  };
  let subtitle = card_subtitle(item);
  if subtitle.is_empty() {
    format!("{}\n{role}", card_title(item))
  } else {
    format!("{}\n{subtitle}\n{role}", card_title(item))
  }
}

/// Image-only selection surface; hover/focus reveals identity without
/// selecting it. Selection and keyboard focus keep distinct rings.
fn hero_rail_card<'a>(
  state: &'a State,
  section: HomeSection,
  item: &'a VideoLibraryItem,
  selected: bool,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  let radius = full_radius(TOKENS.radii.md);
  let cell = state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .artwork
    .selection_card(section, &item.id);
  let title = card_title(item);
  let content = move |_| -> Element<'a, Message> {
    card_artwork(
      state,
      cell,
      title,
      (RAIL_IMAGE_WIDTH, RAIL_IMAGE_HEIGHT),
      radius,
      skeleton_phase,
      reduced_motion,
    )
  };
  let card = control_button_content(content, ButtonVariant::Text)
    .padding(TOKENS.spacing.s0_5)
    .width(Length::Fixed(RAIL_IMAGE_WIDTH + TOKENS.spacing.s0_5 * 2.0))
    .id(iced::widget::Id::from(format!(
      "home-hero-rail-card-{}",
      item.id
    )))
    .on_press(Message::Home(HomeMessage::HeroSelected(item.id.clone())));
  let card = container(card)
    .padding(2.0)
    .style(move |_| iced::widget::container::Style {
      border: iced::Border {
        color: if selected {
          palette.colors.primary
        } else {
          iced::Color::TRANSPARENT
        },
        width: 2.0,
        radius: full_radius(TOKENS.radii.lg),
      },
      ..iced::widget::container::Style::default()
    });
  focus_tooltip(card, rail_label(section, item), TooltipOptions::default())
}

/// Unframed transition from the scrolling image into the page surface.
fn hero_fade(gradient: gradient::Linear) -> Element<'static, Message> {
  container(space::vertical())
    .width(Fill)
    .height(Fill)
    .style(move |_| iced::widget::container::Style {
      background: Some(Background::Gradient(gradient.into())),
      ..iced::widget::container::Style::default()
    })
    .into()
}

/// Chrome-free control style for artwork click surfaces: nothing draws at
/// rest, on hover, or when disabled; ControlButton adds the keyboard focus
/// ring on top with the artwork's corner radius.
fn artwork_button_style(
  _theme: &iced::Theme,
  _variant: ButtonVariant,
  _status: button::Status,
) -> button::Style {
  button::Style {
    border: iced::Border {
      radius: full_radius(TOKENS.radii.lg),
      ..iced::Border::default()
    },
    ..button::Style::default()
  }
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

  let placeholder_color = match cell.map(|cell| cell.state) {
    // No planned load (the server carries no image for this slot): settle on
    // a neutral placeholder instead of shimmering forever.
    None => Some(palette.text.metadata),
    Some(ArtworkCellState::Failed) => Some(palette.colors.warning),
    _ => None,
  };
  if let Some(placeholder_color) = placeholder_color {
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

pub(crate) const PROGRESS_BAR_HEIGHT: f32 = 4.0;

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
    fill: palette.colors.primary.scale_alpha(0.5),
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
      .color(palette.text.heading),
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
  let retry = control_button(None, Some("Retry".to_owned()), ButtonVariant::Tonal)
    .padding([6, 12])
    .on_press(Message::Home(HomeMessage::Retry));
  container(
    column![
      text(title)
        .font(SPACE_GROTESK_FONT)
        .size(24)
        .color(palette.text.heading),
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
    let expected = 1600.0 - 240.0 - super::super::shell::HAIRLINE_WIDTH - TOKENS.spacing.s8 * 2.0;
    assert_eq!(content_width(1600.0, SizeClass::Standard), expected);
    assert_eq!(content_width(1600.0, SizeClass::Standard), 1295.0);
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
