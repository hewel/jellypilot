//! Image demand follows measured scroll geometry, independently of input dispatch.

use std::any::Any;
use std::collections::HashMap;

use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

use crate::app::artwork::{ArtworkSurface, ImagePriority, ImageSpec};
use crate::app::message::{BrowseMessage, Message};

#[derive(Clone, Copy, Debug)]
pub(crate) enum ImageAxis {
  Horizontal,
  Vertical,
}

/// Marks an image (including its loading placeholder) within [`observe_images`].
pub(crate) fn observe_image<'a>(
  content: Element<'a, Message>,
  surface: ArtworkSurface,
  epoch: u64,
  spec: ImageSpec,
  axis: ImageAxis,
) -> Element<'a, Message> {
  Element::new(Observer {
    content,
    image: Some(Identity {
      surface,
      epoch,
      spec,
      axis,
    }),
    grid_epoch: None,
  })
}

/// Wraps a page outside all its scrollables. Required even for nested image rows.
///
/// iced's scrollable input dispatch discards ancestor clips and can skip children.
/// An operation after dispatch instead visits every marker with the final offsets,
/// without replaying input, requesting redraws, or relying on visible-child updates.
pub(crate) fn observe_images(content: Element<'_, Message>) -> Element<'_, Message> {
  Element::new(Observer {
    content,
    image: None,
    grid_epoch: None,
  })
}

/// Measures the whole Browse grid, including sparse spacers, below its count header.
pub(crate) fn observe_grid_viewport(
  content: Element<'_, Message>,
  epoch: u64,
) -> Element<'_, Message> {
  Element::new(Observer {
    content,
    image: None,
    grid_epoch: Some(epoch),
  })
}

#[derive(Clone, Debug)]
struct Identity {
  surface: ArtworkSurface,
  epoch: u64,
  spec: ImageSpec,
  axis: ImageAxis,
}

impl Identity {
  fn same_location(&self, other: &Self) -> bool {
    self.surface == other.surface && self.epoch == other.epoch && self.spec.key == other.spec.key
  }

  fn same_image(&self, other: &Self) -> bool {
    self.same_location(other) && self.spec == other.spec
  }
}

#[derive(Default)]
struct State {
  image: Option<Identity>,
  grid_epoch: Option<u64>,
  inventory: Option<Inventory>,
}

struct Observer<'a> {
  content: Element<'a, Message>,
  image: Option<Identity>,
  grid_epoch: Option<u64>,
}

impl Widget<Message, Theme, iced::Renderer> for Observer<'_> {
  fn tag(&self) -> widget::tree::Tag {
    widget::tree::Tag::of::<State>()
  }

  fn state(&self) -> widget::tree::State {
    widget::tree::State::new(State {
      image: self.image.clone(),
      grid_epoch: self.grid_epoch,
      inventory: None,
    })
  }

  fn diff(&mut self, tree: &mut widget::Tree) {
    let state = tree.state.downcast_mut::<State>();
    match (state.image.as_mut(), self.image.as_ref()) {
      (Some(current), Some(image)) if current.same_image(image) => current.axis = image.axis,
      _ => state.image = self.image.clone(),
    }
    state.grid_epoch = self.grid_epoch;
    tree.diff_children(std::slice::from_mut(&mut self.content));
  }

  fn size(&self) -> Size<Length> {
    self.content.as_widget().size()
  }

  fn is_void(&self) -> bool {
    self.content.as_widget().is_void()
  }

  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) -> layout::Node {
    self
      .content
      .as_widget_mut()
      .layout(&mut tree.children[0], renderer, limits)
  }

  fn update(
    &mut self,
    tree: &mut widget::Tree,
    event: &Event,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    renderer: &iced::Renderer,
    shell: &mut Shell<'_, Message>,
    viewport: &Rectangle,
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
    if self.image.is_none() && self.grid_epoch.is_none() {
      let inventory = tree
        .state
        .downcast_mut::<State>()
        .inventory
        .get_or_insert_default();
      inventory.begin();
      self.content.as_widget_mut().operate(
        &mut tree.children[0],
        layout,
        renderer,
        &mut Measure {
          clip: Clip::window(*viewport),
          pending: None,
          inventory,
        },
      );
      inventory.finish();
      for message in inventory.messages.drain(..) {
        shell.publish(message);
      }
    }
  }

  fn operate(
    &mut self,
    tree: &mut widget::Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    operation.custom(None, layout.bounds(), tree.state.downcast_mut::<State>());
    self
      .content
      .as_widget_mut()
      .operate(&mut tree.children[0], layout, renderer, operation);
  }

  fn draw(
    &self,
    tree: &widget::Tree,
    renderer: &mut iced::Renderer,
    theme: &Theme,
    style: &renderer::Style,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
  ) {
    self.content.as_widget().draw(
      &tree.children[0],
      renderer,
      theme,
      style,
      layout,
      cursor,
      viewport,
    );
  }

  fn mouse_interaction(
    &self,
    tree: &widget::Tree,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
  ) -> mouse::Interaction {
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
    viewport: &Rectangle,
    translation: Vector,
  ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
    self.content.as_widget_mut().overlay(
      &mut tree.children[0],
      layout,
      renderer,
      viewport,
      translation,
    )
  }
}

#[derive(Clone, Copy)]
struct Clip {
  visible: Option<Rectangle>,
  demand: Option<Rectangle>,
  scrolled: bool,
  horizontal: bool,
  vertical: bool,
}

fn expand(mut bounds: Rectangle, axis: ImageAxis) -> Rectangle {
  match axis {
    ImageAxis::Horizontal => {
      bounds.x -= bounds.width;
      bounds.width *= 3.0;
    }
    ImageAxis::Vertical => {
      bounds.y -= bounds.height;
      bounds.height *= 3.0;
    }
  }
  bounds
}

impl Clip {
  fn window(viewport: Rectangle) -> Self {
    Self {
      visible: Some(viewport),
      demand: Some(viewport),
      scrolled: false,
      horizontal: false,
      vertical: false,
    }
  }

  fn scroll(self, bounds: Rectangle, content: Rectangle, translation: Vector) -> Self {
    let mut near = bounds;
    if content.width > bounds.width {
      near = expand(near, ImageAxis::Horizontal);
    }
    if content.height > bounds.height {
      near = expand(near, ImageAxis::Vertical);
    }
    // Introduce each axis's lookahead once. A horizontal rail may extend the
    // horizontal window, but must retain its ancestor's vertical demand cap.
    let horizontal = content.width > bounds.width;
    let vertical = content.height > bounds.height;
    let demand = self.demand.and_then(|mut parent| {
      if horizontal && !self.horizontal {
        parent.x -= bounds.width;
        parent.width += 2.0 * bounds.width;
      }
      if vertical && !self.vertical {
        parent.y -= bounds.height;
        parent.height += 2.0 * bounds.height;
      }
      parent.intersection(&near)
    });
    Self {
      visible: self
        .visible
        .and_then(|parent| parent.intersection(&bounds))
        .map(|r| r + translation),
      demand: demand
        .map(|r| r + translation)
        .and_then(|r| r.intersection(&content)),
      scrolled: true,
      horizontal: self.horizontal || horizontal,
      vertical: self.vertical || vertical,
    }
  }

  fn priority(self, bounds: Rectangle, axis: ImageAxis) -> Option<ImagePriority> {
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
      return None;
    }
    if self.visible.is_some_and(|clip| clip.intersects(&bounds)) {
      return Some(ImagePriority::Visible);
    }
    let demand = self.demand.map(|clip| {
      if self.scrolled {
        clip
      } else {
        expand(clip, axis)
      }
    });
    demand
      .filter(|clip| clip.intersects(&bounds))
      .map(|_| ImagePriority::Prefetch)
  }
}

#[derive(Default)]
struct Inventory {
  locations: HashMap<(ArtworkSurface, u64), HashMap<String, Report>>,
  messages: Vec<Message>,
  grid: Option<GridGeometry>,
  measured_grid: Option<GridGeometry>,
}

struct Report {
  image: Identity,
  priority: Option<ImagePriority>,
  seen: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct GridGeometry {
  epoch: u64,
  offset_y: f32,
  height: f32,
}

impl Inventory {
  fn begin(&mut self) {
    for locations in self.locations.values_mut() {
      for report in locations.values_mut() {
        report.seen = false;
      }
    }
    self.measured_grid = None;
  }

  fn image(&mut self, image: &Identity, priority: Option<ImagePriority>) {
    let locations = self
      .locations
      .entry((image.surface, image.epoch))
      .or_default();
    if let Some(report) = locations.get_mut(&image.spec.key) {
      report.seen = true;
      if report.image.same_image(image) && report.priority == priority {
        return;
      }
      if !report.image.same_image(image) {
        if report.priority.is_some() {
          self.messages.push(observed(&report.image, None));
        }
        report.image = image.clone();
      }
      report.priority = priority;
    } else {
      locations.insert(
        image.spec.key.clone(),
        Report {
          image: image.clone(),
          priority,
          seen: true,
        },
      );
    }
    self.messages.push(observed(image, priority));
  }

  fn finish(&mut self) {
    self.locations.retain(|_, locations| {
      locations.retain(|_, report| {
        if !report.seen && report.priority.is_some() {
          self.messages.push(observed(&report.image, None));
        }
        report.seen
      });
      !locations.is_empty()
    });
    // Dedup belongs to the root, not markers: skipped or destroyed subtrees
    // disappear from this inventory and restored markers must admit again.
    // Reconciliation can swap keys; all removals precede all admissions.
    self.messages.sort_unstable_by_key(|message| {
      matches!(
        message,
        Message::ImageObserved {
          priority: Some(_),
          ..
        }
      )
    });
    if self.grid != self.measured_grid {
      if let Some(grid) = self.measured_grid {
        self
          .messages
          .push(Message::Browse(BrowseMessage::GridViewportMeasured {
            epoch: grid.epoch,
            offset_y: grid.offset_y,
            height: grid.height,
          }));
      }
      self.grid = self.measured_grid;
    }
  }
}

struct Measure<'a> {
  clip: Clip,
  pending: Option<Clip>,
  inventory: &'a mut Inventory,
}

impl widget::Operation for Measure<'_> {
  fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
    let clip = self.pending.take().unwrap_or(self.clip);
    visit(&mut Measure {
      clip,
      pending: None,
      inventory: self.inventory,
    });
  }

  fn scrollable(
    &mut self,
    _id: Option<&widget::Id>,
    bounds: Rectangle,
    content: Rectangle,
    translation: Vector,
    _state: &mut dyn widget::operation::Scrollable,
  ) {
    self.pending = Some(self.clip.scroll(bounds, content, translation));
  }

  fn custom(&mut self, _id: Option<&widget::Id>, bounds: Rectangle, state: &mut dyn Any) {
    let Some(state) = state.downcast_mut::<State>() else {
      return;
    };
    if let Some(epoch) = state.grid_epoch {
      if let Some(viewport) = self.clip.visible {
        self.inventory.measured_grid = Some(GridGeometry {
          epoch,
          offset_y: viewport.y - bounds.y,
          height: viewport.height,
        });
      }
    }
    if let Some(image) = &state.image {
      self
        .inventory
        .image(image, self.clip.priority(bounds, image.axis));
    }
  }
}

fn observed(image: &Identity, priority: Option<ImagePriority>) -> Message {
  Message::ImageObserved {
    surface: image.surface,
    epoch: image.epoch,
    spec: image.spec.clone(),
    priority,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::advanced::widget::Operation;
  use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};

  fn rect(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
    Rectangle {
      x,
      y,
      width,
      height,
    }
  }

  fn window() -> Clip {
    Clip::window(rect(0.0, 0.0, 300.0, 100.0))
  }

  #[test]
  fn vertical_window_has_exactly_one_viewport_ahead_and_behind() {
    let clip = window().scroll(
      rect(0.0, 0.0, 300.0, 100.0),
      rect(0.0, 0.0, 300.0, 1000.0),
      Vector::new(0.0, 200.0),
    );
    let priorities = [90.0, 100.0, 200.0, 300.0, 400.0]
      .map(|y| clip.priority(rect(0.0, y, 50.0, 10.0), ImageAxis::Vertical));
    assert_eq!(
      priorities,
      [
        None,
        Some(ImagePriority::Prefetch),
        Some(ImagePriority::Visible),
        Some(ImagePriority::Prefetch),
        None,
      ]
    );
  }

  #[test]
  fn nested_horizontal_rows_keep_outer_clip_and_both_offsets() {
    let outer = window().scroll(
      rect(0.0, 0.0, 300.0, 100.0),
      rect(0.0, 0.0, 300.0, 1000.0),
      Vector::new(0.0, 200.0),
    );
    let priorities = [220.0, 320.0, 420.0].map(|y| {
      let row = outer.scroll(
        rect(0.0, y, 300.0, 50.0),
        rect(0.0, y, 2000.0, 50.0),
        Vector::new(600.0, 0.0),
      );
      [600.0, 900.0, 1200.0].map(|x| row.priority(rect(x, y, 50.0, 50.0), ImageAxis::Horizontal))
    });
    assert_eq!(
      priorities,
      [
        [
          Some(ImagePriority::Visible),
          Some(ImagePriority::Prefetch),
          None
        ],
        [
          Some(ImagePriority::Prefetch),
          Some(ImagePriority::Prefetch),
          None
        ],
        [None, None, None],
      ]
    );
  }

  fn identity() -> Identity {
    Identity {
      surface: ArtworkSurface::Home,
      epoch: 1,
      spec: ImageSpec {
        key: "poster".into(),
        image_id: "first".into(),
        size_class: ArtworkSizeClass::Card,
        derived: DerivedArtwork::default(),
      },
      axis: ImageAxis::Vertical,
    }
  }

  fn measure(
    inventory: &mut Inventory,
    markers: &mut [State],
    bounds: Rectangle,
    clip: Clip,
  ) -> Vec<Message> {
    inventory.begin();
    for marker in markers {
      Measure {
        clip,
        pending: None,
        inventory,
      }
      .custom(None, bounds, marker);
    }
    inventory.finish();
    std::mem::take(&mut inventory.messages)
  }

  fn observations(messages: Vec<Message>) -> Vec<(u64, String, Option<ImagePriority>)> {
    messages
      .into_iter()
      .map(|message| match message {
        Message::ImageObserved {
          epoch,
          spec,
          priority,
          ..
        } => (epoch, spec.image_id, priority),
        _ => panic!("unexpected geometry message"),
      })
      .collect()
  }

  #[test]
  fn unchanged_geometry_is_silent_but_identity_epoch_resize_and_exit_publish() {
    let mut inventory = Inventory::default();
    let mut markers = [State {
      image: Some(identity()),
      ..State::default()
    }];
    let bounds = rect(0.0, 50.0, 50.0, 30.0);
    let mut changes = measure(&mut inventory, &mut markers, bounds, window());
    assert!(measure(&mut inventory, &mut markers, bounds, window()).is_empty());
    markers[0].image.as_mut().unwrap().spec.image_id = "replacement".into();
    changes.extend(measure(&mut inventory, &mut markers, bounds, window()));
    markers[0].image.as_mut().unwrap().epoch = 2;
    changes.extend(measure(&mut inventory, &mut markers, bounds, window()));
    let resized = Clip::window(rect(0.0, 0.0, 300.0, 40.0));
    changes.extend(measure(&mut inventory, &mut markers, bounds, resized));
    changes.extend(measure(
      &mut inventory,
      &mut markers,
      rect(0.0, 300.0, 50.0, 30.0),
      resized,
    ));
    assert_eq!(
      observations(changes),
      vec![
        (1, "first".into(), Some(ImagePriority::Visible)),
        (1, "first".into(), None),
        (1, "replacement".into(), Some(ImagePriority::Visible)),
        (1, "replacement".into(), None),
        (2, "replacement".into(), Some(ImagePriority::Visible)),
        (2, "replacement".into(), Some(ImagePriority::Prefetch)),
        (2, "replacement".into(), None),
      ]
    );
  }

  #[test]
  fn destroyed_marker_revokes_only_its_position_of_a_shared_image() {
    let mut second = identity();
    second.spec.key = "second-position".into();
    let mut markers = vec![
      State {
        image: Some(identity()),
        ..State::default()
      },
      State {
        image: Some(second),
        ..State::default()
      },
    ];
    let mut inventory = Inventory::default();
    let bounds = rect(0.0, 20.0, 50.0, 30.0);
    measure(&mut inventory, &mut markers, bounds, window());
    drop(markers.remove(0));
    let changes = measure(&mut inventory, &mut markers, bounds, window());
    assert!(matches!(changes.as_slice(), [
      Message::ImageObserved { spec, priority: None, .. }
    ] if spec.key == "poster"));
    assert!(measure(&mut inventory, &mut markers, bounds, window()).is_empty());
  }

  #[test]
  fn skipped_traversal_revokes_and_restored_same_marker_readmits() {
    let mut inventory = Inventory::default();
    let mut markers = [State {
      image: Some(identity()),
      ..State::default()
    }];
    let bounds = rect(0.0, 20.0, 50.0, 30.0);
    measure(&mut inventory, &mut markers, bounds, window());
    let mut changes = measure(&mut inventory, &mut [], bounds, window());
    changes.extend(measure(&mut inventory, &mut markers, bounds, window()));
    assert_eq!(
      observations(changes),
      vec![
        (1, "first".into(), None),
        (1, "first".into(), Some(ImagePriority::Visible)),
      ]
    );
  }

  #[test]
  fn positional_reconciliation_keeps_moved_locations_and_removes_before_admission() {
    let mut second = identity();
    second.spec.key = "second-position".into();
    second.spec.image_id = "second".into();
    let mut markers = [
      State {
        image: Some(identity()),
        ..State::default()
      },
      State {
        image: Some(second),
        ..State::default()
      },
    ];
    let mut inventory = Inventory::default();
    let bounds = rect(0.0, 20.0, 50.0, 30.0);
    measure(&mut inventory, &mut markers, bounds, window());
    markers.swap(0, 1);
    assert!(measure(&mut inventory, &mut markers, bounds, window()).is_empty());
    markers[1].image.as_mut().unwrap().spec.key = "third-position".into();
    assert_eq!(
      observations(measure(&mut inventory, &mut markers, bounds, window())),
      vec![
        (1, "first".into(), None),
        (1, "first".into(), Some(ImagePriority::Visible)),
      ]
    );
  }

  #[test]
  fn sparse_grid_reports_signed_header_offset_and_full_parent_viewport() {
    let mut inventory = Inventory::default();
    let mut markers = [State {
      grid_epoch: Some(7),
      ..State::default()
    }];
    // The real grid begins after a 40px count header and margin. Its short
    // placeholder must not reduce the 100px parent viewport to the grid height.
    let bounds = rect(0.0, 40.0, 300.0, 20.0);
    let changes = measure(&mut inventory, &mut markers, bounds, window());
    assert!(matches!(
      changes.as_slice(),
      [Message::Browse(BrowseMessage::GridViewportMeasured {
        epoch: 7,
        offset_y: -40.0,
        height: 100.0
      })]
    ));
    assert!(measure(&mut inventory, &mut markers, bounds, window()).is_empty());
    let scrolled = window().scroll(
      rect(0.0, 0.0, 300.0, 100.0),
      rect(0.0, 0.0, 300.0, 1000.0),
      Vector::new(0.0, 200.0),
    );
    let changes = measure(&mut inventory, &mut markers, bounds, scrolled);
    assert!(matches!(
      changes.as_slice(),
      [Message::Browse(BrowseMessage::GridViewportMeasured {
        epoch: 7,
        offset_y: 160.0,
        height: 100.0
      })]
    ));
    markers[0].grid_epoch = Some(8);
    let changes = measure(&mut inventory, &mut markers, bounds, scrolled);
    assert!(matches!(
      changes.as_slice(),
      [Message::Browse(BrowseMessage::GridViewportMeasured {
        epoch: 8,
        ..
      })]
    ));
  }
}
