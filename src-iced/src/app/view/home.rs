use super::image_observer::{observe_image, ImageAxis};
use crate::app::artwork::{ArtworkSurface, ImageCell, ImageSpec, ImageStatus};
use crate::app::collections::{self, Source};
use crate::app::home::ArtworkPlacement;
use crate::app::message::{HomeMessage, Message, PlaybackMessage};
use crate::app::state::{HomeRow, HomeSection, State};
use crate::i18n::media::{card_subtitle, hero_metadata, runtime_caption};
use crate::i18n::{Localizer, UiText};
use iced::advanced::{layout, renderer, widget, Layout, Renderer as _, Widget};
use iced::gradient;
use iced::widget::image::Image;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
  button, column, container, mouse_area, responsive, row, scrollable, space, stack, text, Column,
  Row, Stack,
};
use iced::{Alignment, Background, ContentFit, Degrees, Element, Fill, Length};
use jellypilot_core::cards::{card_title, hero_headline, is_episode_item, logo_display_size};
use jellypilot_core::home_hero::has_resume_position;
use jellypilot_core::LoadState;
use jellypilot_media_server::VideoLibraryItem;
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{icon_sized, icon_with_color, Icon, IconControlState, IconSize};
use jellypilot_ui::overlay::{focus_tooltip, TooltipOptions};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::skeleton::{
  skeleton_block, skeleton_block_with_radius, skeleton_panel,
};
use jellypilot_ui::{full_radius, poster_card, rounded_image};
const THUMB_FRAME_WIDTH: f32 = 272.0;
const THUMB_FRAME_HEIGHT: f32 = 153.0;
const POSTER_FRAME_WIDTH: f32 = 175.0;
const POSTER_FRAME_HEIGHT: f32 = 262.0;
const HERO_HEIGHT: f32 = 440.0;
const SECTION_TITLE_SIZE: f32 = 20.0;
const HERO_LOGO_HEIGHT: f32 = 88.0;
/// The rail stays smaller than direct-resume cards, including its focus gutter.
const RAIL_IMAGE_WIDTH: f32 = 96.0;
const RAIL_IMAGE_HEIGHT: f32 = 54.0;
const RAIL_CARD_WIDTH: f32 = RAIL_IMAGE_WIDTH + TOKENS.spacing.s0_5 * 2.0 + 4.0;
const RAIL_ROW_HEIGHT: f32 = 76.0;

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
        let step = RAIL_CARD_WIDTH + TOKENS.spacing.s2_5;
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

pub(crate) const fn section_frame_size(section: HomeSection) -> (f32, f32) {
  if section.is_action() {
    (THUMB_FRAME_WIDTH, THUMB_FRAME_HEIGHT)
  } else {
    (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT)
  }
}

const fn section_scroll_height(section: HomeSection) -> f32 {
  if section.is_action() {
    THUMB_FRAME_HEIGHT + 58.0
  } else {
    POSTER_FRAME_HEIGHT + 58.0
  }
}

pub fn view(state: &State) -> Element<'_, Message> {
  responsive(move |bounds| home_content(state, bounds)).into()
}

fn home_content(state: &State, viewport: iced::Size) -> Element<'_, Message> {
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();

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
  let mut scrim_start = HERO_HEIGHT;
  if featured.is_some() || home_is_loading(state) {
    content = content.push(if let Some((item, section)) = featured {
      let (hero, metadata_top) = featured_hero(
        state,
        item,
        section.unwrap_or(HomeSection::ContinueWatching),
        viewport.width,
        skeleton_phase,
        reduced_motion,
      );
      scrim_start = metadata_top;
      hero
    } else {
      featured_skeleton(skeleton_phase, reduced_motion)
    });
  }

  let mut rows = Column::new()
    .padding(iced::Padding {
      top: 0.0,
      right: TOKENS.spacing.s9,
      bottom: TOKENS.spacing.s9,
      left: TOKENS.spacing.s9,
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
      rows = rows.push(container(section).padding(iced::Padding {
        top: if row.section.is_latest() {
          TOKENS.spacing.s9
        } else {
          TOKENS.spacing.s7
        },
        ..iced::Padding::ZERO
      }));
    }
  }

  let (background, backdrop_height) = hero_imagery(
    state,
    featured.map(|(item, _)| item),
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
    .id(widget::Id::new(if home_is_loading(state) {
      "home-page-loading"
    } else {
      "home-page"
    }))
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
  skeleton_phase: f32,
  reduced_motion: bool,
) -> (Element<'a, Message>, f32) {
  let palette = state.palette();
  let home = &state.full.as_ref().expect("FullUi required").home;
  let inner_width = (width - TOKENS.spacing.s9 * 2.0).max(1.0);
  // Disjoint columns keep localized actions and long titles out of the rail.
  // Shrink the rail viewport, never the fixed foreground, on narrow windows.
  let text_zone = (inner_width * 0.46).clamp(340.0, 480.0).min(inner_width);
  let rail_width = (inner_width - text_zone - TOKENS.spacing.s6).clamp(0.0, 630.0);
  let metadata_size = 12.0;
  let secondary = hero_secondary_line(state, section, item);
  let metadata_height = if secondary.is_empty() {
    0.0
  } else {
    metadata_size * 1.3 + TOKENS.spacing.s2_5
  };
  let headline_height = HERO_LOGO_HEIGHT * 1.5;
  let headline = hero_artwork(
    state,
    home.artwork.get(&ArtworkPlacement::Hero.key(&item.id)),
    item,
    HERO_LOGO_HEIGHT,
    iced::Size::new(text_zone, headline_height),
    45.0,
  );
  let headline = observe_home(
    headline,
    state,
    ArtworkPlacement::Hero.spec(item),
    ImageAxis::Vertical,
  );
  let mut info = Column::new()
    .spacing(TOKENS.spacing.s2_5)
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
    state.t("home-resume")
  } else {
    state.t("home-play")
  };
  let play_enabled = state.playback.view.engine_available;
  let scrim_start = HERO_HEIGHT - TOKENS.spacing.s9 - 40.0 - TOKENS.spacing.s2_5 - metadata_height;
  let backdrop = hero_backdrop(
    state,
    home
      .artwork
      .get(&ArtworkPlacement::HeroBackdrop.key(&item.id)),
    width,
    scrim_start,
  );
  let hero_style =
    |variant: ButtonVariant| -> fn(&iced::Theme, ButtonVariant, button::Status) -> button::Style {
      if backdrop.is_some() && variant != ButtonVariant::Primary {
        jellypilot_ui::widgets::button::hero_glass
      } else {
        jellypilot_ui::widgets::button::style
      }
    };
  let action = |icon, label: String, variant| {
    let style = hero_style(variant);
    control_button_content(
      move |control_state| {
        let status = match control_state {
          IconControlState::Rest => button::Status::Active,
          IconControlState::Hovered => button::Status::Hovered,
          IconControlState::Disabled => button::Status::Disabled,
        };
        row![
          icon_sized(icon, IconSize::Custom(15.0)).style(move |theme, _| {
            iced::widget::svg::Style {
              color: Some(style(theme, variant, status).text_color),
            }
          }),
          container(
            text(label.clone())
              .size(14)
              .font(HEADING_FONT)
              .style(move |theme| text::Style {
                color: Some(style(theme, variant, status).text_color),
              }),
          )
          .width(Length::Shrink),
        ]
        .spacing(TOKENS.spacing.s2)
        .align_y(Alignment::Center)
        .into()
      },
      variant,
    )
    .style(style)
    .padding([8, 16])
    .min_height(40.0)
    .width(Length::Shrink)
  };
  let play = action(Icon::Play, play_label.clone(), ButtonVariant::Primary)
    .id(iced::widget::Id::new("home-hero-play"))
    .on_press_maybe(play_enabled.then(|| play_message(state, item)));
  let details_label = state.t("home-details");
  let details = action(Icon::Info, details_label.clone(), ButtonVariant::Tonal)
    .id(iced::widget::Id::new("home-hero-details"))
    .on_press(Message::OpenDetail(Box::new(item.clone())));
  let controls = collections::controls(state, Source::Hero);
  let favorite_variant = if controls.favorite == Some(true) {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  let favorite = control_button(
    Some(if controls.favorite == Some(true) {
      Icon::HeartFilled
    } else {
      Icon::Heart
    }),
    None,
    favorite_variant,
  )
  .style(hero_style(favorite_variant))
  .icon_size(IconSize::Sm)
  .padding(12)
  .width(Length::Fixed(40.0))
  .min_height(40.0)
  .id(iced::widget::Id::new("home-hero-favorite"))
  .on_press_maybe(controls.favorite_action);
  let watchlist_variant = if controls.watchlisted == Some(true) {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  let watchlist = control_button(
    Some(if controls.watchlisted == Some(true) {
      Icon::BookmarkFilled
    } else {
      Icon::Bookmark
    }),
    None,
    watchlist_variant,
  )
  .style(hero_style(watchlist_variant))
  .icon_size(IconSize::Sm)
  .padding(12)
  .width(Length::Fixed(40.0))
  .min_height(40.0)
  .id(iced::widget::Id::new("home-hero-watchlist"))
  .on_press_maybe(controls.watchlist_action);
  info = info.push(
    row![
      focus_tooltip(play, play_label, TooltipOptions::default()),
      focus_tooltip(details, details_label, TooltipOptions::default()),
      focus_tooltip(favorite, controls.favorite_label, TooltipOptions::default()),
      focus_tooltip(
        watchlist,
        controls.watchlist_label,
        TooltipOptions::default()
      ),
    ]
    .spacing(TOKENS.spacing.s2_5)
    .align_y(Alignment::Center),
  );

  let candidates: Vec<(HomeSection, &VideoLibraryItem)> = home.data.hero_candidates().collect();
  let selected_index = candidates
    .iter()
    .position(|(_, candidate)| candidate.id == item.id)
    .unwrap_or(0);
  let mut selection = Row::new()
    .spacing(TOKENS.spacing.s2_5)
    .align_y(Alignment::Center)
    .width(Fill);
  // One candidate is a static hero: no rail, no navigation controls.
  if candidates.len() > 1 {
    let mut cards = Row::new()
      .spacing(TOKENS.spacing.s2_5)
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
      .content_centered(true)
      .spacing(0.0)
      .padding(6)
      .width(Length::Fixed(40.0))
      .min_height(40.0)
      .style(hero_style(ButtonVariant::Text))
      .id(iced::widget::Id::new("home-hero-rail-prev"))
      .on_press_maybe(previous_id.map(|id| Message::Home(HomeMessage::HeroSelected(id))));
    let next = control_button(Some(Icon::ChevronRight), None, ButtonVariant::Text)
      .icon_size(IconSize::Sm)
      .content_centered(true)
      .spacing(0.0)
      .padding(6)
      .width(Length::Fixed(40.0))
      .min_height(40.0)
      .style(hero_style(ButtonVariant::Text))
      .id(iced::widget::Id::new("home-hero-rail-next"))
      .on_press_maybe(next_id.map(|id| Message::Home(HomeMessage::HeroSelected(id))));
    selection = selection.push(rail).push(
      row![previous, next]
        .spacing(TOKENS.spacing.s1)
        .align_y(Alignment::Center),
    );
  }

  let foreground = container(container(info).width(text_zone))
    .padding(iced::Padding {
      left: TOKENS.spacing.s9,
      bottom: TOKENS.spacing.s9,
      ..iced::Padding::ZERO
    })
    .width(Fill)
    .height(Fill)
    .align_y(Alignment::End);
  let selection = container(container(selection).width(rail_width))
    .padding(iced::Padding {
      right: TOKENS.spacing.s9,
      bottom: TOKENS.spacing.s9,
      ..iced::Padding::ZERO
    })
    .width(Fill)
    .height(Fill)
    .align_x(Alignment::End)
    .align_y(Alignment::End);

  let hero = container(stack![foreground, selection])
    .width(Fill)
    .height(HERO_HEIGHT)
    .clip(true);
  (
    Element::new(HeroGlass {
      content: hero.into(),
      backdrop,
      buttons: HeroButtonBounds::default(),
    }),
    scrim_start,
  )
}

fn featured_skeleton<'a>(phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  let backdrop = skeleton_block(Fill, HERO_HEIGHT, phase, reduced_motion);
  let copy = column![
    skeleton_block(340.0, HERO_LOGO_HEIGHT, phase, reduced_motion),
    skeleton_block(280.0, 18.0, phase, reduced_motion),
    row![
      skeleton_block(112.0, 40.0, phase, reduced_motion),
      skeleton_block(112.0, 40.0, phase, reduced_motion),
      skeleton_block(40.0, 40.0, phase, reduced_motion),
      skeleton_block(40.0, 40.0, phase, reduced_motion),
    ]
    .spacing(TOKENS.spacing.s2_5),
  ]
  .spacing(TOKENS.spacing.s2_5)
  .align_x(Alignment::Start);
  let foreground = container(copy)
    .padding(iced::Padding {
      top: TOKENS.spacing.s2,
      right: TOKENS.spacing.s9,
      bottom: TOKENS.spacing.s9,
      left: TOKENS.spacing.s9,
    })
    .width(Fill)
    .height(HERO_HEIGHT)
    .align_y(Alignment::End);

  stack![backdrop, foreground]
    .width(Fill)
    .height(HERO_HEIGHT)
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
      state.kernel.locale,
      row,
      skeleton_phase,
      reduced_motion,
    )),
    LoadState::Failed(error) => Some(section_error(
      state.palette(),
      state.kernel.locale,
      &row.title,
      error,
    )),
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
    .spacing(TOKENS.spacing.s5)
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
    .id(widget::Id::from(format!(
      "home-row-{}",
      home_row.section.index()
    )))
    .direction(Direction::Horizontal(Scrollbar::new()))
    .height(section_scroll_height(home_row.section))
    .style(jellypilot_ui::theme::scrollable);

  column![
    text(state.kernel.locale.message(&home_row.title))
      .font(HEADING_FONT)
      .size(SECTION_TITLE_SIZE)
      .color(state.palette().text.heading),
    cards,
  ]
  .spacing(TOKENS.spacing.s3_5)
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
  let radius = full_radius(TOKENS.radii.xl);
  let spec = ArtworkPlacement::Card(section).spec(item);
  let cell = state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .artwork
    .get(&ArtworkPlacement::Card(section).key(&item.id));

  let text_stack = column![
    ellipsis_text(card_title(item))
      .font(HEADING_FONT)
      .size(14)
      .color(palette.text.heading),
    ellipsis_text(card_subtitle(state.kernel.locale, item))
      .size(12)
      .color(palette.text.metadata),
  ]
  .spacing(TOKENS.spacing.s2)
  .width(Fill);

  if is_action_card {
    let play_enabled = state.playback.view.engine_available;
    // A ControlButton (not the plain iced button) gives the direct-resume
    // surface a stable widget id and keyboard activation; the custom style
    // keeps the artwork chrome-free so only the keyboard focus ring draws.
    let playable_artwork = control_button_content(
      move |_| -> Element<'a, Message> {
        let mut artwork = Stack::new().push(card_artwork(
          state,
          spec.clone(),
          card_title(item),
          (frame_width, frame_height),
          radius,
          skeleton_phase,
          reduced_motion,
        ));
        if let Some(progress) = card_progress(section, item) {
          let handle = cell.and_then(ImageCell::handle);
          artwork = artwork.push(
            container(progress_bar(
              palette,
              progress,
              radius,
              frame_height,
              handle.cloned(),
            ))
            .width(Fill)
            .height(Fill)
            .align_y(Alignment::End),
          );
        }
        artwork.into()
      },
      ButtonVariant::Text,
    )
    .overlay_border()
    .padding(0)
    .width(Length::Fixed(frame_width))
    .min_height(frame_height)
    .id(iced::widget::Id::from(format!(
      "home-card-play-{}-{}",
      section.index(),
      item.id
    )))
    .on_press_maybe(play_enabled.then(|| play_message(state, item)))
    .style(jellypilot_ui::widgets::button::media_artwork);
    let mut artwork_layers = Stack::new()
      .width(frame_width)
      .height(frame_height)
      .push(playable_artwork);
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
        .on_press(Message::OpenDetail(Box::new(item.clone())));
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
    .clip(true);
    let copy = container(text_stack)
      .padding(iced::Padding {
        top: TOKENS.spacing.s2,
        right: 0.0,
        bottom: 0.0,
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
    spec,
    card_title(item),
    (frame_width, frame_height),
    radius,
    skeleton_phase,
    reduced_motion,
  );

  let copy = column![
    ellipsis_text(card_title(item))
      .font(HEADING_FONT)
      .size(14)
      .color(palette.text.heading),
    ellipsis_text(card_subtitle(state.kernel.locale, item))
      .size(12)
      .color(palette.text.metadata),
  ]
  .spacing(TOKENS.spacing.s2)
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
    .on_press(Message::OpenDetail(Box::new(item.clone())))
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
        }
        .smoothing(jellypilot_ui::widgets::container::SURFACE_SMOOTHING),
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
  item: Option<&VideoLibraryItem>,
  width: f32,
  scrim_start: f32,
) -> (Element<'a, Message>, f32) {
  let spec = item.and_then(|item| ArtworkPlacement::HeroBackdrop.spec(item));
  let cell = spec.as_ref().and_then(|spec| {
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .home
      .artwork
      .get(&spec.key)
  });
  let Some(backdrop) = hero_backdrop(state, cell, width, scrim_start) else {
    // The under-layer does not affect layout; keep a measurable image area
    // while the foreground still determines the page's height.
    let placeholder = space::horizontal()
      .width(width)
      .height(width * 9.0 / 16.0)
      .into();
    return (
      observe_home(placeholder, state, spec, ImageAxis::Vertical),
      0.0,
    );
  };
  let height = backdrop.size.height;
  let image = container(
    Image::new(backdrop.handle)
      .content_fit(ContentFit::Contain)
      .width(Fill)
      .height(height),
  )
  .id(widget::Id::new("home-backdrop"))
  .width(Fill)
  .height(height);
  let fade = backdrop.fade;
  (
    observe_home(
      stack![image, hero_fade(fade)]
        .width(Fill)
        .height(height)
        .into(),
      state,
      spec,
      ImageAxis::Vertical,
    ),
    height,
  )
}

struct HeroBackdrop {
  handle: iced::widget::image::Handle,
  size: iced::Size,
  fade: gradient::Linear,
}

fn hero_backdrop(
  state: &State,
  cell: Option<&ImageCell>,
  width: f32,
  scrim_start: f32,
) -> Option<HeroBackdrop> {
  let cell = cell?;
  let handle = cell.handle()?;
  let (image_width, image_height) = cell
    .dims()
    .filter(|&(width, height)| width > 0 && height > 0)?;
  let height = width * image_height as f32 / image_width as f32;
  let background = state.palette().colors.background;
  let fade = gradient::Linear::new(Degrees(180.0))
    .add_stop(0.0, background.scale_alpha(0.0))
    .add_stop(
      (scrim_start / height.max(1.0)).clamp(0.0, 1.0),
      background.scale_alpha(0.97),
    )
    .add_stop(1.0, background);
  Some(HeroBackdrop {
    handle: handle.clone(),
    size: iced::Size::new(width, height),
    fade,
  })
}

#[derive(Default)]
struct HeroButtonBounds([Option<HeroButtonMask>; 5]);

struct HeroButtonMask {
  bounds: iced::Rectangle,
  radius: f32,
}

impl HeroButtonBounds {
  const GLASS_BUTTONS: [(&'static str, f32); 5] = [
    ("home-hero-details", TOKENS.radii.xl),
    ("home-hero-favorite", TOKENS.radii.xl),
    ("home-hero-watchlist", TOKENS.radii.xl),
    ("home-hero-rail-prev", TOKENS.radii.xl),
    ("home-hero-rail-next", TOKENS.radii.xl),
  ];
}

impl widget::Operation for HeroButtonBounds {
  fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
    operate(self);
  }

  fn focusable(
    &mut self,
    id: Option<&widget::Id>,
    bounds: iced::Rectangle,
    _state: &mut dyn widget::operation::Focusable,
  ) {
    for (index, (name, radius)) in Self::GLASS_BUTTONS.iter().enumerate() {
      if id == Some(&widget::Id::new(name)) {
        self.0[index] = Some(HeroButtonMask {
          bounds,
          radius: *radius,
        });
      }
    }
  }
}

/// Samples only the Backdrop behind the actual laid-out controls. Measuring
/// their existing IDs avoids guessing localized label widths or copy offsets.
struct HeroGlass<'a> {
  content: Element<'a, Message>,
  backdrop: Option<HeroBackdrop>,
  buttons: HeroButtonBounds,
}

impl Widget<Message, iced::Theme, iced::Renderer> for HeroGlass<'_> {
  fn diff(&mut self, tree: &mut widget::Tree) {
    tree.diff_children(&mut [self.content.as_widget_mut()]);
  }

  fn size(&self) -> iced::Size<Length> {
    self.content.as_widget().size()
  }

  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) -> layout::Node {
    let node = self
      .content
      .as_widget_mut()
      .layout(&mut tree.children[0], renderer, limits);
    self.buttons = HeroButtonBounds::default();
    self.content.as_widget_mut().operate(
      &mut tree.children[0],
      Layout::new(&node),
      renderer,
      &mut self.buttons,
    );
    node
  }

  fn operate(
    &mut self,
    tree: &mut widget::Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    self
      .content
      .as_widget_mut()
      .operate(&mut tree.children[0], layout, renderer, operation);
  }

  fn update(
    &mut self,
    tree: &mut widget::Tree,
    event: &iced::Event,
    layout: Layout<'_>,
    cursor: iced::mouse::Cursor,
    renderer: &iced::Renderer,
    shell: &mut iced::advanced::Shell<'_, Message>,
    viewport: &iced::Rectangle,
  ) {
    self.content.as_widget_mut().update(
      &mut tree.children[0],
      event,
      layout,
      cursor,
      renderer,
      shell,
      viewport,
    );
  }

  fn draw(
    &self,
    tree: &widget::Tree,
    renderer: &mut iced::Renderer,
    theme: &iced::Theme,
    style: &renderer::Style,
    layout: Layout<'_>,
    cursor: iced::mouse::Cursor,
    viewport: &iced::Rectangle,
  ) {
    let Some(visible) = layout.bounds().intersection(viewport) else {
      return;
    };
    let origin = iced::Vector::new(layout.bounds().x, layout.bounds().y);
    if let Some(backdrop) = &self.backdrop {
      for mask in self.buttons.0.iter().flatten() {
        let node = layout::Node::new(mask.bounds.size()).move_to(mask.bounds.position() + origin);
        let image = Image::new(backdrop.handle.clone())
          .content_fit(ContentFit::Contain)
          .display_frame(iced::Rectangle::new(
            iced::Point::new(-mask.bounds.x, -mask.bounds.y),
            backdrop.size,
          ))
          .mask_frame(iced::Rectangle::new(
            iced::Point::ORIGIN,
            mask.bounds.size(),
          ))
          .border_radius(mask.radius)
          .border_smoothing(jellypilot_ui::widgets::container::SURFACE_SMOOTHING)
          .blur(10.0)
          // Image anchors the gradient to the full display frame, not the mask.
          .tint(backdrop.fade);
        <Image as Widget<Message, iced::Theme, iced::Renderer>>::draw(
          &image,
          tree,
          renderer,
          theme,
          style,
          Layout::new(&node),
          cursor,
          &visible,
        );
      }
    }
    // Keep Catalog fills, labels and focus borders above native image batches.
    renderer.with_layer(visible, |renderer| {
      self.content.as_widget().draw(
        &tree.children[0],
        renderer,
        theme,
        style,
        layout,
        cursor,
        &visible,
      );
    });
  }

  fn mouse_interaction(
    &self,
    tree: &widget::Tree,
    layout: Layout<'_>,
    cursor: iced::mouse::Cursor,
    viewport: &iced::Rectangle,
    renderer: &iced::Renderer,
  ) -> iced::mouse::Interaction {
    self.content.as_widget().mouse_interaction(
      &tree.children[0],
      layout,
      cursor,
      viewport,
      renderer,
    )
  }

  fn overlay<'a>(
    &'a mut self,
    tree: &'a mut widget::Tree,
    layout: Layout<'a>,
    renderer: &iced::Renderer,
    viewport: &iced::Rectangle,
    translation: iced::Vector,
  ) -> Option<iced::advanced::overlay::Element<'a, Message, iced::Theme, iced::Renderer>> {
    self.content.as_widget_mut().overlay(
      &mut tree.children[0],
      layout,
      renderer,
      viewport,
      translation,
    )
  }
}

fn hero_artwork<'a>(
  state: &'a State,
  cell: Option<&ImageCell>,
  item: &'a VideoLibraryItem,
  ref_height: f32,
  bounds: iced::Size,
  text_size: f32,
) -> Element<'a, Message> {
  if let Some(cell) = cell {
    if cell.state == ImageStatus::Ready {
      if let Some(handle) = cell.handle() {
        let dims = cell.dims().filter(|&(w, h)| w > 0 && h > 0);
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
            Length::Fit
          });
        // The baked shadow canvas carries a transparent margin (height/4 on
        // top/bottom/right, a constant 3/2 render ratio); indent the logo on
        // top only so the glyph overlaps its shadow while the left edge stays
        // flush with the text below.
        let logo = container(logo_image).padding(iced::Padding {
          top: logo_height / 4.0,
          ..iced::Padding::ZERO
        });
        let Some(shadow) = cell.logo_shadow() else {
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
            Length::Fit
          });
        return stack![container(shadow_image), logo].into();
      }
    }
  }

  ellipsis_text(hero_headline(item))
    .font(DISPLAY_FONT)
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
      parts.push(state.kernel.locale.message(&row.title));
    }
  }
  if is_episode_item(item) {
    let subtitle = card_subtitle(state.kernel.locale, item);
    if !subtitle.is_empty() {
      parts.push(subtitle);
    }
    if let Some(runtime) = item
      .runtime_seconds
      .and_then(|seconds| runtime_caption(state.kernel.locale, seconds))
    {
      parts.push(runtime);
    }
  } else {
    parts.push(hero_metadata(state.kernel.locale, item));
  }
  parts.join(" · ")
}

/// Concrete episode identity and honest action/source role for the rail tooltip.
fn rail_label(locale: Localizer, section: HomeSection, item: &VideoLibraryItem) -> String {
  let role = if has_resume_position(item) {
    locale.text("home-resume")
  } else if section == HomeSection::NextUp {
    locale.text("home-next-up")
  } else if section == HomeSection::ContinueWatching {
    locale.text("home-continue-watching")
  } else {
    locale.text("home-latest-role")
  };
  let subtitle = card_subtitle(locale, item);
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
  let radius = full_radius(TOKENS.radii.lg);
  let spec = ArtworkPlacement::Selection(section).spec(item);
  let title = card_title(item);
  let content = move |_| -> Element<'a, Message> {
    let artwork = card_artwork(
      state,
      spec.clone(),
      title,
      (RAIL_IMAGE_WIDTH, RAIL_IMAGE_HEIGHT),
      radius,
      skeleton_phase,
      reduced_motion,
    );
    Stack::new()
      .push(artwork)
      .push(
        container(space())
          .width(RAIL_IMAGE_WIDTH)
          .height(RAIL_IMAGE_HEIGHT)
          .style(move |_| container::Style {
            border: iced::Border {
              smoothing: jellypilot_ui::widgets::container::SURFACE_SMOOTHING,
              color: if selected {
                palette.colors.primary
              } else {
                iced::Color::TRANSPARENT
              },
              width: 2.0,
              radius,
            },
            snap: false,
            ..container::Style::default()
          }),
      )
      .into()
  };
  let card = control_button_content(content, ButtonVariant::Text)
    .overlay_border()
    .style(|theme, variant, status| {
      let mut style = jellypilot_ui::widgets::button::style(theme, variant, status);
      style.snap = false;
      style
    })
    .padding(TOKENS.spacing.s0_5)
    .width(Length::Fixed(RAIL_IMAGE_WIDTH + TOKENS.spacing.s0_5 * 2.0))
    .id(iced::widget::Id::from(format!(
      "home-hero-rail-card-{}",
      item.id
    )))
    .on_press(Message::Home(HomeMessage::HeroSelected(item.id.clone())));
  let card = container(card).padding(2.0);
  focus_tooltip(
    card,
    rail_label(state.kernel.locale, section, item),
    TooltipOptions::default(),
  )
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

fn observe_home<'a>(
  content: Element<'a, Message>,
  state: &State,
  spec: Option<ImageSpec>,
  axis: ImageAxis,
) -> Element<'a, Message> {
  match spec {
    Some(spec) => observe_image(
      content,
      ArtworkSurface::Home,
      state
        .full
        .as_ref()
        .expect("FullUi required")
        .home
        .artwork
        .epoch(),
      spec,
      axis,
    ),
    None => content,
  }
}

fn card_artwork<'a>(
  state: &'a State,
  spec: Option<ImageSpec>,
  name: &'a str,
  size: (f32, f32),
  radius: iced::border::Radius,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let cell = spec.as_ref().and_then(|spec| {
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .home
      .artwork
      .get(&spec.key)
  });
  let content = if spec.is_some() && cell.is_none() {
    skeleton_panel(
      size.0,
      size.1,
      state.palette().colors.surfaceContainerLowest,
      radius,
      phase,
      reduced_motion,
    )
    .into()
  } else {
    render_card_artwork(state, cell, name, size, radius, phase, reduced_motion)
  };
  observe_home(content, state, spec, ImageAxis::Horizontal)
}

fn render_card_artwork<'a>(
  state: &'a State,
  cell: Option<&ImageCell>,
  name: &'a str,
  (width, height): (f32, f32),
  radius: iced::border::Radius,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  if let Some(cell) = cell {
    if cell.state == ImageStatus::Ready {
      if let Some(handle) = cell.handle() {
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
    Some(ImageStatus::Failed) => Some(palette.colors.warning),
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
          .font(HEADING_FONT)
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
        smoothing: jellypilot_ui::widgets::container::SURFACE_SMOOTHING,
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

pub(crate) const PROGRESS_BAR_HEIGHT: f32 = 6.0;

fn progress_bar<'a>(
  palette: &'static ThemePalette,
  progress: f64,
  radius: iced::border::Radius,
  frame_height: f32,
  artwork: Option<iced::widget::image::Handle>,
) -> Element<'a, Message> {
  use jellypilot_ui::widgets::artwork_progress::{ArtworkProgress, Style};

  let style = Style {
    fill: palette.colors.primary.scale_alpha(0.5),
    track: if artwork.is_some() {
      palette.colors.surfaceContainerLowest.scale_alpha(0.4)
    } else {
      palette.colors.surfaceContainerLow.scale_alpha(0.5)
    },
    blur: 12.0,
  };
  ArtworkProgress::new(
    progress,
    frame_height,
    PROGRESS_BAR_HEIGHT,
    radius,
    artwork,
    style,
  )
  .into()
}

fn section_skeleton<'a>(
  palette: &ThemePalette,
  locale: Localizer,
  row: &'a HomeRow,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let (width, height) = section_frame_size(row.section);
  let mut cards = Row::new().spacing(TOKENS.spacing.s5);
  for _ in 0..5 {
    cards = cards.push(
      column![
        skeleton_block_with_radius(
          width,
          height,
          full_radius(TOKENS.radii.xl),
          phase,
          reduced_motion,
        ),
        skeleton_block(width, 18.0, phase, reduced_motion),
        skeleton_block(width * 0.6, 14.0, phase, reduced_motion),
      ]
      .spacing(TOKENS.spacing.s2),
    );
  }
  column![
    text(locale.message(&row.title))
      .font(HEADING_FONT)
      .size(SECTION_TITLE_SIZE)
      .color(palette.text.heading),
    scrollable(cards)
      .direction(Direction::Horizontal(Scrollbar::new()))
      .height(section_scroll_height(row.section))
      .style(jellypilot_ui::theme::scrollable),
  ]
  .spacing(TOKENS.spacing.s3_5)
  .into()
}

fn section_error<'a>(
  palette: &ThemePalette,
  locale: Localizer,
  title: &'a UiText,
  error: &'a UiText,
) -> Element<'a, Message> {
  let retry = control_button(None, Some(locale.text("home-retry")), ButtonVariant::Tonal)
    .padding([6, 12])
    .on_press(Message::Home(HomeMessage::Retry));
  container(
    column![
      text(locale.message(title))
        .font(HEADING_FONT)
        .size(SECTION_TITLE_SIZE)
        .color(palette.text.heading),
      text(locale.message(error))
        .size(13)
        .color(palette.colors.error),
      retry,
    ]
    .spacing(TOKENS.spacing.s3_5),
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
  fn unplayed_badge_only_formats_nonzero_latest_series_counts() {
    let mut item = VideoLibraryItem {
      community_rating: None,
      episode_count: None,
      last_played_date: None,
      premiere_date: None,
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
