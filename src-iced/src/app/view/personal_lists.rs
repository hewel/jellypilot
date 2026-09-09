use crate::i18n::{Localizer, UiText};
use iced::widget::{
  column, container, responsive, row, scrollable, space, text, Column, Row, Stack,
};
use iced::{Alignment, ContentFit, Element, Fill, Length, Pixels};
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::overlay::{focus_tooltip, TooltipOptions};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::artwork_progress::ArtworkProgress;
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::skeleton::skeleton_panel;
use jellypilot_ui::{full_radius, rounded_image};

use super::image_observer::{observe_image, ImageAxis};
use crate::app::artwork::{ArtworkSurface, ImageStatus};
use crate::app::collections::{self, CollectionMessage};
use crate::app::message::{HomeMessage, Message};
use crate::app::personal_lists::{
  artwork_key, artwork_spec, ItemAvailability, Kind, ListEntry, ListPage, PersonalListsMessage,
  Route, PAGE_SIZE,
};
use crate::app::state::{Destination, State};

const LANDSCAPE_WIDTH: f32 = 272.0;
const POSTER_WIDTH: f32 = 174.66667;
const COPY_HEIGHT: f32 = 50.0;

pub fn view(state: &State) -> Element<'_, Message> {
  responsive(move |bounds| {
    let width = (bounds.width - TOKENS.spacing.s9 * 2.0).max(1.0);
    match state.shell.destination {
      Destination::PersonalLists(Route::Favorites) => list_page(state, Kind::Favorites, width),
      Destination::PersonalLists(Route::Watchlist) => list_page(state, Kind::Watchlist, width),
      Destination::PersonalLists(Route::History) => list_page(state, Kind::History, width),
      _ => overview(state, width),
    }
  })
  .into()
}

fn overview(state: &State, width: f32) -> Element<'_, Message> {
  let lists = &state.full.as_ref().expect("FullUi required").personal_lists;
  let subtitle = state.format(
    "lists-overview-counts",
    &[
      ("watchlist", count_label(&lists.watchlist).into()),
      ("favorites", count_label(&lists.favorites).into()),
      ("history", count_label(&lists.history).into()),
    ],
  );
  let content = column![
    page_heading(state.palette(), state.t("lists-title"), subtitle),
    space().height(TOKENS.spacing.s8),
    overview_section(state, Kind::Watchlist, &lists.watchlist, width),
    space().height(TOKENS.spacing.s10),
    overview_section(state, Kind::Favorites, &lists.favorites, width),
    space().height(TOKENS.spacing.s10),
    overview_section(state, Kind::History, &lists.history, width),
  ]
  .padding([TOKENS.spacing.s10, TOKENS.spacing.s9])
  .width(Fill);
  scrollable(content)
    .id(iced::widget::Id::new("personal-lists-overview"))
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn count_label(page: &ListPage) -> String {
  if page.total == 0 && (page.loading || page.error.is_some()) {
    "—".to_owned()
  } else {
    page.total.to_string()
  }
}

fn overview_section<'a>(
  state: &'a State,
  kind: Kind,
  page: &'a ListPage,
  width: f32,
) -> Element<'a, Message> {
  let view_all = control_button(
    Some(Icon::ChevronRight),
    Some(state.t("lists-view-all")),
    ButtonVariant::Text,
  )
  .trailing_icon(true)
  .icon_size(IconSize::Xs)
  .label_size(12.0)
  .spacing(TOKENS.spacing.s1)
  .padding([6, 10])
  .on_press(Message::Home(HomeMessage::Navigate(
    Destination::PersonalLists(route_for(kind)),
  )));
  column![
    row![
      text(title_for(state.kernel.locale, kind))
        .font(HEADING_FONT)
        .size(20)
        .line_height(Pixels(28.0))
        .color(state.palette().text.heading),
      text(count_label(page))
        .size(12)
        .line_height(Pixels(16.0))
        .color(state.palette().text.metadata),
      space::horizontal(),
      view_all,
    ]
    .spacing(TOKENS.spacing.s2_5)
    .align_y(Alignment::Center),
    list_body(state, kind, page, width, true),
  ]
  .spacing(if kind == Kind::Favorites {
    TOKENS.spacing.s4
  } else {
    TOKENS.spacing.s3
  })
  .width(Fill)
  .into()
}

fn list_page(state: &State, kind: Kind, width: f32) -> Element<'_, Message> {
  let page = page_for(state, kind);
  let back = control_button(
    Some(Icon::ChevronLeft),
    Some(state.t("lists-title")),
    ButtonVariant::Text,
  )
  .icon_size(IconSize::Sm)
  .label_size(12.0)
  .min_height(40.0)
  .padding([6, 10])
  .on_press(Message::Home(HomeMessage::Navigate(
    Destination::PersonalLists(Route::Overview),
  )));
  let content = column![
    back,
    page_heading(
      state.palette(),
      title_for(state.kernel.locale, kind),
      state.format("lists-item-count", &[("count", count_label(page).into())]),
    ),
    list_body(state, kind, page, width, false),
    pagination(state.kernel.locale, kind, page),
  ]
  .spacing(TOKENS.spacing.s8)
  .padding([TOKENS.spacing.s10, TOKENS.spacing.s9])
  .width(Fill);
  scrollable(content)
    .id(iced::widget::Id::from(format!(
      "personal-list-{kind:?}-{}",
      page.offset
    )))
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn page_heading(
  palette: &'static ThemePalette,
  title: impl Into<String>,
  subtitle: impl Into<String>,
) -> Element<'static, Message> {
  column![
    text(title.into())
      .font(DISPLAY_FONT)
      .size(28)
      .line_height(Pixels(36.0))
      .color(palette.text.heading),
    text(subtitle.into())
      .size(12)
      .line_height(Pixels(16.0))
      .color(palette.text.metadata),
  ]
  .spacing(TOKENS.spacing.s2)
  .into()
}

fn frame_size(kind: Kind, width: f32, shelf: bool) -> (usize, f32, f32) {
  let (reference_columns, preferred) = match kind {
    Kind::Favorites => (6, POSTER_WIDTH),
    Kind::Watchlist | Kind::History => (4, LANDSCAPE_WIDTH),
  };
  let gap = TOKENS.spacing.s5;
  let columns = if shelf {
    reference_columns
  } else {
    ((width + gap) / (preferred + gap)).floor().max(1.0) as usize
  };
  let card_width = ((width - gap * (columns - 1) as f32) / columns as f32).max(if shelf {
    preferred
  } else {
    1.0
  });
  let height = match kind {
    Kind::Favorites => card_width * 1.5,
    Kind::Watchlist | Kind::History => card_width * 9.0 / 16.0,
  };
  (columns, card_width, height)
}

fn list_body<'a>(
  state: &'a State,
  kind: Kind,
  page: &'a ListPage,
  width: f32,
  shelf: bool,
) -> Element<'a, Message> {
  let (columns, card_width, art_height) = frame_size(kind, width, shelf);
  let mut body = Column::new().spacing(TOKENS.spacing.s4).width(Fill);
  if page.loading && !page.entries.is_empty() {
    body = body.push(
      text(state.t("common-loading"))
        .size(12)
        .color(state.palette().text.metadata),
    );
  }
  if let Some(error) = &page.error {
    body = body.push(failure_surface(
      state.palette(),
      state.kernel.locale,
      kind,
      error,
    ));
  }
  if page.entries.is_empty() && !page.loading {
    if page.error.is_none() {
      body = body.push(empty_surface(state.palette(), state.kernel.locale, kind));
    }
    return body.into();
  }
  let mut grid = Column::new().spacing(TOKENS.spacing.s8).width(Fill);
  let rows = if shelf {
    1
  } else {
    page.entries.len().max(columns).div_ceil(columns)
  };
  for row_index in 0..rows {
    let mut cards = Row::new().spacing(TOKENS.spacing.s5);
    if page.entries.is_empty() {
      for _ in 0..columns {
        cards = cards.push(skeleton_card(state, card_width, art_height));
      }
    } else {
      for entry in page.entries.iter().skip(row_index * columns).take(columns) {
        cards = cards.push(list_card(state, kind, entry, card_width, art_height, shelf));
      }
    }
    if shelf {
      return body
        .push(
          scrollable(cards)
            .id(iced::widget::Id::from(format!("personal-shelf-{kind:?}")))
            .direction(scrollable::Direction::Horizontal(
              scrollable::Scrollbar::new(),
            ))
            .width(Fill)
            // The floating scrollbar needs its own gutter below both text lines.
            .height(art_height + COPY_HEIGHT + TOKENS.spacing.s4)
            .style(jellypilot_ui::theme::scrollable),
        )
        .into();
    }
    grid = grid.push(cards);
  }
  body.push(grid).into()
}

fn list_card<'a>(
  state: &'a State,
  kind: Kind,
  entry: &'a ListEntry,
  width: f32,
  art_height: f32,
  shelf: bool,
) -> Element<'a, Message> {
  let progress = entry.item.as_ref().and_then(item_progress).map(|value| {
    (
      value,
      jellypilot_ui::widgets::library::artwork_progress_style(
        &jellypilot_ui::theme::theme(state.theme_mode()),
        kind == Kind::Favorites,
      ),
    )
  });
  let artwork = control_button_content(
    move |_| {
      let mut art = Stack::new().push(list_artwork(state, kind, entry, art_height));
      if let Some((progress, style)) = progress {
        let handle = state
          .full
          .as_ref()
          .expect("FullUi required")
          .personal_lists
          .artwork
          .get(&artwork_key(kind, &entry.id))
          .and_then(|cell| cell.handle())
          .cloned();
        art = art.push(
          container(ArtworkProgress::new(
            progress,
            art_height,
            4.0,
            full_radius(TOKENS.radii.xl),
            handle,
            style,
          ))
          .width(Fill)
          .height(Fill)
          .align_y(Alignment::End),
        );
      }
      art
        .push(
          container(space().width(Fill).height(Fill))
            .width(Fill)
            .height(Fill)
            .style(|theme| jellypilot_ui::widgets::library::artwork_outline(theme, false)),
        )
        .into()
    },
    ButtonVariant::Text,
  )
  .padding(0)
  .width(Length::Fixed(width))
  .min_height(art_height)
  .overlay_border()
  .style(jellypilot_ui::widgets::button::media_artwork)
  .id(iced::widget::Id::from(format!(
    "personal-open-{}",
    artwork_key(kind, &entry.id)
  )))
  .on_press_maybe(
    entry
      .item
      .as_ref()
      .map(|item| Message::OpenDetail(Box::new(item.clone()))),
  );
  let artwork = focus_tooltip(artwork, entry.name.clone(), TooltipOptions::default());
  let mut layers = Stack::new().width(width).height(art_height).push(artwork);
  if let Some(action) = card_action(state, kind, entry) {
    layers = layers.push(
      container(action)
        .padding(TOKENS.spacing.s2)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::Start),
    );
  }
  if kind == Kind::Favorites {
    if let Some(rating) = entry
      .item
      .as_ref()
      .and_then(|item| item.community_rating)
      .filter(|rating| rating.is_finite() && (0.0..=10.0).contains(rating))
    {
      layers = layers.push(
        container(
          container(
            row![
              text("★").size(12).line_height(Pixels(14.0)).color(
                jellypilot_ui::tokens::DARK_PALETTE
                  .colors
                  .onWarningContainer
              ),
              text(format!("{rating:.1}"))
                .font(HEADING_FONT)
                .size(12)
                .line_height(Pixels(14.0)),
            ]
            .spacing(TOKENS.spacing.s1),
          )
          .padding([4, 8])
          .style(jellypilot_ui::widgets::library::rating),
        )
        .padding(TOKENS.spacing.s2)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Start)
        .align_y(Alignment::End),
      );
    }
  }
  let mut artwork: Element<'a, Message> = layers.into();
  if let Some(spec) = artwork_spec(kind, entry) {
    artwork = observe_image(
      artwork,
      ArtworkSurface::PersonalLists,
      state
        .full
        .as_ref()
        .expect("FullUi required")
        .personal_lists
        .artwork
        .epoch(),
      spec,
      if shelf {
        ImageAxis::Horizontal
      } else {
        ImageAxis::Vertical
      },
    );
  }
  let title = if entry.availability == ItemAvailability::Unavailable {
    state.format(
      "lists-unavailable-item",
      &[("name", entry.name.as_str().into())],
    )
  } else {
    entry.name.clone()
  };
  let subtitle = if kind == Kind::History {
    let played = crate::i18n::media::history_timestamp(
      state.kernel.locale,
      entry
        .item
        .as_ref()
        .and_then(|item| item.last_played_date.as_deref()),
    );
    format!("{} · {played}", entry.subtitle.text(state.kernel.locale))
  } else {
    entry.subtitle.text(state.kernel.locale)
  };
  column![
    artwork,
    column![
      ellipsis_text(title)
        .font(HEADING_FONT)
        .size(14)
        .line_height(Pixels(18.0))
        .color(state.palette().text.heading),
      ellipsis_text(subtitle)
        .size(12)
        .line_height(Pixels(16.0))
        .color(state.palette().text.metadata),
    ]
    .spacing(TOKENS.spacing.s1)
    .padding(iced::Padding {
      top: TOKENS.spacing.s3,
      ..iced::Padding::ZERO
    })
    .height(COPY_HEIGHT)
    .width(Fill),
  ]
  .width(width)
  .into()
}

fn item_progress(item: &jellypilot_media_server::VideoLibraryItem) -> Option<f64> {
  if let Some(percentage) = item.played_percentage.filter(|value| value.is_finite()) {
    return Some(percentage.clamp(0.0, 100.0));
  }
  match (item.resume_position_seconds, item.runtime_seconds) {
    (Some(position), Some(runtime))
      if position.is_finite() && position > 0.0 && runtime.is_finite() && runtime > 0.0 =>
    {
      Some((position / runtime * 100.0).clamp(0.0, 100.0))
    }
    _ => None,
  }
}

fn card_action<'a>(
  state: &'a State,
  kind: Kind,
  entry: &'a ListEntry,
) -> Option<Element<'a, Message>> {
  let full = state.full.as_ref().expect("FullUi required");
  let (icon, action) = match kind {
    Kind::Favorites => {
      let item = entry.item.as_ref()?;
      (
        Icon::HeartFilled,
        Message::Collections(CollectionMessage::Favorite {
          session: state.kernel.request_gate.current_session(),
          item_id: item.id.clone(),
          favorite: false,
        }),
      )
    }
    Kind::Watchlist => (
      Icon::Trash,
      Message::PersonalLists(PersonalListsMessage::RemoveWatchlist(entry.id.clone())),
    ),
    Kind::History => return None,
  };
  let busy = collections::busy(full, &entry.id);
  let disabled = busy || crate::app::accounts::content_mutations_blocked(&state.accounts);
  let control = control_button(Some(icon), None, ButtonVariant::Tonal)
    .icon_size(IconSize::Sm)
    .padding(12)
    .width(Length::Fixed(40.0))
    .min_height(40.0)
    .id(iced::widget::Id::from(format!(
      "personal-remove-{}",
      artwork_key(kind, &entry.id)
    )))
    .on_press_maybe((!disabled).then_some(action));
  Some(focus_tooltip(
    control,
    if busy {
      state.t("lists-removing")
    } else {
      state.t("lists-remove")
    },
    TooltipOptions::default(),
  ))
}

fn list_artwork<'a>(
  state: &'a State,
  kind: Kind,
  entry: &'a ListEntry,
  height: f32,
) -> Element<'a, Message> {
  let cell = state
    .full
    .as_ref()
    .expect("FullUi required")
    .personal_lists
    .artwork
    .get(&artwork_key(kind, &entry.id));
  if let Some(cell) = cell {
    if cell.state == ImageStatus::Ready {
      if let Some(handle) = cell.handle() {
        return rounded_image(handle.clone(), full_radius(TOKENS.radii.xl))
          .content_fit(ContentFit::Cover)
          .width(Fill)
          .height(height)
          .into();
      }
    }
    if cell.state == ImageStatus::Loading {
      return skeleton_panel(
        Fill,
        height,
        state.palette().colors.surfaceContainerLowest,
        full_radius(TOKENS.radii.xl),
        state.shell.skeleton_phase,
        state.kernel.settings.snapshot().reduced_motion(),
      )
      .into();
    }
  }
  let unavailable = entry.availability == ItemAvailability::Unavailable;
  container(icon_with_color(
    if unavailable {
      Icon::Warning
    } else {
      Icon::Movie
    },
    IconSize::Custom(36.0),
    if unavailable {
      state.palette().colors.warning
    } else {
      state.palette().text.metadata
    },
  ))
  .width(Fill)
  .height(height)
  .center_x(Fill)
  .center_y(Fill)
  .style(jellypilot_ui::widgets::library::poster_placeholder)
  .into()
}

fn skeleton_card(state: &State, width: f32, height: f32) -> Element<'_, Message> {
  let base = state.palette().colors.surfaceContainerLowest;
  let phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  column![
    skeleton_panel(
      width,
      height,
      base,
      full_radius(TOKENS.radii.xl),
      phase,
      reduced_motion
    ),
    column![
      skeleton_panel(
        width,
        18.0,
        base,
        full_radius(TOKENS.radii.sm),
        phase,
        reduced_motion
      ),
      skeleton_panel(
        width * 0.7,
        16.0,
        base,
        full_radius(TOKENS.radii.sm),
        phase,
        reduced_motion
      ),
    ]
    .spacing(TOKENS.spacing.s1)
    .padding(iced::Padding {
      top: TOKENS.spacing.s3,
      ..iced::Padding::ZERO
    })
    .height(COPY_HEIGHT),
  ]
  .width(width)
  .into()
}

fn empty_surface<'a>(
  palette: &'static ThemePalette,
  locale: Localizer,
  kind: Kind,
) -> Element<'a, Message> {
  let (label, icon) = match kind {
    Kind::Favorites => ("lists-favorites-empty", Icon::Heart),
    Kind::Watchlist => ("lists-watchlist-empty", Icon::Bookmark),
    Kind::History => ("lists-history-empty", Icon::Playlist),
  };
  container(
    column![
      icon_with_color(icon, IconSize::Xl, palette.text.metadata),
      text(locale.text(label))
        .size(14)
        .color(palette.text.secondary),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_x(Alignment::Center),
  )
  .padding(TOKENS.spacing.s6)
  .width(Fill)
  .center_x(Fill)
  .into()
}

fn failure_surface<'a>(
  palette: &'static ThemePalette,
  locale: Localizer,
  kind: Kind,
  error: &'a UiText,
) -> Element<'a, Message> {
  let retry = control_button(
    Some(Icon::Refresh),
    Some(locale.text("lists-retry")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .label_size(12.0)
  .padding([6, 12])
  .min_height(40.0)
  .on_press(Message::PersonalLists(PersonalListsMessage::Retry(kind)));
  column![
    text(locale.text("lists-load-heading"))
      .font(HEADING_FONT)
      .size(15)
      .color(palette.text.heading),
    text(locale.message(error))
      .size(13)
      .color(palette.colors.error),
    retry,
  ]
  .spacing(TOKENS.spacing.s2)
  .width(Fill)
  .into()
}

fn pagination(locale: Localizer, kind: Kind, page: &ListPage) -> Element<'_, Message> {
  let previous = control_button(
    Some(Icon::ChevronLeft),
    Some(locale.text("lists-previous")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .label_size(12.0)
  .padding([6, 10])
  .min_height(40.0)
  .on_press_maybe(
    (page.offset > 0 && !page.loading).then_some(Message::PersonalLists(
      PersonalListsMessage::PreviousPage(kind),
    )),
  );
  let next = control_button(
    Some(Icon::ChevronRight),
    Some(locale.text("lists-next")),
    ButtonVariant::Tonal,
  )
  .trailing_icon(true)
  .icon_size(IconSize::Sm)
  .label_size(12.0)
  .padding([6, 10])
  .min_height(40.0)
  .on_press_maybe(
    (page.offset.saturating_add(PAGE_SIZE) < page.total && !page.loading)
      .then_some(Message::PersonalLists(PersonalListsMessage::NextPage(kind))),
  );
  row![previous, space::horizontal(), next]
    .align_y(Alignment::Center)
    .width(Fill)
    .into()
}

fn route_for(kind: Kind) -> Route {
  match kind {
    Kind::Favorites => Route::Favorites,
    Kind::Watchlist => Route::Watchlist,
    Kind::History => Route::History,
  }
}

fn page_for(state: &State, kind: Kind) -> &ListPage {
  let lists = &state.full.as_ref().expect("FullUi required").personal_lists;
  match kind {
    Kind::Favorites => &lists.favorites,
    Kind::Watchlist => &lists.watchlist,
    Kind::History => &lists.history,
  }
}

fn title_for(locale: Localizer, kind: Kind) -> String {
  locale.text(match kind {
    Kind::Favorites => "lists-favorites",
    Kind::Watchlist => "lists-watchlist",
    Kind::History => "lists-history",
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::advanced::{renderer::Headless, widget};
  use iced_runtime::user_interface::{Cache, UserInterface};

  fn entry() -> ListEntry {
    crate::app::personal_lists::entry_from_item(jellypilot_media_server::VideoLibraryItem {
      id: "movie".to_owned(),
      name: "A movie with a long localized title that must stay inside its card".to_owned(),
      item_type: "Movie".to_owned(),
      production_year: Some(2026),
      premiere_date: None,
      runtime_seconds: None,
      community_rating: None,
      episode_count: None,
      last_played_date: None,
      played: false,
      favorite: true,
      artwork_image_id: None,
      backdrop_image_id: None,
      logo_image_id: None,
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
      series_continuing: false,
      unplayed_item_count: None,
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
    })
  }

  #[tokio::test]
  async fn card_layout_keeps_landscape_and_poster_copy_outside_action_bounds() {
    let state = State::boot(false);
    let entry = entry();
    let renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for (kind, expected_width, expected_height) in [
      (Kind::Watchlist, 272.0, 203.0),
      (Kind::Favorites, 174.66667, 312.0),
      (Kind::History, 272.0, 203.0),
    ] {
      let (_, width, height) = frame_size(kind, 1148.0, true);
      let mut card = list_card(&state, kind, &entry, width, height, true);
      let mut tree = widget::Tree::new(card.as_widget());
      tree.diff(card.as_widget_mut());
      let node = card.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &iced::advanced::layout::Limits::new(iced::Size::ZERO, iced::Size::new(1148.0, 500.0)),
      );
      assert!(
        (node.size().width - expected_width).abs() < 0.01,
        "{kind:?}"
      );
      assert!(
        (node.size().height - expected_height).abs() < 0.01,
        "{kind:?}"
      );
    }
  }

  #[tokio::test]
  async fn overflowing_shelves_keep_scrollbar_hits_below_card_captions() {
    let state = State::boot(false);
    let page = ListPage {
      entries: (0..6)
        .map(|index| {
          let mut entry = entry();
          entry.id = format!("movie-{index}");
          entry.item.as_mut().unwrap().id.clone_from(&entry.id);
          entry
        })
        .collect(),
      ..ListPage::default()
    };
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for kind in [Kind::Watchlist, Kind::Favorites, Kind::History] {
      let (_, _, art_height) = frame_size(kind, 700.0, true);
      let mut ui = UserInterface::build(
        list_body(&state, kind, &page, 700.0, true),
        iced::Size::new(700.0, 500.0),
        Cache::new(),
        &mut renderer,
      );
      let mut bus = iced::advanced::shell::Bus::new();
      let (_, statuses) = ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
          iced::mouse::Button::Left,
        ))],
        iced::mouse::Cursor::Available(iced::Point::new(100.0, art_height + COPY_HEIGHT - 6.0)),
        &mut renderer,
        &mut bus,
      );
      assert_eq!(
        statuses,
        [iced::event::Status::Ignored],
        "{kind:?}: clicking caption text must not grab the scrollbar",
      );
    }
  }

  #[tokio::test]
  async fn watchlist_remove_and_detail_targets_do_not_overlap_or_capture_copy() {
    let state = State::boot(false);
    let entry = entry();
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for (position, expected) in [
      (iced::Point::new(244.0, 28.0), "remove"),
      (iced::Point::new(100.0, 80.0), "details"),
      (iced::Point::new(100.0, 180.0), "none"),
    ] {
      let mut ui = UserInterface::build(
        list_card(&state, Kind::Watchlist, &entry, 272.0, 153.0, true),
        iced::Size::new(272.0, 203.0),
        Cache::new(),
        &mut renderer,
      );
      let mut bus = iced::advanced::shell::Bus::new();
      let _ = ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[
          iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
          iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left,
          )),
        ],
        iced::mouse::Cursor::Available(position),
        &mut renderer,
        &mut bus,
      );
      let actions = bus
        .drain()
        .map(|(message, _)| match message {
          Message::OpenDetail(_) => "details",
          Message::PersonalLists(PersonalListsMessage::RemoveWatchlist(_)) => "remove",
          _ => "other",
        })
        .collect::<Vec<_>>();
      assert_eq!(
        actions,
        if expected == "none" {
          Vec::new()
        } else {
          vec![expected]
        }
      );
    }
  }

  #[tokio::test]
  async fn history_details_remain_keyboard_operable_without_a_remove_action() {
    struct Focus;
    impl widget::Operation for Focus {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn focusable(
        &mut self,
        _: Option<&widget::Id>,
        _: iced::Rectangle,
        state: &mut dyn widget::operation::Focusable,
      ) {
        state.focus();
      }
    }
    let state = State::boot(false);
    let entry = entry();
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let mut ui = UserInterface::build(
      list_card(&state, Kind::History, &entry, 272.0, 153.0, true),
      iced::Size::new(272.0, 203.0),
      Cache::new(),
      &mut renderer,
    );
    ui.operate(&renderer, &mut Focus);
    let mut bus = iced::advanced::shell::Bus::new();
    let _ = ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      &[iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
        modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::NONE,
        text: None,
        repeat: false,
      })],
      iced::mouse::Cursor::Unavailable,
      &mut renderer,
      &mut bus,
    );
    let actions = bus.drain().map(|(message, _)| message).collect::<Vec<_>>();
    assert!(matches!(actions.as_slice(), [Message::OpenDetail(item)] if item.id == entry.id));
  }
}
