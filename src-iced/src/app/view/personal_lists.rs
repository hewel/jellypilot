use iced::widget::{column, container, row, scrollable, space, text, Column, Row};
use iced::{Alignment, Color, ContentFit, Element, Fill, Length};
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::control_button;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::skeleton::skeleton_panel;
use jellypilot_ui::{full_radius, poster_card, rounded_image};

use crate::app::message::{HomeMessage, Message};
use crate::app::personal_lists::{
  ItemAvailability, Kind, ListEntry, ListPage, PersonalListsMessage, Route,
};
use crate::app::state::{ArtworkCellState, Destination, State};

const CARD_ARTWORK_HEIGHT: f32 = 220.0;
const OVERVIEW_LIMIT: usize = 6;

pub fn view(state: &State) -> Element<'_, Message> {
  let route = match state.shell.destination {
    Destination::PersonalLists(route) => route,
    _ => Route::Overview,
  };
  match route {
    Route::Overview => overview(state),
    Route::Favorites => list_page(state, Kind::Favorites),
    Route::Watchlist => list_page(state, Kind::Watchlist),
  }
}

fn overview(state: &State) -> Element<'_, Message> {
  let lists = &state.full.as_ref().expect("FullUi required").personal_lists;
  let content = column![
    page_heading(
      state.palette(),
      "Personal Lists",
      "Favorites and Watchlist are kept separate."
    ),
    overview_section(state, Kind::Favorites, &lists.favorites),
    overview_section(state, Kind::Watchlist, &lists.watchlist),
  ]
  .spacing(TOKENS.spacing.s6)
  .padding([TOKENS.spacing.s6, TOKENS.spacing.s8])
  .width(Fill);
  scrollable(content)
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn overview_section<'a>(state: &'a State, kind: Kind, page: &'a ListPage) -> Element<'a, Message> {
  let route = route_for(kind);
  let title = title_for(kind);
  let label = format!("{title} · {}", page.total);
  let view_all = control_button(
    Some(Icon::ChevronRight),
    Some("View all".to_owned()),
    ButtonVariant::Tonal,
  )
  .trailing_icon(true)
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(Message::Home(HomeMessage::Navigate(
    Destination::PersonalLists(route),
  )));
  let body = list_body(state, kind, page, OVERVIEW_LIMIT);
  column![
    row![
      text(label)
        .font(SPACE_GROTESK_FONT)
        .size(20)
        .color(state.palette().text.heading),
      space::horizontal(),
      view_all,
    ]
    .align_y(Alignment::Center),
    body,
  ]
  .spacing(TOKENS.spacing.s3)
  .width(Fill)
  .into()
}

fn list_page<'a>(state: &'a State, kind: Kind) -> Element<'a, Message> {
  let page = page_for(state, kind);
  let title = title_for(kind);
  let back = control_button(
    Some(Icon::ChevronLeft),
    Some("Personal Lists".to_owned()),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(Message::Home(HomeMessage::Navigate(
    Destination::PersonalLists(Route::Overview),
  )));
  let content = column![
    back,
    page_heading(
      state.palette(),
      title,
      format!("{} saved items", page.total),
    ),
    list_body(state, kind, page, usize::MAX),
    pagination(kind, page),
  ]
  .spacing(TOKENS.spacing.s4)
  .padding([TOKENS.spacing.s5, TOKENS.spacing.s8])
  .width(Fill);
  scrollable(content)
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
      .font(SPACE_GROTESK_FONT)
      .size(28)
      .color(palette.text.heading),
    text(subtitle.into()).size(13).color(palette.text.metadata),
  ]
  .spacing(TOKENS.spacing.s1)
  .into()
}

fn list_body<'a>(
  state: &'a State,
  kind: Kind,
  page: &'a ListPage,
  limit: usize,
) -> Element<'a, Message> {
  if page.loading && page.entries.is_empty() {
    return skeleton_grid(state).into();
  }
  if let Some(error) = &page.error {
    return failure_surface(state.palette(), kind, error).into();
  }
  if page.entries.is_empty() {
    return empty_surface(state.palette(), kind).into();
  }

  let entries = page.entries.iter().take(limit).collect::<Vec<_>>();
  let mut grid = Column::new().spacing(TOKENS.spacing.s5).width(Fill);
  for row_entries in entries.chunks(4) {
    let mut card_row = Row::new().spacing(TOKENS.spacing.s4).width(Fill);
    for entry in row_entries {
      card_row = card_row.push(
        container(list_card(state, kind, entry))
          .width(Length::FillPortion(1))
          .height(Length::Shrink),
      );
    }
    grid = grid.push(card_row);
  }
  grid.into()
}

fn list_card<'a>(state: &'a State, kind: Kind, entry: &'a ListEntry) -> Element<'a, Message> {
  let artwork = list_artwork(state, entry);
  let unavailable = entry.availability == ItemAvailability::Unavailable;
  let title = if unavailable {
    format!("{} · Unavailable", entry.name)
  } else {
    entry.name.clone()
  };
  let copy = column![
    ellipsis_text(title)
      .size(14)
      .color(state.palette().text.heading),
    ellipsis_text(&entry.subtitle)
      .size(12)
      .color(state.palette().text.metadata),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let card: Element<'_, Message> = if let Some(item) = &entry.item {
    poster_card(artwork, copy)
      .width(Fill)
      .on_press(Message::OpenDetail(item.clone()))
      .into()
  } else {
    column![artwork, copy]
      .spacing(TOKENS.spacing.s2)
      .width(Fill)
      .into()
  };
  let remove = match (kind, &entry.item) {
    (Kind::Favorites, Some(item)) => {
      Message::PersonalLists(PersonalListsMessage::RemoveFavorite(item.clone()))
    }
    (Kind::Favorites, None) => {
      return card;
    }
    (Kind::Watchlist, _) => {
      Message::PersonalLists(PersonalListsMessage::RemoveWatchlist(entry.id.clone()))
    }
  };
  let busy = state
    .full
    .as_ref()
    .expect("FullUi required")
    .personal_lists
    .busy_items
    .contains(&entry.id);
  let remove_button = control_button(
    Some(Icon::Trash),
    Some(if busy { "Removing…" } else { "Remove" }.to_owned()),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Xs)
  .spacing(TOKENS.spacing.s1_5)
  .padding([5, 9])
  .on_press_maybe((!busy).then_some(remove));
  column![card, remove_button]
    .spacing(TOKENS.spacing.s2)
    .width(Fill)
    .into()
}

fn list_artwork<'a>(state: &'a State, entry: &'a ListEntry) -> Element<'a, Message> {
  let cell = state
    .full
    .as_ref()
    .expect("FullUi required")
    .personal_lists
    .artwork
    .get(&entry.id);
  if let Some(cell) = cell {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.kernel.artwork_handles.get(cell.slot, &cell.image_id) {
        return rounded_image(handle.clone(), full_radius(TOKENS.radii.lg))
          .content_fit(ContentFit::Cover)
          .width(Fill)
          .height(CARD_ARTWORK_HEIGHT)
          .into();
      }
    }
  }
  let color = match cell.map(|cell| cell.state) {
    Some(ArtworkCellState::Failed) => state.palette().colors.warning,
    None if entry.availability == ItemAvailability::Unavailable => state.palette().colors.warning,
    _ => state.palette().text.metadata,
  };
  let icon = if entry.availability == ItemAvailability::Unavailable {
    Icon::Warning
  } else {
    Icon::Movie
  };
  container(icon_with_color(icon, IconSize::Custom(36.0), color))
    .width(Fill)
    .height(CARD_ARTWORK_HEIGHT)
    .center_x(Fill)
    .center_y(Fill)
    .style(move |_theme| container::Style {
      background: Some(iced::Background::Color(
        state.palette().colors.surfaceContainerLowest,
      )),
      border: iced::Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: full_radius(TOKENS.radii.lg),
      },
      ..container::Style::default()
    })
    .into()
}

fn skeleton_grid<'a>(state: &'a State) -> Column<'a, Message> {
  let base = state.palette().colors.surfaceContainerLowest;
  let phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  let mut row_cells = Row::new().spacing(TOKENS.spacing.s4).width(Fill);
  for _ in 0..4 {
    row_cells = row_cells.push(
      column![
        skeleton_panel(
          Fill,
          CARD_ARTWORK_HEIGHT,
          base,
          full_radius(TOKENS.radii.lg),
          phase,
          reduced_motion
        ),
        skeleton_panel(
          Fill,
          16.0,
          base,
          full_radius(TOKENS.radii.sm),
          phase,
          reduced_motion
        ),
        skeleton_panel(
          Fill,
          14.0,
          base,
          full_radius(TOKENS.radii.sm),
          phase,
          reduced_motion
        ),
      ]
      .spacing(TOKENS.spacing.s2)
      .width(Length::FillPortion(1)),
    );
  }
  column![row_cells].width(Fill)
}

fn empty_surface<'a>(
  palette: &'static ThemePalette,
  kind: Kind,
) -> iced::widget::Container<'a, Message> {
  let label = match kind {
    Kind::Favorites => "No favorites yet.",
    Kind::Watchlist => "Your Watchlist is empty.",
  };
  let icon = match kind {
    Kind::Favorites => Icon::Heart,
    Kind::Watchlist => Icon::Bookmark,
  };
  container(
    column![
      icon_with_color(icon, IconSize::Xl, palette.text.metadata),
      text(label).size(14).color(palette.text.secondary),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_x(Alignment::Center),
  )
  .padding(TOKENS.spacing.s6)
  .width(Fill)
  .center_x(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
}

fn failure_surface<'a>(
  palette: &'static ThemePalette,
  kind: Kind,
  error: &'a str,
) -> iced::widget::Container<'a, Message> {
  let retry = control_button(
    Some(Icon::Refresh),
    Some("Retry".to_owned()),
    ButtonVariant::Primary,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 12])
  .on_press(Message::PersonalLists(PersonalListsMessage::Retry(kind)));
  container(
    column![
      text("Could not load this list")
        .size(15)
        .color(palette.text.heading),
      text(error).size(13).color(palette.colors.error),
      retry,
    ]
    .spacing(TOKENS.spacing.s2),
  )
  .padding(TOKENS.spacing.s5)
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
}

fn pagination<'a>(kind: Kind, page: &'a ListPage) -> Element<'a, Message> {
  let previous = control_button(
    Some(Icon::ChevronLeft),
    Some("Previous".to_owned()),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(
    (page.offset > 0 && !page.loading).then_some(Message::PersonalLists(
      PersonalListsMessage::PreviousPage(kind),
    )),
  );
  let has_next = page.offset.saturating_add(page.entries.len()) < page.total;
  let next = control_button(
    Some(Icon::ChevronRight),
    Some("Next".to_owned()),
    ButtonVariant::Tonal,
  )
  .trailing_icon(true)
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(
    (has_next && !page.loading)
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
  }
}

fn page_for(state: &State, kind: Kind) -> &ListPage {
  let lists = &state.full.as_ref().expect("FullUi required").personal_lists;
  match kind {
    Kind::Favorites => &lists.favorites,
    Kind::Watchlist => &lists.watchlist,
  }
}

fn title_for(kind: Kind) -> &'static str {
  match kind {
    Kind::Favorites => "Favorites",
    Kind::Watchlist => "Watchlist",
  }
}
