use std::collections::HashMap;
use std::sync::Arc;

use jellypilot_media_server::{
  JellyfinClient, VideoLibraryItem, VideoLibraryPageRequest, VideoLibraryPlayedFilter,
  VideoLibraryShortcut, VideoLibrarySort, VideoLibrarySortDirection, VideoSearchRequest,
};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, Sender};

use crate::artwork::DecodedArtwork;
use crate::artwork_binder::{ArtworkBinder, ArtworkSlot, ArtworkSurface};
use crate::browse_model::{
  BrowseModel, BrowsePagePayload, BrowsePageRequest, BrowsePageSettlement, BrowsePreferences,
  BrowseSource,
};
use crate::library_browse::LibraryBrowseView;
use crate::pages::cards::{
  apply_decoded_artwork, clear_box, dim_label, library_kind, loading_view, poster_card,
  register_artwork, row_card, state_view, ArtworkTarget,
};
use crate::playback::{Playable, PlaybackStartPosition};
use crate::request_gate::RequestGate;
use crate::shell::AppMessage;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum BrowsePresentation {
  #[default]
  Grid,
  List,
}

pub(crate) struct BrowsePage {
  root: gtk::ScrolledWindow,
  sender: Sender<AppMessage>,
  title: gtk::Label,
  status: gtk::Label,
  content: gtk::Box,
  filter_bar: gtk::Box,
  sort_dropdown: gtk::DropDown,
  played_dropdown: gtk::DropDown,
  favorites_only: gtk::CheckButton,
  grid_button: gtk::ToggleButton,
  list_button: gtk::ToggleButton,
  load_previous_button: gtk::Button,
  load_next_button: gtk::Button,
  browse_title: String,
  model: BrowseModel,
  error: Option<String>,
  presentation: BrowsePresentation,
  library_shortcut: Option<VideoLibraryShortcut>,
  sort_selection: u32,
  played_selection: u32,
  favorites_only_value: bool,
  artwork_targets: HashMap<ArtworkSlot, ArtworkTarget>,
}

pub(crate) struct BrowseContext<'a> {
  pub gate: &'a mut RequestGate,
  pub binder: &'a mut ArtworkBinder,
  pub playback_enabled: bool,
}

#[derive(Debug)]
pub(crate) enum Message {
  OpenLibrary(VideoLibraryShortcut),
  Search(String),
  SetPresentation(BrowsePresentation),
  SetSort(u32),
  SetPlayedFilter(u32),
  SetFavoritesOnly(bool),
  LoadPreviousPage,
  LoadNextPage,
  Retry,
  SelectItem(VideoLibraryItem),
  Play(VideoLibraryItem, PlaybackStartPosition),
}

pub(crate) enum BrowseEvent {
  Page(BrowsePageSettlement),
}

pub(crate) enum BrowseEffect {
  BeginArtworkView,
  ArtworkLoad {
    surface: ArtworkSurface,
    slot: ArtworkSlot,
    image_id: String,
  },
  BrowsePage(BrowsePageRequest),
  OpenDetail(VideoLibraryItem),
  PlayItem(Playable, PlaybackStartPosition),
  Render,
}

impl BrowsePage {
  pub(crate) fn build(sender: &Sender<AppMessage>) -> Self {
    let browse_title = gtk::Label::new(Some("Library"));
    browse_title.add_css_class("title-2");
    browse_title.set_xalign(0.0);
    browse_title.set_hexpand(true);
    browse_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    browse_title.set_max_width_chars(48);
    let browse_status = dim_label("");
    browse_status.set_xalign(0.0);
    browse_status.set_wrap(true);
    let grid_button = gtk::ToggleButton::new();
    grid_button.set_child(Some(&gtk::Image::from_icon_name("view-grid-symbolic")));
    grid_button.set_tooltip_text(Some("Grid view"));
    grid_button.update_property(&[gtk::accessible::Property::Label("Grid view")]);
    grid_button.set_active(true);
    grid_button.set_valign(gtk::Align::Center);
    grid_button.set_size_request(36, 32);
    let list_button = gtk::ToggleButton::new();
    list_button.set_child(Some(&gtk::Image::from_icon_name("view-list-symbolic")));
    list_button.set_tooltip_text(Some("List view"));
    list_button.update_property(&[gtk::accessible::Property::Label("List view")]);
    list_button.set_group(Some(&grid_button));
    list_button.set_valign(gtk::Align::Center);
    list_button.set_size_request(36, 32);
    grid_button.connect_toggled({
      let sender = sender.clone();
      move |button| {
        if button.is_active() {
          sender.emit(AppMessage::Browse(Message::SetPresentation(
            BrowsePresentation::Grid,
          )));
        }
      }
    });
    list_button.connect_toggled({
      let sender = sender.clone();
      move |button| {
        if button.is_active() {
          sender.emit(AppMessage::Browse(Message::SetPresentation(
            BrowsePresentation::List,
          )));
        }
      }
    });
    let load_next_button = gtk::Button::with_label("Load more");
    load_next_button.set_visible(false);
    load_next_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Browse(Message::LoadNextPage))
    });
    let load_previous_button = gtk::Button::with_label("Previous page");
    load_previous_button.set_visible(false);
    load_previous_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Browse(Message::LoadPreviousPage))
    });
    let toolbar = adw::PreferencesGroup::new();
    toolbar.set_title("Browse");
    toolbar.add(&browse_title);
    toolbar.add(&browse_status);
    let browse_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    browse_actions.append(&grid_button);
    browse_actions.append(&list_button);
    toolbar.add(&browse_actions);
    let browse_filter_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let sort_label = gtk::Label::new(Some("Sort"));
    let sort_dropdown =
      gtk::DropDown::from_strings(&["Title A–Z", "Title Z–A", "Recently added", "Release date"]);
    sort_dropdown.update_property(&[gtk::accessible::Property::Label("Sort library")]);
    sort_dropdown.connect_selected_notify({
      let sender = sender.clone();
      move |dropdown| sender.emit(AppMessage::Browse(Message::SetSort(dropdown.selected())))
    });
    let played_label = gtk::Label::new(Some("Watched"));
    let played_dropdown = gtk::DropDown::from_strings(&["All", "Unwatched", "Watched"]);
    played_dropdown.update_property(&[gtk::accessible::Property::Label("Filter watched state")]);
    played_dropdown.connect_selected_notify({
      let sender = sender.clone();
      move |dropdown| {
        sender.emit(AppMessage::Browse(Message::SetPlayedFilter(
          dropdown.selected(),
        )))
      }
    });
    let favorites_only = gtk::CheckButton::with_label("Favorites only");
    favorites_only.connect_toggled({
      let sender = sender.clone();
      move |button| {
        sender.emit(AppMessage::Browse(Message::SetFavoritesOnly(
          button.is_active(),
        )))
      }
    });
    browse_filter_bar.append(&sort_label);
    browse_filter_bar.append(&sort_dropdown);
    browse_filter_bar.append(&played_label);
    browse_filter_bar.append(&played_dropdown);
    browse_filter_bar.append(&favorites_only);
    toolbar.add(&browse_filter_bar);
    let browse_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let browse_page = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(16)
      .margin_top(24)
      .margin_bottom(24)
      .margin_start(24)
      .margin_end(24)
      .build();
    browse_page.append(&toolbar);
    browse_page.append(&browse_content);
    let pagination = adw::ActionRow::new();
    pagination.set_title("Pages");
    pagination.add_suffix(&load_previous_button);
    pagination.add_suffix(&load_next_button);
    browse_page.append(&pagination);
    let browse_scroll = gtk::ScrolledWindow::builder()
      .child(&browse_page)
      .vexpand(true)
      .build();
    Self {
      root: browse_scroll,
      sender: sender.clone(),
      title: browse_title,
      status: browse_status,
      content: browse_content,
      filter_bar: browse_filter_bar,
      sort_dropdown,
      played_dropdown,
      favorites_only,
      grid_button,
      list_button,
      load_previous_button,
      load_next_button,
      browse_title: String::new(),
      model: BrowseModel::default(),
      error: None,
      presentation: BrowsePresentation::Grid,
      library_shortcut: None,
      sort_selection: 0,
      played_selection: 0,
      favorites_only_value: false,
      artwork_targets: HashMap::new(),
    }
  }

  pub(crate) fn root(&self) -> &gtk::ScrolledWindow {
    &self.root
  }

  /// Resets browse state on Disconnect.
  pub(crate) fn reset(&mut self) {
    self.model.reset();
    self.browse_title.clear();
    self.model = BrowseModel::default();
    self.error = None;
    self.presentation = BrowsePresentation::Grid;
    self.library_shortcut = None;
    self.sort_selection = 0;
    self.played_selection = 0;
    self.favorites_only_value = false;
    self.artwork_targets.clear();
    self.sort_dropdown.set_selected(0);
    self.played_dropdown.set_selected(0);
    self.favorites_only.set_active(false);
  }

  pub(crate) fn handle(
    &mut self,
    message: Message,
    cx: &mut BrowseContext<'_>,
  ) -> Vec<BrowseEffect> {
    match message {
      Message::OpenLibrary(shortcut) => self.open_library(shortcut, cx),
      Message::Search(query) => self.start_search(query, cx),
      Message::SetPresentation(presentation) => {
        self.presentation = presentation;
        vec![BrowseEffect::Render]
      }
      Message::SetSort(selection) => {
        self.sort_selection = selection;
        self.apply_preferences(cx)
      }
      Message::SetPlayedFilter(selection) => {
        self.played_selection = selection;
        self.apply_preferences(cx)
      }
      Message::SetFavoritesOnly(favorites_only) => {
        self.favorites_only_value = favorites_only;
        self.apply_preferences(cx)
      }
      Message::LoadPreviousPage => self.load_previous_page(),
      Message::LoadNextPage => self.load_next_page(),
      Message::Retry => self.retry(),
      Message::SelectItem(item) => vec![BrowseEffect::OpenDetail(item)],
      Message::Play(item, position) => vec![BrowseEffect::PlayItem(Playable::from(item), position)],
    }
  }

  pub(crate) fn handle_event(
    &mut self,
    event: BrowseEvent,
    _cx: &mut BrowseContext<'_>,
  ) -> Vec<BrowseEffect> {
    match event {
      BrowseEvent::Page(settlement) => {
        let mut effects = match self.model.settle(settlement) {
          Ok(model_effects) => {
            self.error = None;
            self.model_effects(model_effects)
          }
          Err(error) => {
            self.error = Some(error.to_string());
            Vec::new()
          }
        };
        effects.push(BrowseEffect::Render);
        effects
      }
    }
  }

  pub(crate) fn render(&mut self, cx: &mut BrowseContext<'_>) -> Vec<BrowseEffect> {
    cx.binder.begin_view(ArtworkSurface::Browse);
    self.artwork_targets.clear();
    let mut effects = vec![BrowseEffect::BeginArtworkView];
    self.title.set_label(&self.browse_title);
    self.filter_bar.set_visible(self.library_shortcut.is_some());
    self.sort_dropdown.set_selected(self.sort_selection);
    self.played_dropdown.set_selected(self.played_selection);
    self.favorites_only.set_active(self.favorites_only_value);
    self
      .grid_button
      .set_active(matches!(self.presentation, BrowsePresentation::Grid));
    self
      .list_button
      .set_active(matches!(self.presentation, BrowsePresentation::List));
    clear_box(&self.content);
    self.status.set_label("");
    self.load_previous_button.set_visible(false);
    self.load_previous_button.set_sensitive(true);
    self.load_next_button.set_visible(false);
    self.load_next_button.set_sensitive(true);
    if let Some(message) = &self.error {
      self.content.append(&state_view(
        "Items could not load",
        message,
        "dialog-error-symbolic",
      ));
      return effects;
    }

    match self.model.view() {
      LibraryBrowseView::Inactive => self.content.append(&state_view(
        "Choose a library",
        "Select Movies or Shows from the sidebar.",
        "folder-videos-symbolic",
      )),
      LibraryBrowseView::Loading => self.content.append(&loading_view("Loading items…")),
      LibraryBrowseView::Empty => self.content.append(&state_view(
        "No matching items",
        "Try a different library or search term.",
        "edit-find-symbolic",
      )),
      LibraryBrowseView::Failed {
        message,
        retryable,
        retry_busy,
      } => {
        self.status.set_label(&message);
        self.content.append(&state_view(
          "Items could not load",
          &message,
          "dialog-error-symbolic",
        ));
        if retryable {
          let retry = gtk::Button::with_label("Retry");
          retry.set_sensitive(!retry_busy);
          let sender = self.sender.clone();
          retry.connect_clicked(move |_| sender.emit(AppMessage::Browse(Message::Retry)));
          self.content.append(&retry);
        }
      }
      LibraryBrowseView::Ready {
        visible_items,
        total_record_count,
        is_fetching_more,
        load_more_failure,
        retry_busy,
        ..
      } => {
        let display_range = self.model.display_range();
        let items: Vec<_> = visible_items
          .into_iter()
          .filter_map(|slot| slot.item)
          .collect();
        self.render_media_results(&items, total_record_count, cx, &mut effects);
        if let Some(range) = display_range {
          self.status.set_label(&format!(
            "Items {}–{} of {total_record_count}",
            range.start.saturating_add(1),
            range.end
          ));
        }
        if is_fetching_more {
          self.content.append(&loading_view("Loading more items…"));
        }
        if let Some(message) = load_more_failure {
          self.content.append(&state_view(
            "More items could not load",
            &message,
            "dialog-warning-symbolic",
          ));
          let retry = gtk::Button::with_label("Retry loading more");
          retry.set_sensitive(!retry_busy);
          let sender = self.sender.clone();
          retry.connect_clicked(move |_| sender.emit(AppMessage::Browse(Message::Retry)));
          self.content.append(&retry);
        } else {
          self
            .load_previous_button
            .set_visible(self.model.can_load_previous());
          self.load_previous_button.set_sensitive(!is_fetching_more);
          self
            .load_next_button
            .set_visible(self.model.can_load_more());
          self.load_next_button.set_sensitive(!is_fetching_more);
        }
      }
    }
    effects
  }

  pub(crate) fn apply_artwork(&mut self, slot: ArtworkSlot, decoded: DecodedArtwork) -> bool {
    apply_decoded_artwork(&mut self.artwork_targets, slot, decoded)
  }

  fn open_library(
    &mut self,
    shortcut: VideoLibraryShortcut,
    cx: &mut BrowseContext<'_>,
  ) -> Vec<BrowseEffect> {
    self.browse_title = shortcut.name.clone();
    self.error = None;
    self.library_shortcut = Some(shortcut.clone());
    let result = self.model.configure_with_preferences(
      BrowseSource::Library {
        session: cx.gate.current_session(),
        shortcut,
      },
      browse_preferences(
        self.sort_selection,
        self.played_selection,
        self.favorites_only_value,
      ),
    );
    let mut effects = match result {
      Ok(model_effects) => self.model_effects(model_effects),
      Err(error) => {
        self.error = Some(error.to_string());
        Vec::new()
      }
    };
    effects.push(BrowseEffect::Render);
    effects
  }

  fn start_search(&mut self, query: String, cx: &mut BrowseContext<'_>) -> Vec<BrowseEffect> {
    self.browse_title = format!("Search results for \"{query}\"");
    self.error = None;
    self.library_shortcut = None;
    let result = self.model.configure(BrowseSource::Search {
      session: cx.gate.current_session(),
      query,
    });
    let mut effects = match result {
      Ok(model_effects) => self.model_effects(model_effects),
      Err(error) => {
        self.error = Some(error.to_string());
        Vec::new()
      }
    };
    effects.push(BrowseEffect::Render);
    effects
  }

  fn apply_preferences(&mut self, cx: &mut BrowseContext<'_>) -> Vec<BrowseEffect> {
    let Some(shortcut) = self.library_shortcut.clone() else {
      return Vec::new();
    };
    self.error = None;
    let result = self.model.configure_with_preferences(
      BrowseSource::Library {
        session: cx.gate.current_session(),
        shortcut,
      },
      browse_preferences(
        self.sort_selection,
        self.played_selection,
        self.favorites_only_value,
      ),
    );
    let mut effects = match result {
      Ok(model_effects) => self.model_effects(model_effects),
      Err(error) => {
        self.error = Some(error.to_string());
        Vec::new()
      }
    };
    effects.push(BrowseEffect::Render);
    effects
  }

  fn load_next_page(&mut self) -> Vec<BrowseEffect> {
    let mut effects = match self.model.load_next() {
      Ok(model_effects) => {
        self.error = None;
        self.model_effects(model_effects)
      }
      Err(error) => {
        self.error = Some(error.to_string());
        Vec::new()
      }
    };
    effects.push(BrowseEffect::Render);
    effects
  }

  fn load_previous_page(&mut self) -> Vec<BrowseEffect> {
    let mut effects = match self.model.load_previous() {
      Ok(model_effects) => {
        self.error = None;
        self.model_effects(model_effects)
      }
      Err(error) => {
        self.error = Some(error.to_string());
        Vec::new()
      }
    };
    effects.push(BrowseEffect::Render);
    effects
  }

  fn retry(&mut self) -> Vec<BrowseEffect> {
    let mut effects = match self.model.retry() {
      Ok(model_effects) => {
        self.error = None;
        self.model_effects(model_effects)
      }
      Err(error) => {
        self.error = Some(error.to_string());
        Vec::new()
      }
    };
    effects.push(BrowseEffect::Render);
    effects
  }

  fn model_effects(&self, effects: Vec<crate::browse_model::BrowseEffect>) -> Vec<BrowseEffect> {
    let mut out = Vec::new();
    for effect in effects {
      match effect {
        crate::browse_model::BrowseEffect::ResetViewport => {
          let adjustment = self.root.vadjustment();
          adjustment.set_value(adjustment.lower());
        }
        crate::browse_model::BrowseEffect::RequestPage(request) => {
          out.push(BrowseEffect::BrowsePage(request));
        }
        crate::browse_model::BrowseEffect::CancelPage => {}
      }
    }
    out
  }

  fn render_media_results(
    &mut self,
    items: &[VideoLibraryItem],
    total: u32,
    cx: &mut BrowseContext<'_>,
    effects: &mut Vec<BrowseEffect>,
  ) {
    self.status.set_label(&format!("{total} items"));
    if items.is_empty() {
      self.content.append(&state_view(
        "No matching items",
        "Try a different library or search term.",
        "edit-find-symbolic",
      ));
      return;
    }
    let content = match self.presentation {
      BrowsePresentation::Grid => self.media_grid(items, cx, effects),
      BrowsePresentation::List => self.media_list(items, cx, effects),
    };
    self.content.append(&content);
  }

  fn media_grid(
    &mut self,
    items: &[VideoLibraryItem],
    cx: &mut BrowseContext<'_>,
    effects: &mut Vec<BrowseEffect>,
  ) -> gtk::Widget {
    let flow = gtk::FlowBox::builder()
      .selection_mode(gtk::SelectionMode::None)
      .max_children_per_line(6)
      .min_children_per_line(1)
      .row_spacing(12)
      .column_spacing(12)
      .build();
    for item in items {
      let child = gtk::FlowBoxChild::new();
      child.set_child(Some(&self.media_button(item, true, cx, effects)));
      flow.insert(&child, -1);
    }
    // [DEBUG-browse-grid] temporary layout probe; remove after diagnosis.
    gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(600), {
      let flow = flow.clone();
      move || {
        let mut widths = Vec::new();
        let mut child = flow.first_child();
        while let Some(widget) = child {
          widths.push(widget.width());
          child = widget.next_sibling();
        }
        eprintln!(
          "[DEBUG-browse-grid] flow width={} min_children=1 max_children=6 children={:?}",
          flow.width(),
          widths
        );
      }
    });
    flow.upcast()
  }

  fn media_list(
    &mut self,
    items: &[VideoLibraryItem],
    cx: &mut BrowseContext<'_>,
    effects: &mut Vec<BrowseEffect>,
  ) -> gtk::Widget {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    for item in items {
      list.append(&self.media_button(item, false, cx, effects));
    }
    list.upcast()
  }

  fn media_button(
    &mut self,
    item: &VideoLibraryItem,
    compact: bool,
    cx: &mut BrowseContext<'_>,
    effects: &mut Vec<BrowseEffect>,
  ) -> gtk::Widget {
    let sender = self.sender.clone();
    let play_sender = self.sender.clone();
    let select_item = item.clone();
    let play_item = item.clone();
    let (widget, artwork) = if compact {
      poster_card(item, move || {
        sender.emit(AppMessage::Browse(Message::SelectItem(select_item.clone())));
      })
    } else {
      row_card(
        item,
        cx.playback_enabled,
        move || {
          sender.emit(AppMessage::Browse(Message::SelectItem(select_item.clone())));
        },
        move |position| {
          play_sender.emit(AppMessage::Browse(Message::Play(
            play_item.clone(),
            position,
          )));
        },
      )
    };
    if let Some((slot, image_id)) = register_artwork(
      &mut self.artwork_targets,
      cx.binder,
      ArtworkSurface::Browse,
      artwork,
    ) {
      effects.push(BrowseEffect::ArtworkLoad {
        surface: ArtworkSurface::Browse,
        slot,
        image_id,
      });
    }
    widget
  }
}

pub(crate) async fn fetch_browse_page(
  client: Arc<JellyfinClient>,
  request: BrowsePageRequest,
) -> BrowsePageSettlement {
  let BrowsePageRequest {
    source_id,
    source,
    token,
    start_index,
    limit,
    preferences,
  } = request;
  let result = async {
    let start_index = i32::try_from(start_index)
      .map_err(|_| "Library page start index is too large.".to_owned())?;
    let limit = i32::try_from(limit).map_err(|_| "Library page size is too large.".to_owned())?;
    match source {
      BrowseSource::Library { shortcut, .. } => {
        let collection_type = library_kind(&shortcut.collection_type);
        client
          .library()
          .browse_video(VideoLibraryPageRequest {
            library_id: shortcut.id,
            collection_type,
            start_index,
            limit,
            sort: preferences.sort,
            sort_direction: preferences.sort_direction,
            played_filter: preferences.played_filter,
            favorites_only: preferences.favorites_only,
          })
          .await
          .map_err(|error| error.to_string())?
          .try_into()
      }
      BrowseSource::Search { query, .. } => {
        let page = client
          .library()
          .search_video(VideoSearchRequest {
            query: query.clone(),
            start_index,
            limit,
          })
          .await
          .map_err(|error| error.to_string())?;
        if page.query != query {
          return Err("Media server returned results for a different search.".to_owned());
        }
        BrowsePagePayload::try_from(page)
      }
    }
  }
  .await;
  BrowsePageSettlement {
    source_id,
    token,
    result,
  }
}

pub(crate) fn browse_preferences(
  sort_selection: u32,
  played_selection: u32,
  favorites_only: bool,
) -> BrowsePreferences {
  let (sort, sort_direction) = match sort_selection {
    1 => (
      VideoLibrarySort::Title,
      VideoLibrarySortDirection::Descending,
    ),
    2 => (
      VideoLibrarySort::RecentlyAdded,
      VideoLibrarySortDirection::Descending,
    ),
    3 => (
      VideoLibrarySort::ReleaseDate,
      VideoLibrarySortDirection::Descending,
    ),
    _ => (
      VideoLibrarySort::Title,
      VideoLibrarySortDirection::Ascending,
    ),
  };
  let played_filter = match played_selection {
    1 => VideoLibraryPlayedFilter::Unplayed,
    2 => VideoLibraryPlayedFilter::Played,
    _ => VideoLibraryPlayedFilter::All,
  };
  BrowsePreferences {
    sort,
    sort_direction,
    played_filter,
    favorites_only,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn browse_controls_map_to_provider_neutral_preferences() {
    let preferences = browse_preferences(2, 1, true);

    assert!(matches!(preferences.sort, VideoLibrarySort::RecentlyAdded));
    assert!(matches!(
      preferences.sort_direction,
      VideoLibrarySortDirection::Descending
    ));
    assert!(matches!(
      preferences.played_filter,
      VideoLibraryPlayedFilter::Unplayed
    ));
    assert!(preferences.favorites_only);
  }
}
