//! Native danmaku text overlaid on an embedded video surface.
//!
//! The widget owns playback-driven redraws, lane scheduling, and a bounded
//! paragraph cache. It publishes no messages and never captures input, so it
//! can be stacked over the video without changing its controls or streams.

use std::collections::HashMap;
use std::time::Instant;

use iced::advanced::text::{self, paragraph};
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{layout, renderer, Layout, Shell, Widget};
use iced::mouse;
use iced::{Color, Element, Event, Font, Length, Pixels, Rectangle, Size, Vector};
use jellypilot_core::danmaku::{Clock, Kind, Layout as DanmakuLayout, Scheduler, Track};

use crate::tokens::DARK_PALETTE;

const MAX_VISIBLE: usize = 128;
const CACHE_NEARBY: usize = 64;
const MAX_TEXT_WIDTH: f32 = 1_048_576.0;
const OUTLINE_DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;
const OUTLINE_OFFSETS: [Vector; 8] = [
    Vector::new(-1.0, 0.0),
    Vector::new(1.0, 0.0),
    Vector::new(0.0, -1.0),
    Vector::new(0.0, 1.0),
    Vector::new(-OUTLINE_DIAGONAL, -OUTLINE_DIAGONAL),
    Vector::new(OUTLINE_DIAGONAL, -OUTLINE_DIAGONAL),
    Vector::new(-OUTLINE_DIAGONAL, OUTLINE_DIAGONAL),
    Vector::new(OUTLINE_DIAGONAL, OUTLINE_DIAGONAL),
];
// A compact weighted kernel softens the offset shadow without an offscreen
// blur surface or another paragraph layout for every moving comment.
const SHADOW_SAMPLES: [(Vector, f32); 5] = [
    (Vector::new(0.0, 0.0), 0.5),
    (Vector::new(-1.0, 0.0), 0.125),
    (Vector::new(1.0, 0.0), 0.125),
    (Vector::new(0.0, -1.0), 0.125),
    (Vector::new(0.0, 1.0), 0.125),
];

/// Visual and timing settings for a [`Danmaku`] overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Comment font size in logical pixels.
    pub font_size: f32,
    /// Opacity applied to comment glyphs, outline, and shadow samples.
    pub opacity: f32,
    /// Draws a dark outline without changing glyph layout or lane scheduling.
    pub outline_enabled: bool,
    /// Draws the soft offset shadow independently of the outline.
    pub shadow_enabled: bool,
    /// Fraction of the overlay height available to comment lanes.
    pub coverage: f32,
    /// Maximum simultaneous comments; values above 128 are capped.
    pub max_visible: usize,
    /// Time a scrolling comment remains alive.
    pub scroll_seconds: f64,
    /// Time a top or bottom comment remains visible.
    pub fixed_seconds: f64,
    /// Enables `[scroll, top, bottom]` comments respectively.
    pub kinds: [bool; 3],
    /// Media-time offset; positive values delay comments.
    pub offset_seconds: f64,
    /// Clear space above the comment region in logical pixels.
    pub top_inset: f32,
    /// Clear space below the comment region in logical pixels.
    pub bottom_inset: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font_size: 24.0,
            opacity: 0.75,
            outline_enabled: true,
            shadow_enabled: true,
            coverage: 0.5,
            max_visible: 48,
            scroll_seconds: 10.0,
            fixed_seconds: 5.0,
            kinds: [true; 3],
            offset_seconds: 0.0,
            top_inset: 0.0,
            bottom_inset: 0.0,
        }
    }
}

/// A full-size, transparent danmaku overlay driven by a shared playback clock.
pub struct Danmaku<'a> {
    track: &'a Track,
    clock: &'a Clock,
    style: Style,
    enabled: bool,
    font: Option<Font>,
}

/// Creates a native danmaku overlay for `track` using the shared media `clock`.
#[must_use]
pub fn danmaku<'a>(track: &'a Track, clock: &'a Clock) -> Danmaku<'a> {
    Danmaku {
        track,
        clock,
        style: Style::default(),
        enabled: true,
        font: None,
    }
}

impl<'a> Danmaku<'a> {
    /// Replaces all visual and timing settings.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Shows or hides the overlay. Disabling it also stops playback redraws.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Selects the font used for both width measurement and glyph rendering.
    #[must_use]
    pub fn font(mut self, font: Font) -> Self {
        self.font = Some(font);
        self
    }
}

impl iced::advanced::widget::Meta for Danmaku<'_> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShapingKey {
    font: Font,
    font_size: u32,
    line_height: u32,
    hint_factor: Option<u32>,
}

struct CachedParagraph<P: text::Paragraph> {
    paragraph: paragraph::Plain<P>,
    width: f32,
    last_used: u64,
}

impl<P: text::Paragraph> Default for CachedParagraph<P> {
    fn default() -> Self {
        Self {
            paragraph: paragraph::Plain::default(),
            width: 0.0,
            last_used: 0,
        }
    }
}

struct ParagraphCache<P: text::Paragraph> {
    entries: HashMap<usize, CachedParagraph<P>>,
    access_counter: u64,
    capacity: usize,
}

impl<P: text::Paragraph> Default for ParagraphCache<P> {
    fn default() -> Self {
        Self {
            entries: HashMap::default(),
            access_counter: 0,
            capacity: Style::default().max_visible + CACHE_NEARBY,
        }
    }
}

impl<P: text::Paragraph> ParagraphCache<P> {
    fn clear(&mut self) {
        self.entries.clear();
    }

    fn begin_update(&mut self, capacity: usize) {
        self.capacity = capacity;
        while self.entries.len() > self.capacity {
            self.evict_least_recent();
        }
    }

    fn next_access(&mut self) -> u64 {
        self.access_counter = self.access_counter.wrapping_add(1);
        self.access_counter
    }

    fn measure<R>(
        &mut self,
        renderer: &R,
        index: usize,
        content: &str,
        font: Font,
        font_size: f32,
        line_height: f32,
    ) -> f32
    where
        R: text::Renderer<Paragraph = P>,
    {
        let last_used = self.next_access();
        if let Some(entry) = self.entries.get_mut(&index) {
            entry.last_used = last_used;
            return entry.width;
        }
        if self.entries.len() >= self.capacity {
            self.evict_least_recent();
        }

        let mut entry = CachedParagraph::default();
        let _ = entry.paragraph.update(text::Text {
            content,
            bounds: Size::new(MAX_TEXT_WIDTH, line_height),
            size: Pixels(font_size),
            line_height: text::LineHeight::Absolute(Pixels(line_height)),
            font,
            align_x: text::Alignment::Left,
            align_y: iced::alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::None,
            hint_factor: renderer.hint_factor(),
        });
        entry.width = entry.paragraph.min_width();
        entry.last_used = last_used;
        let width = entry.width;
        self.entries.insert(index, entry);
        width
    }

    fn touch(&mut self, index: usize) -> bool {
        let last_used = self.next_access();
        if let Some(entry) = self.entries.get_mut(&index) {
            entry.last_used = last_used;
            true
        } else {
            false
        }
    }

    fn paragraph(&self, index: usize) -> Option<&P> {
        self.entries.get(&index).map(|entry| entry.paragraph.raw())
    }

    fn evict_least_recent(&mut self) {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(&index, _)| index);
        if let Some(index) = oldest {
            self.entries.remove(&index);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Redraw {
    Wait,
    NextFrame,
    At(Instant),
}

struct State<P: text::Paragraph> {
    scheduler: Scheduler,
    cache: ParagraphCache<P>,
    track_id: Option<u64>,
    media_id: Option<u64>,
    layout: Option<DanmakuLayout>,
    shaping: Option<ShapingKey>,
    upcoming: Option<usize>,
    upcoming_valid: bool,
    last_position: Option<f64>,
}

impl<P: text::Paragraph> Default for State<P> {
    fn default() -> Self {
        Self {
            scheduler: Scheduler::default(),
            cache: ParagraphCache::default(),
            track_id: None,
            media_id: None,
            layout: None,
            shaping: None,
            upcoming: None,
            upcoming_valid: false,
            last_position: None,
        }
    }
}

impl<P: text::Paragraph> State<P> {
    #[allow(
        clippy::too_many_arguments,
        reason = "widget inputs mirror its public contract"
    )]
    fn synchronize<R>(
        &mut self,
        renderer: &R,
        track: &Track,
        clock: &Clock,
        style: Style,
        font: Font,
        layout: DanmakuLayout,
        shaping: ShapingKey,
        enabled: bool,
        now: Instant,
    ) -> Redraw
    where
        R: text::Renderer<Paragraph = P>,
    {
        if self.track_id != Some(track.id()) {
            self.track_id = Some(track.id());
            self.scheduler.clear();
            self.cache.clear();
            self.upcoming_valid = false;
            self.last_position = None;
        }
        if self.media_id != clock.media_id() {
            self.media_id = clock.media_id();
            self.scheduler.clear();
            self.upcoming_valid = false;
            self.last_position = None;
        }
        if self.shaping != Some(shaping) {
            self.shaping = Some(shaping);
            self.scheduler.clear();
            self.cache.clear();
        }
        if self.layout != Some(layout) {
            self.layout = Some(layout);
            self.scheduler.clear();
            self.upcoming_valid = false;
        }

        let usable = enabled
            && !track.comments().is_empty()
            && style.opacity.is_finite()
            && style.opacity > 0.0
            && style.font_size.is_finite()
            && style.font_size > 0.0
            && style.coverage.is_finite()
            && style.coverage > 0.0
            && style.max_visible > 0
            && style.kinds.into_iter().any(|enabled| enabled)
            && layout.width.is_finite()
            && layout.height.is_finite()
            && layout.width > 0.0
            && layout.height > 0.0;
        let Some(position) = clock.position(now).filter(|_| usable) else {
            self.scheduler.clear();
            self.upcoming_valid = false;
            self.last_position = None;
            return Redraw::Wait;
        };
        let advancing = clock.advancing(now);

        if self
            .last_position
            .is_some_and(|previous| position < previous)
        {
            self.upcoming_valid = false;
        }
        self.last_position = Some(position);
        if !self.upcoming_valid {
            self.reset_upcoming(track, position, layout);
        }
        self.advance_upcoming(track, position, layout);

        let cache_capacity = layout.max_visible.saturating_add(CACHE_NEARBY);
        self.cache.begin_update(cache_capacity);
        let cache = &mut self.cache;
        self.scheduler
            .update(track, position, layout, |index, content| {
                cache.measure(
                    renderer,
                    index,
                    content,
                    font,
                    style.font_size,
                    layout.line_height,
                )
            });

        // A bounded cache may have evicted an active entry while nearby comments
        // were measured. Restore every visible entry before the immutable draw.
        for placement in self.scheduler.visible() {
            if self.cache.touch(placement.index) {
                continue;
            }
            if let Some(comment) = track.comments().get(placement.index) {
                let _ = self.cache.measure(
                    renderer,
                    placement.index,
                    &comment.text,
                    font,
                    style.font_size,
                    layout.line_height,
                );
            }
        }

        if !advancing {
            return Redraw::Wait;
        }
        if self.scheduler.visible().iter().any(|placement| {
            track
                .comments()
                .get(placement.index)
                .is_some_and(|comment| {
                    comment.kind == Kind::Scroll && enabled_kind(layout.kinds, comment.kind)
                })
        }) {
            return Redraw::NextFrame;
        }

        let mut deadline = self.upcoming.and_then(|index| {
            track
                .comments()
                .get(index)
                .map(|comment| comment.at_seconds + layout.offset_seconds)
        });
        for placement in self.scheduler.visible() {
            let Some(comment) = track.comments().get(placement.index) else {
                continue;
            };
            if comment.kind != Kind::Scroll {
                let expires = comment.at_seconds + layout.offset_seconds + layout.fixed_seconds;
                deadline = Some(deadline.map_or(expires, |current| current.min(expires)));
            }
        }
        let Some(at) = deadline.and_then(|position| clock.deadline(position, now)) else {
            return Redraw::Wait;
        };
        if at <= now {
            Redraw::NextFrame
        } else {
            Redraw::At(at)
        }
    }

    fn reset_upcoming(&mut self, track: &Track, position: f64, layout: DanmakuLayout) {
        let comments = track.comments();
        let mut next = comments
            .partition_point(|comment| comment.at_seconds + layout.offset_seconds <= position);
        while comments
            .get(next)
            .is_some_and(|comment| !enabled_kind(layout.kinds, comment.kind))
        {
            next += 1;
        }
        self.upcoming = (next < comments.len()).then_some(next);
        self.upcoming_valid = true;
    }

    fn advance_upcoming(&mut self, track: &Track, position: f64, layout: DanmakuLayout) {
        let comments = track.comments();
        let mut next = self.upcoming.unwrap_or(comments.len());
        while comments.get(next).is_some_and(|comment| {
            comment.at_seconds + layout.offset_seconds <= position
                || !enabled_kind(layout.kinds, comment.kind)
        }) {
            next += 1;
        }
        self.upcoming = (next < comments.len()).then_some(next);
    }
}

fn enabled_kind(kinds: [bool; 3], kind: Kind) -> bool {
    match kind {
        Kind::Scroll => kinds[0],
        Kind::Top => kinds[1],
        Kind::Bottom => kinds[2],
    }
}

fn core_layout(style: Style, bounds: Size, line_height: f32) -> DanmakuLayout {
    let defaults = DanmakuLayout::default();
    DanmakuLayout {
        width: bounds.width,
        height: bounds.height,
        line_height,
        top_inset: finite_or(style.top_inset, 0.0).max(0.0),
        bottom_inset: finite_or(style.bottom_inset, 0.0).max(0.0),
        coverage: if style.coverage.is_finite() {
            style.coverage.clamp(0.0, 1.0)
        } else {
            defaults.coverage
        },
        max_visible: style.max_visible.min(MAX_VISIBLE),
        scroll_seconds: positive_or(style.scroll_seconds, defaults.scroll_seconds),
        fixed_seconds: positive_or(style.fixed_seconds, defaults.fixed_seconds),
        kinds: style.kinds,
        offset_seconds: if style.offset_seconds.is_finite() {
            style.offset_seconds
        } else {
            0.0
        },
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn positive_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Danmaku<'_>
where
    Renderer: text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer::Paragraph>::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        tree.size = layout::atomic(limits, Length::Fill, Length::Fill);
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let font = self.font.unwrap_or_else(|| renderer.font());
        let line_height = renderer
            .line_height()
            .to_absolute(Pixels(self.style.font_size))
            .0;
        let layout = core_layout(self.style, tree.size, line_height);
        let shaping = ShapingKey {
            font,
            font_size: self.style.font_size.to_bits(),
            line_height: line_height.to_bits(),
            hint_factor: renderer.hint_factor().map(f32::to_bits),
        };
        let _ = state.synchronize(
            renderer,
            self.track,
            self.clock,
            self.style,
            font,
            layout,
            shaping,
            self.enabled,
            Instant::now(),
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        _cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let Event::Window(iced::window::Event::RedrawRequested(now)) = event else {
            return;
        };
        if layout
            .bounds()
            .intersection(viewport)
            .is_none_or(|clip| clip.width <= 0.0 || clip.height <= 0.0)
        {
            return;
        }
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let font = self.font.unwrap_or_else(|| renderer.font());
        let line_height = renderer
            .line_height()
            .to_absolute(Pixels(self.style.font_size))
            .0;
        let core_layout = core_layout(self.style, layout.bounds().size(), line_height);
        let shaping = ShapingKey {
            font,
            font_size: self.style.font_size.to_bits(),
            line_height: line_height.to_bits(),
            hint_factor: renderer.hint_factor().map(f32::to_bits),
        };
        match state.synchronize(
            renderer,
            self.track,
            self.clock,
            self.style,
            font,
            core_layout,
            shaping,
            self.enabled,
            *now,
        ) {
            Redraw::Wait => {}
            Redraw::NextFrame => shell.request_redraw(),
            Redraw::At(at) => shell.request_redraw_at(at),
        }
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "renders dynamic per-comment RGB colors from danmaku stream metadata"
    )]
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !self.enabled || !self.style.opacity.is_finite() || self.style.opacity <= 0.0 {
            return;
        }
        let Some(clip) = layout.bounds().intersection(viewport) else {
            return;
        };
        if clip.width <= 0.0 || clip.height <= 0.0 {
            return;
        }

        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let opacity = self.style.opacity.clamp(0.0, 1.0);
        let outline = self
            .style
            .outline_enabled
            .then(|| DARK_PALETTE.colors.background.scale_alpha(opacity));
        let shadow_samples = self.style.shadow_enabled.then(|| {
            let shadow = DARK_PALETTE.shadows.raised;
            SHADOW_SAMPLES.map(|(offset, weight)| {
                (
                    shadow.offset + offset,
                    shadow.color.scale_alpha(opacity * weight),
                )
            })
        });
        let origin = layout.bounds().position();
        for placement in state.scheduler.visible() {
            let Some(comment) = self.track.comments().get(placement.index) else {
                continue;
            };
            let Some(paragraph) = state.cache.paragraph(placement.index) else {
                continue;
            };
            if !placement.x.is_finite() || !placement.y.is_finite() {
                continue;
            }
            let position = origin + Vector::new(placement.x, placement.y);
            if let Some(samples) = &shadow_samples {
                for &(offset, color) in samples {
                    renderer.fill_paragraph(paragraph, position + offset, color, clip);
                }
            }
            if let Some(outline) = outline {
                for offset in OUTLINE_OFFSETS {
                    renderer.fill_paragraph(paragraph, position + offset, outline, clip);
                }
            }
            let color = Color::from_rgb8(
                ((comment.color >> 16) & 0xff) as u8,
                ((comment.color >> 8) & 0xff) as u8,
                (comment.color & 0xff) as u8,
            )
            .scale_alpha(opacity);
            renderer.fill_paragraph(paragraph, position, color, clip);
        }
    }
}

impl<'a, Message, Theme, Renderer> From<Danmaku<'a>> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(danmaku: Danmaku<'a>) -> Self {
        danmaku._boxed()
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "test asserts rendered RGBA color for hex comment payload"
)]
mod tests {
    use std::cell::Cell;
    use std::time::Duration;

    use iced::advanced::{image, shell};
    use iced::{Background, Point, Theme, Transformation};
    use jellypilot_core::danmaku::{ClockSample, Comment};

    use super::*;

    thread_local! {
        static SHAPE_COUNT: Cell<usize> = const { Cell::new(0) };
    }

    #[derive(Debug, Clone)]
    struct Draw {
        paragraph: usize,
        content: String,
        position: Point,
        color: Color,
        clip: Rectangle,
    }

    struct RecordingParagraph {
        id: usize,
        content: String,
        bounds: Size,
        size: Pixels,
        line_height: text::LineHeight,
        font: Font,
        align_x: text::Alignment,
        align_y: iced::alignment::Vertical,
        shaping: text::Shaping,
        wrapping: text::Wrapping,
        ellipsis: text::Ellipsis,
        hint_factor: Option<f32>,
    }

    impl Default for RecordingParagraph {
        fn default() -> Self {
            Self {
                id: 0,
                content: String::new(),
                bounds: Size::ZERO,
                size: Pixels(16.0),
                line_height: text::LineHeight::default(),
                font: Font::DEFAULT,
                align_x: text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: text::Shaping::default(),
                wrapping: text::Wrapping::default(),
                ellipsis: text::Ellipsis::default(),
                hint_factor: None,
            }
        }
    }

    impl text::Paragraph for RecordingParagraph {
        fn with_text(text: text::Text<&str>) -> Self {
            let id = SHAPE_COUNT.with(|count| {
                let next = count.get() + 1;
                count.set(next);
                next
            });
            Self {
                id,
                content: text.content.to_owned(),
                bounds: text.bounds,
                size: text.size,
                line_height: text.line_height,
                font: text.font,
                align_x: text.align_x,
                align_y: text.align_y,
                shaping: text.shaping,
                wrapping: text.wrapping,
                ellipsis: text.ellipsis,
                hint_factor: text.hint_factor,
            }
        }

        fn with_spans<Link>(text: text::Text<&[text::Span<'_, Link>]>) -> Self {
            let content = text
                .content
                .iter()
                .map(|span| span.text.as_ref())
                .collect::<String>();
            Self::with_text(text::Text {
                content: &content,
                bounds: text.bounds,
                size: text.size,
                line_height: text.line_height,
                font: text.font,
                align_x: text.align_x,
                align_y: text.align_y,
                shaping: text.shaping,
                wrapping: text.wrapping,
                ellipsis: text.ellipsis,
                hint_factor: text.hint_factor,
            })
        }

        fn resize(&mut self, new_bounds: Size) {
            self.bounds = new_bounds;
        }

        fn compare(&self, other: text::Text<()>) -> text::Difference {
            if self.size != other.size
                || self.line_height != other.line_height
                || self.font != other.font
                || self.align_x != other.align_x
                || self.align_y != other.align_y
                || self.shaping != other.shaping
                || self.wrapping != other.wrapping
                || self.ellipsis != other.ellipsis
                || self.hint_factor != other.hint_factor
            {
                text::Difference::Shape
            } else if self.bounds != other.bounds {
                text::Difference::Bounds
            } else {
                text::Difference::None
            }
        }

        fn size(&self) -> Pixels {
            self.size
        }

        fn hint_factor(&self) -> Option<f32> {
            self.hint_factor
        }

        fn font(&self) -> Font {
            self.font
        }

        fn line_height(&self) -> text::LineHeight {
            self.line_height
        }

        fn align_x(&self) -> text::Alignment {
            self.align_x
        }

        fn align_y(&self) -> iced::alignment::Vertical {
            self.align_y
        }

        fn wrapping(&self) -> text::Wrapping {
            self.wrapping
        }

        fn ellipsis(&self) -> text::Ellipsis {
            self.ellipsis
        }

        fn shaping(&self) -> text::Shaping {
            self.shaping
        }

        fn bounds(&self) -> Size {
            self.bounds
        }

        fn min_bounds(&self) -> Size {
            Size::new(
                self.content.chars().count() as f32 * self.size.0 * 0.5,
                self.line_height.to_absolute(self.size).0,
            )
        }

        fn hit_test(&self, _point: Point) -> Option<text::Hit> {
            None
        }

        fn hit_span(&self, _point: Point) -> Option<usize> {
            None
        }

        fn span_bounds(&self, _index: usize) -> Vec<Rectangle> {
            Vec::new()
        }

        fn select(&mut self, _start: Point, _end: Point, _target: text::Target) {}

        fn select_all(&mut self) {}

        fn deselect(&mut self) {}

        fn selection(&self) -> &[Rectangle] {
            &[]
        }

        fn copy(&mut self) -> Option<String> {
            None
        }
    }

    #[derive(Default)]
    struct RecordingRenderer {
        draws: Vec<Draw>,
    }

    impl renderer::Renderer for RecordingRenderer {
        fn blur_backdrop(&mut self, _blur: impl Into<iced::Blur>) {}

        fn draw_backdrop(&mut self, _backdrop: renderer::Backdrop) {}

        fn start_layer(&mut self, _bounds: Rectangle) {}

        fn end_layer(&mut self) {}

        fn start_transformation(&mut self, _transformation: Transformation) {}

        fn end_transformation(&mut self) {}

        fn fill_quad(&mut self, _quad: renderer::Quad, _background: impl Into<Background>) {}

        fn allocate_image(
            &self,
            _handle: &image::Handle,
            _callback: impl FnOnce(Result<image::Allocation, image::Error>) + Send + 'static,
        ) {
            panic!("danmaku does not allocate images")
        }

        fn hint(&mut self, _scale: renderer::Scale) {}

        fn scale(&self) -> Option<renderer::Scale> {
            None
        }

        fn reset(&mut self, _new_bounds: Rectangle) {
            self.draws.clear();
        }

        fn settings(&self) -> renderer::Settings {
            renderer::Settings::default()
        }
    }

    impl text::Renderer for RecordingRenderer {
        type Paragraph = RecordingParagraph;
        type Editor = ();

        const ICON_FONT: Font = Font::DEFAULT;
        const CHECKMARK_ICON: char = '0';
        const ARROW_DOWN_ICON: char = '0';
        const SCROLL_UP_ICON: char = '0';
        const SCROLL_DOWN_ICON: char = '0';
        const SCROLL_LEFT_ICON: char = '0';
        const SCROLL_RIGHT_ICON: char = '0';
        const ICED_LOGO: char = '0';

        fn fill_paragraph(
            &mut self,
            paragraph: &Self::Paragraph,
            position: Point,
            color: Color,
            clip_bounds: Rectangle,
        ) {
            self.draws.push(Draw {
                paragraph: paragraph.id,
                content: paragraph.content.clone(),
                position,
                color,
                clip: clip_bounds,
            });
        }

        fn fill_editor(
            &mut self,
            _editor: &Self::Editor,
            _position: Point,
            _color: Color,
            _clip_bounds: Rectangle,
        ) {
        }

        fn fill_text(
            &mut self,
            _text: text::Text<String>,
            _position: Point,
            _color: Color,
            _clip_bounds: Rectangle,
        ) {
        }
    }

    struct UpdateOutcome {
        captured: bool,
        messages_empty: bool,
        redraw: iced::window::RedrawRequest,
    }

    fn track(content: &str) -> Track {
        Track::new(vec![Comment {
            at_seconds: 0.0,
            text: content.to_owned(),
            color: 0xffd166,
            kind: Kind::Scroll,
        }])
        .expect("valid test track")
    }

    fn clock(now: Instant, paused: bool) -> Clock {
        let mut clock = Clock::default();
        assert!(clock.observe(
            ClockSample {
                media_id: 7,
                position_seconds: 0.0,
                speed: 1.0,
                paused,
                buffering: false,
                seeking: false,
            },
            now,
        ));
        clock
    }

    fn new_tree(widget: &Danmaku<'_>) -> Tree {
        Tree::new::<(), Theme, RecordingRenderer>(widget)
    }

    fn lay_out(
        widget: &mut Danmaku<'_>,
        tree: &mut Tree,
        renderer: &RecordingRenderer,
        size: Size,
    ) {
        <Danmaku<'_> as Widget<(), Theme, RecordingRenderer>>::layout(
            widget,
            tree,
            renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
    }

    fn draw(
        widget: &Danmaku<'_>,
        tree: &Tree,
        renderer: &mut RecordingRenderer,
        node: Layout,
        viewport: &Rectangle,
    ) {
        <Danmaku<'_> as Widget<(), Theme, RecordingRenderer>>::draw(
            widget,
            tree,
            renderer,
            &Theme::Dark,
            &renderer::Style::default(),
            node,
            mouse::Cursor::Unavailable,
            viewport,
        );
    }

    fn update(
        widget: &mut Danmaku<'_>,
        tree: &mut Tree,
        renderer: &RecordingRenderer,
        node: Layout,
        event: Event,
    ) -> UpdateOutcome {
        let mut bus = shell::Bus::default();
        let viewport = Rectangle::with_size(node.size());
        let mut shell = Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut bus);
        <Danmaku<'_> as Widget<(), Theme, RecordingRenderer>>::update(
            widget,
            tree,
            &event,
            node,
            mouse::Cursor::Available(node.bounds().center()),
            renderer,
            &mut shell,
            &viewport,
        );
        UpdateOutcome {
            captured: shell.is_event_captured(),
            messages_empty: shell.is_empty(),
            redraw: shell.redraw_request(),
        }
    }

    fn shape_count() -> usize {
        SHAPE_COUNT.with(Cell::get)
    }

    #[test]
    fn clipped_overlay_stops_redrawing_until_it_is_visible_again() {
        let start = Instant::now();
        let track = track("offscreen comment");
        let clock = clock(start, false);
        let mut widget = danmaku(&track, &clock);
        let renderer = RecordingRenderer::default();
        let mut tree = new_tree(&widget);
        lay_out(&mut widget, &mut tree, &renderer, Size::new(240.0, 140.0));
        let node = Layout::new(tree.size);
        let event = Event::Window(iced::window::Event::RedrawRequested(
            start + Duration::from_millis(16),
        ));
        let hidden = update(
            &mut widget,
            &mut tree,
            &renderer,
            node.move_to(Point::new(500.0, 0.0)),
            event.clone(),
        );
        assert_eq!(hidden.redraw, iced::window::RedrawRequest::Wait);
        let visible = update(&mut widget, &mut tree, &renderer, node, event);
        assert_eq!(visible.redraw, iced::window::RedrawRequest::NextFrame);
    }

    #[test]
    fn scrolling_reuses_shaping_while_positions_move_and_drawing_stays_clipped() {
        SHAPE_COUNT.with(|count| count.set(0));
        let start = Instant::now();
        let track = track("moving comment");
        let clock = clock(start, false);
        let mut widget = danmaku(&track, &clock).style(Style {
            coverage: 1.0,
            ..Style::default()
        });
        let mut renderer = RecordingRenderer::default();
        let mut tree = new_tree(&widget);
        lay_out(&mut widget, &mut tree, &renderer, Size::new(240.0, 140.0));
        let node = Layout::new(tree.size).move_to(Point::new(20.0, 30.0));
        let viewport = Rectangle::new(Point::new(40.0, 30.0), Size::new(140.0, 100.0));
        let clip = node.bounds().intersection(&viewport).expect("visible clip");

        draw(&widget, &tree, &mut renderer, node, &viewport);
        let first = renderer.draws.last().expect("main glyph draw").clone();
        assert_eq!(first.content, "moving comment");
        assert_eq!(first.clip, clip);
        assert!(renderer.draws.iter().all(|draw| draw.clip == clip));

        let outcome = update(
            &mut widget,
            &mut tree,
            &renderer,
            node,
            Event::Window(iced::window::Event::RedrawRequested(
                Instant::now() + Duration::from_millis(16),
            )),
        );
        assert_eq!(outcome.redraw, iced::window::RedrawRequest::NextFrame);
        assert!(!outcome.captured);
        assert!(outcome.messages_empty);

        renderer.draws.clear();
        draw(&widget, &tree, &mut renderer, node, &viewport);
        let second = renderer.draws.last().expect("moved glyph draw");
        assert!(second.position.x < first.position.x);
        assert_eq!(second.position.y, first.position.y);
        assert_eq!(second.paragraph, first.paragraph);
        assert_eq!(second.clip, clip);
        assert_eq!(shape_count(), 1, "movement must not reshape text");

        let input = update(
            &mut widget,
            &mut tree,
            &renderer,
            node,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        );
        assert!(!input.captured);
        assert!(input.messages_empty);
        assert_eq!(input.redraw, iced::window::RedrawRequest::Wait);
        assert_eq!(
            <Danmaku<'_> as Widget<(), Theme, RecordingRenderer>>::mouse_interaction(
                &widget,
                &tree,
                node,
                mouse::Cursor::Available(node.bounds().center()),
                &viewport,
                &renderer,
            ),
            mouse::Interaction::None
        );
    }

    #[test]
    fn text_effects_stay_behind_colored_glyphs_and_hide_with_the_overlay() {
        for kind in [Kind::Scroll, Kind::Top, Kind::Bottom] {
            let track = Track::new(vec![Comment {
                at_seconds: 0.0,
                text: "描边 Shadow".to_owned(),
                color: 0xffcc66,
                kind,
            }])
            .expect("valid colored comment");
            let clock = clock(Instant::now(), true);
            let style = Style {
                opacity: 0.5,
                coverage: 1.0,
                ..Style::default()
            };
            let mut widget = danmaku(&track, &clock).style(style);
            let mut renderer = RecordingRenderer::default();
            let mut tree = new_tree(&widget);
            lay_out(&mut widget, &mut tree, &renderer, Size::new(240.0, 140.0));
            let node = Layout::new(tree.size).move_to(Point::new(20.0, 30.0));
            let viewport = Rectangle::new(Point::new(40.0, 30.0), Size::new(140.0, 100.0));
            let shapes = shape_count();

            draw(&widget, &tree, &mut renderer, node, &viewport);
            let (glyph, effects) = renderer.draws.split_last().expect("foreground glyph");
            assert_eq!(
                glyph.color,
                Color::from_rgb8(0xff, 0xcc, 0x66).scale_alpha(0.5)
            );
            let shadow_end = effects.partition_point(|effect| effect.color.a < glyph.color.a);
            let (shadows, outlines) = effects.split_at(shadow_end);
            let center_shadow = shadows.first().expect("shadow beneath outline");
            assert!(center_shadow.position.y > glyph.position.y);
            assert!(shadows
                .iter()
                .any(|sample| sample.color.a < center_shadow.color.a));
            for (horizontal, vertical) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                assert!(
                    outlines.iter().any(|outline| {
                        (outline.position.x - glyph.position.x) * horizontal > 0.0
                            && (outline.position.y - glyph.position.y) * vertical > 0.0
                    }),
                    "outline must cover diagonal glyph edges"
                );
            }
            assert!(effects.iter().all(|effect| {
                effect.paragraph == glyph.paragraph
                    && effect.clip == glyph.clip
                    && effect.color.a > 0.0
                    && effect.color.r < glyph.color.r
            }));
            assert_eq!(
                glyph.clip,
                node.bounds().intersection(&viewport).expect("clip")
            );
            assert_eq!(shape_count(), shapes, "effects reuse the shaped paragraph");

            renderer.draws.clear();
            widget = widget.style(Style {
                opacity: 0.0,
                ..style
            });
            draw(&widget, &tree, &mut renderer, node, &viewport);
            assert!(
                renderer.draws.is_empty(),
                "transparent text must leave no shadow"
            );
        }
    }

    #[test]
    fn effect_switches_are_independent_and_preserve_cached_glyph_geometry() {
        let track = track("independent effects");
        let clock = clock(Instant::now(), true);
        let mut renderer = RecordingRenderer::default();
        let mut tree = new_tree(&danmaku(&track, &clock));
        let mut original = None;
        let mut original_shapes = None;
        for (outline_enabled, shadow_enabled) in [
            (true, true),
            (false, true),
            (true, false),
            (false, false),
            (true, true),
        ] {
            let mut widget = danmaku(&track, &clock).style(Style {
                outline_enabled,
                shadow_enabled,
                ..Style::default()
            });
            Tree::diff::<(), Theme, RecordingRenderer>(&mut tree, &mut widget);
            lay_out(&mut widget, &mut tree, &renderer, Size::new(240.0, 140.0));
            let node = Layout::new(tree.size);
            renderer.draws.clear();
            draw(&widget, &tree, &mut renderer, node, &node.bounds());
            let (glyph, effects) = renderer
                .draws
                .split_last()
                .expect("foreground stays visible");
            assert_eq!(
                effects.iter().any(|effect| effect.color.a == glyph.color.a),
                outline_enabled,
                "outline follows only its own switch"
            );
            assert_eq!(
                effects.iter().any(|effect| effect.color.a < glyph.color.a),
                shadow_enabled,
                "shadow follows only its own switch"
            );
            let identity = (glyph.paragraph, glyph.position, glyph.color, glyph.clip);
            assert_eq!(*original.get_or_insert(identity), identity);
            assert_eq!(*original_shapes.get_or_insert(shape_count()), shape_count());
        }
    }

    #[test]
    fn paused_and_disabled_overlays_do_not_draw_or_request_frames() {
        for (paused, enabled) in [(true, true), (false, false)] {
            let start = Instant::now();
            let track = track("still comment");
            let clock = clock(start, paused);
            let mut widget = danmaku(&track, &clock).enabled(enabled);
            let mut renderer = RecordingRenderer::default();
            let mut tree = new_tree(&widget);
            lay_out(&mut widget, &mut tree, &renderer, Size::new(240.0, 140.0));
            let node = Layout::new(tree.size);
            let viewport = node.bounds();

            let outcome = update(
                &mut widget,
                &mut tree,
                &renderer,
                node,
                Event::Window(iced::window::Event::RedrawRequested(
                    start + Duration::from_millis(16),
                )),
            );
            assert_eq!(outcome.redraw, iced::window::RedrawRequest::Wait);
            assert!(!outcome.captured);
            assert!(outcome.messages_empty);

            // Pausing freezes already-visible comments; disabling hides them.
            draw(&widget, &tree, &mut renderer, node, &viewport);
            if enabled {
                assert!(renderer.draws.iter().all(|draw| draw.clip == viewport));
            } else {
                assert!(renderer.draws.is_empty());
            }
        }
    }

    #[test]
    fn opacity_reuses_paragraphs_but_font_and_track_changes_reshape() {
        SHAPE_COUNT.with(|count| count.set(0));
        let start = Instant::now();
        let first_track = track("first track");
        let second_track = track("replacement track");
        let clock = clock(start, false);
        let style = Style {
            coverage: 1.0,
            ..Style::default()
        };
        let mut renderer = RecordingRenderer::default();
        let mut widget = danmaku(&first_track, &clock)
            .style(style)
            .font(Font::DEFAULT);
        let mut tree = new_tree(&widget);
        lay_out(&mut widget, &mut tree, &renderer, Size::new(240.0, 140.0));
        let initial_shapes = shape_count();
        assert_eq!(initial_shapes, 1);

        let layout = Layout::new(tree.size);
        let outcome = update(
            &mut widget,
            &mut tree,
            &renderer,
            layout,
            Event::Window(iced::window::Event::RedrawRequested(
                start + Duration::from_millis(16),
            )),
        );
        assert_eq!(outcome.redraw, iced::window::RedrawRequest::NextFrame);
        assert_eq!(shape_count(), initial_shapes);

        let translucent = Style {
            opacity: 0.4,
            ..style
        };
        let mut replacement = danmaku(&first_track, &clock)
            .style(translucent)
            .font(Font::DEFAULT);
        Tree::diff::<(), Theme, RecordingRenderer>(&mut tree, &mut replacement);
        lay_out(
            &mut replacement,
            &mut tree,
            &renderer,
            Size::new(240.0, 140.0),
        );
        assert_eq!(shape_count(), initial_shapes);
        let node = Layout::new(tree.size);
        draw(&replacement, &tree, &mut renderer, node, &node.bounds());
        assert_eq!(renderer.draws.last().expect("glyph draw").color.a, 0.4);
        let first_paragraph = renderer.draws.last().expect("glyph draw").paragraph;

        let mut replacement = danmaku(&first_track, &clock)
            .style(translucent)
            .font(Font::MONOSPACE);
        Tree::diff::<(), Theme, RecordingRenderer>(&mut tree, &mut replacement);
        lay_out(
            &mut replacement,
            &mut tree,
            &renderer,
            Size::new(240.0, 140.0),
        );
        assert!(shape_count() > initial_shapes);
        let after_font = shape_count();
        renderer.draws.clear();
        draw(&replacement, &tree, &mut renderer, node, &node.bounds());
        assert_ne!(
            renderer
                .draws
                .last()
                .expect("reshaped glyph draw")
                .paragraph,
            first_paragraph
        );

        let mut replacement = danmaku(&second_track, &clock)
            .style(translucent)
            .font(Font::MONOSPACE);
        Tree::diff::<(), Theme, RecordingRenderer>(&mut tree, &mut replacement);
        lay_out(
            &mut replacement,
            &mut tree,
            &renderer,
            Size::new(240.0, 140.0),
        );
        assert!(shape_count() > after_font);
        renderer.draws.clear();
        draw(&replacement, &tree, &mut renderer, node, &node.bounds());
        assert_eq!(
            renderer
                .draws
                .last()
                .expect("replacement glyph draw")
                .content,
            "replacement track"
        );
    }

    #[test]
    fn dense_rejections_leave_every_visible_paragraph_drawable() {
        let start = Instant::now();
        let track = Track::new(
            (0..80)
                .map(|index| Comment {
                    at_seconds: 0.0,
                    text: format!("dense {index}"),
                    color: 0xffffff,
                    kind: Kind::Top,
                })
                .collect(),
        )
        .expect("valid dense track");
        let clock = clock(start, false);
        let mut widget = danmaku(&track, &clock).style(Style {
            coverage: 1.0,
            max_visible: 3,
            kinds: [false, true, false],
            ..Style::default()
        });
        let mut renderer = RecordingRenderer::default();
        let mut tree = new_tree(&widget);
        lay_out(&mut widget, &mut tree, &renderer, Size::new(240.0, 66.0));

        let state = tree.state.downcast_ref::<State<RecordingParagraph>>();
        assert_eq!(state.scheduler.visible().len(), 2);
        assert!(state.cache.entries.len() <= 3 + CACHE_NEARBY);
        assert!(state
            .scheduler
            .visible()
            .iter()
            .all(|placement| { state.cache.paragraph(placement.index).is_some() }));

        let node = Layout::new(tree.size);
        draw(&widget, &tree, &mut renderer, node, &node.bounds());
        for content in ["dense 0", "dense 1"] {
            assert!(
                renderer.draws.iter().any(|draw| draw.content == content),
                "visible comment {content:?} lost its cached paragraph"
            );
        }
    }

    #[test]
    fn exhausted_track_rewinds_to_a_confirmed_upcoming_deadline() {
        let track = Track::new(vec![Comment {
            at_seconds: 5.0,
            text: "future fixed".to_owned(),
            color: 0xffffff,
            kind: Kind::Top,
        }])
        .expect("valid future track");
        let start = Instant::now();
        let mut clock = Clock::default();
        assert!(clock.observe(
            ClockSample {
                media_id: 7,
                position_seconds: 20.0,
                speed: 1.0,
                paused: false,
                buffering: false,
                seeking: false,
            },
            start,
        ));
        let style = Style {
            coverage: 1.0,
            kinds: [false, true, false],
            ..Style::default()
        };
        let mut renderer = RecordingRenderer::default();
        let mut tree;
        {
            let mut widget = danmaku(&track, &clock).style(style);
            tree = new_tree(&widget);
            lay_out(&mut widget, &mut tree, &renderer, Size::new(240.0, 140.0));
            let layout = Layout::new(tree.size);
            let exhausted = update(
                &mut widget,
                &mut tree,
                &renderer,
                layout,
                Event::Window(iced::window::Event::RedrawRequested(
                    start + Duration::from_millis(16),
                )),
            );
            assert_eq!(exhausted.redraw, iced::window::RedrawRequest::Wait);
        }

        let rewind = Instant::now();
        assert!(clock.observe(
            ClockSample {
                media_id: 7,
                position_seconds: 4.8,
                speed: 1.0,
                paused: false,
                buffering: false,
                seeking: false,
            },
            rewind,
        ));
        let mut widget = danmaku(&track, &clock).style(style);
        Tree::diff::<(), Theme, RecordingRenderer>(&mut tree, &mut widget);
        lay_out(&mut widget, &mut tree, &renderer, Size::new(240.0, 140.0));
        let node = Layout::new(tree.size);
        let outcome = update(
            &mut widget,
            &mut tree,
            &renderer,
            node,
            Event::Window(iced::window::Event::RedrawRequested(
                rewind + Duration::from_millis(16),
            )),
        );
        let expected = rewind + Duration::from_millis(200);
        let iced::window::RedrawRequest::At(at) = outcome.redraw else {
            panic!("rewound upcoming comment must recover its deadline");
        };
        let error = if at >= expected {
            at - expected
        } else {
            expected - at
        };
        assert!(error <= Duration::from_millis(1));

        let due = update(
            &mut widget,
            &mut tree,
            &renderer,
            node,
            Event::Window(iced::window::Event::RedrawRequested(expected)),
        );
        assert_eq!(due.redraw, iced::window::RedrawRequest::Wait);
        draw(&widget, &tree, &mut renderer, node, &node.bounds());
        assert!(renderer
            .draws
            .iter()
            .any(|draw| draw.content == "future fixed"));
    }
}
