//! Validated danmaku timelines, playback-clock interpolation, and lane scheduling.
//!
//! The scheduler is display-free: callers provide shaped text widths when a
//! comment is admitted and receive geometry for the current playback position.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const MAX_COMMENTS: usize = 100_000;
const MAX_TEXT_BYTES: usize = 4_096;
const MAX_VISIBLE: usize = 256;
const MAX_LANES: usize = 256;
const CLOCK_EXTRAPOLATION: Duration = Duration::from_millis(500);
const FORWARD_RESET_SECONDS: f64 = 1.0;

static NEXT_TRACK_ID: AtomicU64 = AtomicU64::new(1);

/// The presentation behavior of a danmaku comment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// Moves from the right edge to the left edge of the viewport.
    Scroll,
    /// Remains centered in a lane anchored to the top of the viewport.
    Top,
    /// Remains centered in a lane anchored to the bottom of the viewport.
    Bottom,
}

impl Kind {
    const fn index(self) -> usize {
        match self {
            Self::Scroll => 0,
            Self::Top => 1,
            Self::Bottom => 2,
        }
    }
}

/// One timed comment. Colors use `0xRRGGBB`, without an alpha component.
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub at_seconds: f64,
    pub text: String,
    pub color: u32,
    pub kind: Kind,
}

/// A rejected danmaku track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// The input exceeded the bounded track allocation.
    TooManyComments { count: usize, max: usize },
    /// A timestamp was negative or non-finite.
    InvalidTime { index: usize },
    /// A color contained bits outside `0xRRGGBB`.
    InvalidColor { index: usize, color: u32 },
    /// Empty comments have no renderable content.
    EmptyText { index: usize },
    /// A comment exceeded the bounded text allocation.
    TextTooLong {
        index: usize,
        bytes: usize,
        max: usize,
    },
    /// The process exhausted unique track identities.
    TrackIdentityExhausted,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyComments { count, max } => {
                write!(
                    formatter,
                    "danmaku track has {count} comments; maximum is {max}"
                )
            }
            Self::InvalidTime { index } => {
                write!(
                    formatter,
                    "danmaku comment {index} has an invalid timestamp"
                )
            }
            Self::InvalidColor { index, color } => {
                write!(
                    formatter,
                    "danmaku comment {index} has invalid color {color:#x}"
                )
            }
            Self::EmptyText { index } => {
                write!(formatter, "danmaku comment {index} has empty text")
            }
            Self::TextTooLong { index, bytes, max } => write!(
                formatter,
                "danmaku comment {index} has {bytes} text bytes; maximum is {max}"
            ),
            Self::TrackIdentityExhausted => {
                formatter.write_str("danmaku track identities are exhausted")
            }
        }
    }
}

impl std::error::Error for Error {}

/// An immutable, validated timeline sorted by ascending comment time.
///
/// Equal-time comments retain their input order. A track accepts at most
/// 100,000 comments and 4,096 UTF-8 bytes per comment. Unicode is preserved;
/// CRLF, CR, and LF are each normalized to one ASCII space so every comment is
/// a single line.
#[derive(Debug)]
pub struct Track {
    id: u64,
    comments: Vec<Comment>,
}

impl Track {
    /// Validates and sorts a timeline.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] for excessive input, invalid timestamps or colors, and
    /// empty or excessive text. No track identity is consumed on validation
    /// failure.
    pub fn new(mut comments: Vec<Comment>) -> Result<Self, Error> {
        if comments.len() > MAX_COMMENTS {
            return Err(Error::TooManyComments {
                count: comments.len(),
                max: MAX_COMMENTS,
            });
        }

        for (index, comment) in comments.iter_mut().enumerate() {
            if !comment.at_seconds.is_finite() || comment.at_seconds < 0.0 {
                return Err(Error::InvalidTime { index });
            }
            if comment.at_seconds == 0.0 {
                comment.at_seconds = 0.0;
            }
            if comment.color > 0x00ff_ffff {
                return Err(Error::InvalidColor {
                    index,
                    color: comment.color,
                });
            }
            if comment.text.is_empty() {
                return Err(Error::EmptyText { index });
            }
            if comment.text.len() > MAX_TEXT_BYTES {
                return Err(Error::TextTooLong {
                    index,
                    bytes: comment.text.len(),
                    max: MAX_TEXT_BYTES,
                });
            }
            normalize_multiline(&mut comment.text);
        }

        comments.sort_by(|left, right| left.at_seconds.total_cmp(&right.at_seconds));
        let id = NEXT_TRACK_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| Error::TrackIdentityExhausted)?;
        Ok(Self { id, comments })
    }

    /// Returns comments in deterministic timeline order.
    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }

    /// Returns this immutable track's process-unique identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}

fn normalize_multiline(text: &mut String) {
    if !text.bytes().any(|byte| matches!(byte, b'\r' | b'\n')) {
        return;
    }

    let mut normalized = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    let _ = characters.next();
                }
                normalized.push(' ');
            }
            '\n' => normalized.push(' '),
            other => normalized.push(other),
        }
    }
    *text = normalized;
}

/// A confirmed playback-clock observation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockSample {
    pub media_id: u64,
    pub position_seconds: f64,
    pub speed: f64,
    pub paused: bool,
    pub buffering: bool,
    pub seeking: bool,
}

#[derive(Clone, Copy, Debug)]
struct Observation {
    sample: ClockSample,
    observed_at: Instant,
}

/// Monotonic interpolation of confirmed playback samples.
///
/// Paused and buffering samples freeze at their confirmed position. Seeking
/// samples hide the position entirely. Playing samples extrapolate for at most
/// 500 ms and then remain frozen until another confirmed observation arrives.
#[derive(Clone, Debug, Default)]
pub struct Clock {
    observation: Option<Observation>,
}

impl Clock {
    /// Replaces the confirmed observation, returning whether it was accepted.
    ///
    /// Negative or non-finite positions, non-positive or non-finite speeds, and
    /// observations older than the current sample are rejected without changing
    /// clock state. Equal-instant observations are accepted; a media change may
    /// reset the position to any valid value.
    pub fn observe(&mut self, sample: ClockSample, observed_at: Instant) -> bool {
        if !sample.position_seconds.is_finite()
            || sample.position_seconds < 0.0
            || !sample.speed.is_finite()
            || sample.speed <= 0.0
            || self
                .observation
                .is_some_and(|current| observed_at < current.observed_at)
        {
            return false;
        }

        self.observation = Some(Observation {
            sample,
            observed_at,
        });
        true
    }

    /// Returns the interpolated position, or `None` without a sample or while
    /// seeking. Arithmetic saturates at `f64::MAX` rather than producing an
    /// infinite position.
    pub fn position(&self, now: Instant) -> Option<f64> {
        let observation = self.observation?;
        if observation.sample.seeking {
            return None;
        }

        let elapsed = now.saturating_duration_since(observation.observed_at);
        let extrapolated = if observation.sample.paused || observation.sample.buffering {
            0.0
        } else {
            elapsed.min(CLOCK_EXTRAPOLATION).as_secs_f64()
        };
        let position =
            observation.sample.position_seconds + extrapolated * observation.sample.speed;
        Some(if position.is_finite() {
            position
        } else {
            f64::MAX
        })
    }

    /// Returns whether the position is currently advancing rather than paused,
    /// buffering, seeking, or frozen after stale extrapolation.
    pub fn advancing(&self, now: Instant) -> bool {
        let Some(observation) = self.observation else {
            return false;
        };
        !observation.sample.paused
            && !observation.sample.buffering
            && !observation.sample.seeking
            && now.saturating_duration_since(observation.observed_at) < CLOCK_EXTRAPOLATION
    }

    /// Returns when a target position will be reached within the confirmed
    /// sample's 500 ms freshness horizon.
    ///
    /// The delay uses the confirmed speed directly. Paused, buffering, seeking,
    /// stale, invalid, and out-of-horizon requests return `None`. An already
    /// reached target returns `Some(now)` only while the clock is advancing.
    pub fn deadline(&self, position_seconds: f64, now: Instant) -> Option<Instant> {
        if !position_seconds.is_finite() || position_seconds < 0.0 || !self.advancing(now) {
            return None;
        }
        let observation = self.observation?;
        let current = self.position(now)?;
        if position_seconds <= current {
            return Some(now);
        }

        let seconds = (position_seconds - current) / observation.sample.speed;
        let delay = Duration::try_from_secs_f64(seconds).ok()?;
        let deadline = now.checked_add(delay)?;
        let fresh_until = observation.observed_at.checked_add(CLOCK_EXTRAPOLATION)?;
        (deadline <= fresh_until).then_some(deadline)
    }

    /// Returns the confirmed media identity, including while seeking.
    ///
    /// This identity is not retired-request filtering: the clock can accept a
    /// later observation for any media ID. Callers must reject responses from
    /// retired request generations before offering them to [`Clock::observe`].
    pub fn media_id(&self) -> Option<u64> {
        self.observation
            .map(|observation| observation.sample.media_id)
    }
}

/// Viewport and timing inputs to the lane scheduler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub line_height: f32,
    pub top_inset: f32,
    pub bottom_inset: f32,
    pub coverage: f32,
    pub max_visible: usize,
    pub scroll_seconds: f64,
    pub fixed_seconds: f64,
    /// Enables `[Scroll, Top, Bottom]` respectively.
    pub kinds: [bool; 3],
    /// Positive values delay comments; negative values show them earlier.
    pub offset_seconds: f64,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 720.0,
            line_height: 32.0,
            top_inset: 0.0,
            bottom_inset: 0.0,
            coverage: 0.5,
            max_visible: 64,
            scroll_seconds: 8.0,
            fixed_seconds: 5.0,
            kinds: [true; 3],
            offset_seconds: 0.0,
        }
    }
}

/// Current geometry for one comment in [`Track::comments`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SanitizedLayout {
    width: f32,
    height: f32,
    line_height: f32,
    top_inset: f32,
    bottom_inset: f32,
    lane_count: usize,
    max_visible: usize,
    scroll_seconds: f64,
    fixed_seconds: f64,
    kinds: [bool; 3],
    offset_seconds: f64,
}

impl SanitizedLayout {
    fn new(layout: Layout) -> Self {
        let defaults = Layout::default();
        let width = positive_f32_or(layout.width, 0.0);
        let height = positive_f32_or(layout.height, 0.0);
        let line_height = positive_f32_or(layout.line_height, defaults.line_height);
        let top_inset = nonnegative_f32_or(layout.top_inset, 0.0).min(height);
        let bottom_inset = nonnegative_f32_or(layout.bottom_inset, 0.0).min(height);
        let coverage = if layout.coverage.is_finite() {
            layout.coverage.clamp(0.0, 1.0)
        } else {
            defaults.coverage
        };
        let available = (height - top_inset - bottom_inset).max(0.0);
        let lanes = (available * coverage / line_height).floor();
        let lane_count = if lanes >= MAX_LANES as f32 {
            MAX_LANES
        } else {
            lanes as usize
        };

        Self {
            width,
            height,
            line_height,
            top_inset,
            bottom_inset,
            lane_count,
            max_visible: layout.max_visible.min(MAX_VISIBLE),
            scroll_seconds: positive_f64_or(layout.scroll_seconds, defaults.scroll_seconds),
            fixed_seconds: positive_f64_or(layout.fixed_seconds, defaults.fixed_seconds),
            kinds: layout.kinds,
            offset_seconds: if layout.offset_seconds.is_finite() {
                layout.offset_seconds
            } else {
                0.0
            },
        }
    }

    fn lane_y(self, kind: Kind, lane: usize) -> f32 {
        match kind {
            Kind::Bottom => self.height - self.bottom_inset - (lane + 1) as f32 * self.line_height,
            Kind::Scroll | Kind::Top => self.top_inset + lane as f32 * self.line_height,
        }
    }

    fn lifetime(self, kind: Kind) -> f64 {
        match kind {
            Kind::Scroll => self.scroll_seconds,
            Kind::Top | Kind::Bottom => self.fixed_seconds,
        }
    }
}

fn positive_f32_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn nonnegative_f32_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        fallback
    }
}

fn positive_f64_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

#[derive(Clone, Copy, Debug)]
struct Active {
    index: usize,
    kind: Kind,
    y: f32,
    width: f32,
    start: f64,
    expires_at: f64,
}

impl Active {
    fn shares_lane(self, y: f32, line_height: f32) -> bool {
        self.y < y + line_height && y < self.y + line_height
    }

    fn blocks_entry(self, kind: Kind, y: f32, at: f64, layout: &SanitizedLayout) -> bool {
        if !self.shares_lane(y, layout.line_height) {
            return false;
        }
        if self.kind != Kind::Scroll || kind != Kind::Scroll {
            return true;
        }

        let existing_right = scroll_x(
            layout.width,
            self.width,
            self.start,
            layout.scroll_seconds,
            at,
        ) + f64::from(self.width);
        existing_right > f64::from(layout.width)
    }

    fn conflicts_with(self, candidate: &Self, at: f64, layout: &SanitizedLayout) -> bool {
        if self.blocks_entry(candidate.kind, candidate.y, at, layout) {
            return true;
        }
        if self.kind != Kind::Scroll
            || candidate.kind != Kind::Scroll
            || !self.shares_lane(candidate.y, layout.line_height)
        {
            return false;
        }

        // Separation is linear. After the width-independent entry check, the
        // older comment's exit determines whether a wider later comment catches it.
        let candidate_x_at_exit = scroll_x(
            layout.width,
            candidate.width,
            candidate.start,
            layout.scroll_seconds,
            self.expires_at,
        );
        candidate_x_at_exit < 0.0
    }
}

/// Deterministic, allocation-bounded danmaku lane scheduling.
///
/// Comments are admitted only at their shifted timeline time. A comment that
/// cannot immediately obtain a lane is dropped, never queued or delayed. All
/// three kinds share vertical occupancy; fixed comments reserve a complete
/// lane while active, and scrolling comments also check future catch-up
/// collisions. Text is measured only during admission or reconstruction.
///
/// Backward seeks, forward jumps over one second, and track/layout changes
/// trigger a binary-search reconstruction over at most the maximum comment
/// lifetime. Reconstruction replays due events in order and does not emit a
/// late barrage. Because reconstruction starts with an empty active set at the
/// lookback boundary, a seek may reflow marginal lane choices rather than
/// promising bit-for-bit recovery of every earlier forward-only drop.
#[derive(Debug, Default)]
pub struct Scheduler {
    track_id: Option<u64>,
    layout: Option<SanitizedLayout>,
    last_position: Option<f64>,
    next_index: usize,
    active: Vec<Active>,
    visible: Vec<Placement>,
}

impl Scheduler {
    /// Advances or reconstructs placements for a playback position.
    ///
    /// Invalid positions and zero-sized viewports produce no placements.
    /// Non-finite measured widths drop their candidate; negative widths are
    /// treated as zero. Invalid scalar layout values are replaced with defaults
    /// or clamped to safe bounds. Density is capped at 256 visible comments and
    /// 256 candidate lanes.
    pub fn update(
        &mut self,
        track: &Track,
        position_seconds: f64,
        layout: Layout,
        mut measure: impl FnMut(usize, &str) -> f32,
    ) {
        let sanitized = SanitizedLayout::new(layout);
        let mut reset = self.track_id != Some(track.id()) || self.layout != Some(sanitized);
        self.track_id = Some(track.id());
        self.layout = Some(sanitized);

        if !position_seconds.is_finite() || position_seconds < 0.0 {
            self.active.clear();
            self.visible.clear();
            self.next_index = 0;
            self.last_position = None;
            return;
        }

        reset |= match self.last_position {
            None => true,
            Some(previous) => {
                position_seconds < previous || position_seconds - previous > FORWARD_RESET_SECONDS
            }
        };

        self.last_position = Some(position_seconds);
        if reset {
            self.reconstruct(track, position_seconds, sanitized, &mut measure);
        } else {
            self.advance(track, position_seconds, sanitized, &mut measure);
        }
        self.refresh(position_seconds, sanitized);
    }

    /// Returns geometry for the active comments in deterministic admission order.
    pub fn visible(&self) -> &[Placement] {
        &self.visible
    }

    /// Clears timeline state while retaining bounded allocations for reuse.
    pub fn clear(&mut self) {
        self.track_id = None;
        self.layout = None;
        self.last_position = None;
        self.next_index = 0;
        self.active.clear();
        self.visible.clear();
    }

    fn reconstruct(
        &mut self,
        track: &Track,
        position: f64,
        layout: SanitizedLayout,
        measure: &mut impl FnMut(usize, &str) -> f32,
    ) {
        self.active.clear();
        let lookback = position - layout.scroll_seconds.max(layout.fixed_seconds);
        let first = lower_bound(track, layout.offset_seconds, lookback);
        let end = upper_bound(track, layout.offset_seconds, position);
        for index in first..end {
            let at = shifted_at(track, layout.offset_seconds, index);
            self.expire(at);
            self.admit(track, index, at, &layout, measure);
        }
        self.next_index = end;
        self.expire(position);
    }

    fn advance(
        &mut self,
        track: &Track,
        position: f64,
        layout: SanitizedLayout,
        measure: &mut impl FnMut(usize, &str) -> f32,
    ) {
        while self.next_index < track.comments.len() {
            let index = self.next_index;
            let at = shifted_at(track, layout.offset_seconds, index);
            if at > position {
                break;
            }
            self.next_index += 1;
            self.expire(at);
            self.admit(track, index, at, &layout, measure);
        }
        self.expire(position);
    }

    fn has_entry_lane(&self, kind: Kind, at: f64, layout: &SanitizedLayout) -> bool {
        (0..layout.lane_count).any(|lane| {
            let y = layout.lane_y(kind, lane);
            self.active
                .iter()
                .all(|active| !active.blocks_entry(kind, y, at, layout))
        })
    }

    fn admit(
        &mut self,
        track: &Track,
        index: usize,
        at: f64,
        layout: &SanitizedLayout,
        measure: &mut impl FnMut(usize, &str) -> f32,
    ) {
        let Some(comment) = track.comments.get(index) else {
            return;
        };
        if !layout.kinds[comment.kind.index()]
            || self.active.len() >= layout.max_visible
            || layout.lane_count == 0
            || layout.width == 0.0
        {
            return;
        }
        if !self.has_entry_lane(comment.kind, at, layout) {
            return;
        }

        let measured = measure(index, &comment.text);
        if !measured.is_finite() {
            return;
        }
        let width = measured.max(0.0);
        let expires_at = at + layout.lifetime(comment.kind);
        for lane in 0..layout.lane_count {
            let candidate = Active {
                index,
                kind: comment.kind,
                y: layout.lane_y(comment.kind, lane),
                width,
                start: at,
                expires_at,
            };
            if self
                .active
                .iter()
                .all(|active| !active.conflicts_with(&candidate, at, layout))
            {
                self.active.push(candidate);
                return;
            }
        }
    }

    fn expire(&mut self, at: f64) {
        self.active.retain(|active| active.expires_at > at);
    }

    fn refresh(&mut self, position: f64, layout: SanitizedLayout) {
        self.visible.clear();
        for active in &self.active {
            let x = match active.kind {
                Kind::Scroll => scroll_x(
                    layout.width,
                    active.width,
                    active.start,
                    layout.scroll_seconds,
                    position,
                ) as f32,
                Kind::Top | Kind::Bottom => (layout.width - active.width) / 2.0,
            };
            self.visible.push(Placement {
                index: active.index,
                x,
                y: active.y,
                width: active.width,
            });
        }
    }
}

fn scroll_x(width: f32, text_width: f32, start: f64, duration: f64, at: f64) -> f64 {
    let progress = ((at - start) / duration).clamp(0.0, 1.0);
    f64::from(width) - progress * (f64::from(width) + f64::from(text_width))
}

fn shifted_at(track: &Track, offset: f64, index: usize) -> f64 {
    track.comments[index].at_seconds + offset
}

fn lower_bound(track: &Track, offset: f64, target: f64) -> usize {
    let mut left = 0;
    let mut right = track.comments.len();
    while left < right {
        let middle = left + (right - left) / 2;
        if shifted_at(track, offset, middle) < target {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    left
}

fn upper_bound(track: &Track, offset: f64, target: f64) -> usize {
    let mut left = 0;
    let mut right = track.comments.len();
    while left < right {
        let middle = left + (right - left) / 2;
        if shifted_at(track, offset, middle) <= target {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(at_seconds: f64, kind: Kind, text: &str) -> Comment {
        Comment {
            at_seconds,
            text: text.to_owned(),
            color: 0x00ff_ffff,
            kind,
        }
    }

    fn track(comments: Vec<Comment>) -> Track {
        Track::new(comments).unwrap()
    }

    fn layout(width: f32, height: f32) -> Layout {
        Layout {
            width,
            height,
            line_height: 20.0,
            coverage: 1.0,
            max_visible: 16,
            scroll_seconds: 10.0,
            fixed_seconds: 5.0,
            ..Layout::default()
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {expected}, got {actual}"
        );
    }

    fn visible_indices(scheduler: &Scheduler) -> Vec<usize> {
        scheduler
            .visible()
            .iter()
            .map(|placement| placement.index)
            .collect()
    }

    #[test]
    fn track_sorts_stably_and_normalizes_only_line_breaks() {
        let track = track(vec![
            comment(2.0, Kind::Top, "后"),
            comment(1.0, Kind::Scroll, "你好\r\nworld\n🦀"),
            comment(1.0, Kind::Bottom, "同刻"),
        ]);

        assert_eq!(track.comments()[0].text, "你好 world 🦀");
        assert_eq!(track.comments()[1].text, "同刻");
        assert_eq!(track.comments()[2].text, "后");
    }

    #[test]
    fn track_rejects_invalid_times_without_consuming_valid_input() {
        for at_seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01] {
            let error = Track::new(vec![comment(at_seconds, Kind::Scroll, "x")]).unwrap_err();
            assert!(matches!(error, Error::InvalidTime { index: 0 }));
        }
    }

    #[test]
    fn track_rejects_out_of_range_color_with_structured_error() {
        let mut input = comment(0.0, Kind::Scroll, "x");
        input.color = 0x0100_0000;

        assert_eq!(
            Track::new(vec![input]).unwrap_err(),
            Error::InvalidColor {
                index: 0,
                color: 0x0100_0000,
            }
        );
    }

    #[test]
    fn track_enforces_count_and_utf8_text_budgets() {
        let too_many = (0..=MAX_COMMENTS)
            .map(|_| comment(0.0, Kind::Scroll, "x"))
            .collect();
        assert!(matches!(
            Track::new(too_many),
            Err(Error::TooManyComments { .. })
        ));

        let excessive = "界".repeat(MAX_TEXT_BYTES);
        assert!(matches!(
            Track::new(vec![comment(0.0, Kind::Scroll, &excessive)]),
            Err(Error::TextTooLong { index: 0, .. })
        ));
    }

    #[test]
    fn each_validated_track_has_a_distinct_identity() {
        let first = track(Vec::new());
        let second = track(Vec::new());

        assert_ne!(first.id(), second.id());
    }

    fn sample(media_id: u64, position_seconds: f64) -> ClockSample {
        ClockSample {
            media_id,
            position_seconds,
            speed: 1.0,
            paused: false,
            buffering: false,
            seeking: false,
        }
    }

    #[test]
    fn clock_interpolates_confirmed_position_at_playback_speed() {
        let start = Instant::now();
        let mut clock = Clock::default();
        let mut input = sample(7, 10.0);
        input.speed = 2.0;
        assert!(clock.observe(input, start));

        assert_eq!(
            clock.position(start + Duration::from_millis(250)),
            Some(10.5)
        );
        assert!(clock.advancing(start + Duration::from_millis(250)));
    }

    #[test]
    fn clock_freezes_pause_and_buffering_at_the_confirmed_sample() {
        let start = Instant::now();
        for frozen in [true, false] {
            let mut clock = Clock::default();
            let mut input = sample(1, 20.0);
            input.paused = frozen;
            input.buffering = !frozen;
            assert!(clock.observe(input, start));

            assert_eq!(clock.position(start + Duration::from_secs(5)), Some(20.0));
            assert!(!clock.advancing(start + Duration::from_secs(5)));
        }
    }

    #[test]
    fn clock_bounds_stale_extrapolation_then_stops_advancing() {
        let start = Instant::now();
        let mut clock = Clock::default();
        let mut input = sample(1, 3.0);
        input.speed = 2.0;
        assert!(clock.observe(input, start));

        assert_eq!(
            clock.position(start + Duration::from_millis(750)),
            Some(4.0)
        );
        assert_eq!(clock.position(start + Duration::from_secs(10)), Some(4.0));
        assert!(!clock.advancing(start + Duration::from_millis(500)));
    }

    #[test]
    fn clock_rejects_invalid_and_out_of_order_observations_without_mutation() {
        let start = Instant::now();
        let mut clock = Clock::default();
        assert!(clock.observe(sample(1, 10.0), start + Duration::from_secs(1)));

        for invalid in [
            ClockSample {
                position_seconds: f64::NAN,
                ..sample(1, 0.0)
            },
            ClockSample {
                position_seconds: -1.0,
                ..sample(1, 0.0)
            },
            ClockSample {
                speed: 0.0,
                ..sample(1, 0.0)
            },
            ClockSample {
                speed: f64::INFINITY,
                ..sample(1, 0.0)
            },
        ] {
            assert!(!clock.observe(invalid, start + Duration::from_secs(2)));
        }
        assert!(!clock.observe(sample(2, 0.0), start));
        assert_eq!(clock.position(start + Duration::from_secs(2)), Some(10.5));
        assert_eq!(clock.media_id(), Some(1));
    }

    #[test]
    fn clock_media_rollover_accepts_a_reset_position() {
        let start = Instant::now();
        let mut clock = Clock::default();
        assert!(clock.observe(sample(1, 100.0), start));
        assert!(clock.observe(sample(2, 0.0), start + Duration::from_secs(1)));

        assert_eq!(clock.media_id(), Some(2));
        assert_eq!(clock.position(start + Duration::from_secs(2)), Some(0.5));
    }

    #[test]
    fn clock_hides_seeking_geometry_but_retains_media_identity() {
        let start = Instant::now();
        let mut clock = Clock::default();
        let mut input = sample(9, 12.0);
        input.seeking = true;
        assert!(clock.observe(input, start));

        assert_eq!(clock.position(start + Duration::from_millis(100)), None);
        assert!(!clock.advancing(start + Duration::from_millis(100)));
        assert_eq!(clock.media_id(), Some(9));
    }

    #[test]
    fn clock_projection_saturates_instead_of_overflowing() {
        let start = Instant::now();
        let mut clock = Clock::default();
        let mut input = sample(1, f64::MAX);
        input.speed = f64::MAX;
        assert!(clock.observe(input, start));

        assert_eq!(
            clock.position(start + Duration::from_millis(500)),
            Some(f64::MAX)
        );
    }

    #[test]
    fn clock_deadline_uses_confirmed_speed_directly() {
        let start = Instant::now();
        let mut clock = Clock::default();
        let mut input = sample(1, 10.0);
        input.speed = 2.0;
        assert!(clock.observe(input, start));

        assert_eq!(
            clock.deadline(10.5, start),
            Some(start + Duration::from_millis(250))
        );
    }

    #[test]
    fn clock_deadline_returns_now_for_an_already_due_target() {
        let start = Instant::now();
        let mut clock = Clock::default();
        assert!(clock.observe(sample(1, 10.0), start));
        let now = start + Duration::from_millis(100);

        assert_eq!(clock.deadline(10.1, now), Some(now));
    }

    #[test]
    fn clock_deadline_rejects_paused_buffering_and_seeking_samples() {
        let start = Instant::now();
        for state in 0..3 {
            let mut clock = Clock::default();
            let mut input = sample(1, 10.0);
            input.paused = state == 0;
            input.buffering = state == 1;
            input.seeking = state == 2;
            assert!(clock.observe(input, start));

            assert_eq!(clock.deadline(10.1, start), None);
        }
    }

    #[test]
    fn clock_deadline_rejects_stale_clock_even_when_target_is_due() {
        let start = Instant::now();
        let mut clock = Clock::default();
        assert!(clock.observe(sample(1, 0.0), start));
        let stale = start + CLOCK_EXTRAPOLATION;

        assert_eq!(clock.deadline(0.25, stale), None);
    }

    #[test]
    fn clock_deadline_enforces_the_freshness_horizon() {
        let start = Instant::now();
        let mut clock = Clock::default();
        assert!(clock.observe(sample(1, 0.0), start));

        assert_eq!(
            clock.deadline(0.5, start),
            Some(start + CLOCK_EXTRAPOLATION)
        );
        assert_eq!(clock.deadline(0.500_001, start), None);
    }

    #[test]
    fn clock_deadline_rejects_invalid_targets_and_empty_clocks() {
        let start = Instant::now();
        let mut clock = Clock::default();
        assert_eq!(clock.deadline(1.0, start), None);
        assert!(clock.observe(sample(1, 0.0), start));

        for target in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            assert_eq!(clock.deadline(target, start), None);
        }
    }

    #[test]
    fn scroll_geometry_uses_shaped_width_without_remeasuring_frames() {
        let track = track(vec![comment(0.0, Kind::Scroll, "shaped")]);
        let mut scheduler = Scheduler::default();
        let mut measurements = 0;
        scheduler.update(&track, 0.0, layout(400.0, 100.0), |_, _| {
            measurements += 1;
            120.0
        });
        scheduler.update(&track, 1.0, layout(400.0, 100.0), |_, _| {
            measurements += 1;
            120.0
        });

        assert_eq!(measurements, 1);
        assert_eq!(scheduler.visible()[0].width, 120.0);
        assert_close(scheduler.visible()[0].x, 348.0);
    }

    #[test]
    fn simultaneous_scrolls_use_separate_lanes_and_excess_drops() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "a"),
            comment(0.0, Kind::Scroll, "b"),
            comment(0.0, Kind::Scroll, "c"),
        ]);
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, layout(400.0, 40.0), |_, _| 100.0);

        assert_eq!(visible_indices(&scheduler), [0, 1]);
        assert_eq!(scheduler.visible()[0].y, 0.0);
        assert_eq!(scheduler.visible()[1].y, 20.0);
    }

    #[test]
    fn density_limit_drops_new_comments_even_when_lanes_remain() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "a"),
            comment(0.0, Kind::Scroll, "b"),
            comment(0.0, Kind::Scroll, "c"),
        ]);
        let mut settings = layout(400.0, 100.0);
        settings.max_visible = 2;
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, settings, |_, _| 100.0);

        assert_eq!(visible_indices(&scheduler), [0, 1]);
    }

    #[test]
    fn geometrically_blocked_dense_burst_is_not_shaped() {
        let track = track(
            (0..1_000)
                .map(|_| comment(0.0, Kind::Scroll, "dense"))
                .collect(),
        );
        let mut settings = layout(400.0, 160.0);
        settings.max_visible = 64;
        let mut scheduler = Scheduler::default();
        let mut measurements = 0;
        scheduler.update(&track, 0.0, settings, |_, _| {
            measurements += 1;
            100.0
        });

        assert_eq!(scheduler.visible().len(), 8);
        assert_eq!(measurements, 8);
    }

    #[test]
    fn wider_later_comment_is_dropped_before_it_can_catch_leader() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "narrow"),
            comment(1.0, Kind::Scroll, "wide"),
        ]);
        let widths = [20.0, 300.0];
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, layout(400.0, 20.0), |index, _| widths[index]);
        scheduler.update(&track, 1.0, layout(400.0, 20.0), |index, _| widths[index]);

        assert_eq!(visible_indices(&scheduler), [0]);
    }

    #[test]
    fn scroll_top_and_bottom_share_vertical_occupancy() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "scroll"),
            comment(0.0, Kind::Top, "top"),
            comment(0.0, Kind::Bottom, "bottom"),
        ]);
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, layout(400.0, 20.0), |_, _| 100.0);

        assert_eq!(visible_indices(&scheduler), [0]);
    }

    #[test]
    fn fixed_comments_center_in_their_anchored_lanes_and_expire() {
        let track = track(vec![
            comment(0.0, Kind::Top, "top"),
            comment(0.0, Kind::Bottom, "bottom"),
        ]);
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, layout(400.0, 40.0), |_, _| 100.0);

        assert_eq!(scheduler.visible()[0].x, 150.0);
        assert_eq!(scheduler.visible()[0].y, 0.0);
        assert_eq!(scheduler.visible()[1].y, 20.0);
        scheduler.update(&track, 5.0, layout(400.0, 40.0), |_, _| 100.0);
        assert!(scheduler.visible().is_empty());
    }

    #[test]
    fn overloaded_comment_is_not_queued_after_a_lane_frees() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "first"),
            comment(0.1, Kind::Scroll, "late"),
        ]);
        let mut settings = layout(400.0, 20.0);
        settings.scroll_seconds = 1.0;
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, settings, |_, _| 100.0);
        scheduler.update(&track, 0.1, settings, |_, _| 100.0);
        assert_eq!(visible_indices(&scheduler), [0]);

        scheduler.update(&track, 1.1, settings, |_, _| 100.0);
        assert!(scheduler.visible().is_empty());
    }

    #[test]
    fn positive_offset_delays_admission_without_shifting_motion_origin() {
        let track = track(vec![comment(2.0, Kind::Scroll, "offset")]);
        let mut settings = layout(400.0, 20.0);
        settings.offset_seconds = 1.0;
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 2.9, settings, |_, _| 100.0);
        assert!(scheduler.visible().is_empty());

        scheduler.update(&track, 3.0, settings, |_, _| 100.0);
        assert_eq!(scheduler.visible()[0].x, 400.0);
        scheduler.update(&track, 3.5, settings, |_, _| 100.0);
        assert_close(scheduler.visible()[0].x, 375.0);
    }

    #[test]
    fn backward_seek_and_repeat_reconstruct_current_geometry() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "zero"),
            comment(1.0, Kind::Scroll, "one"),
            comment(2.0, Kind::Scroll, "two"),
        ]);
        let settings = layout(400.0, 60.0);
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, settings, |_, _| 100.0);
        scheduler.update(&track, 1.0, settings, |_, _| 100.0);
        scheduler.update(&track, 2.2, settings, |_, _| 100.0);
        scheduler.update(&track, 1.2, settings, |_, _| 100.0);

        assert_eq!(visible_indices(&scheduler), [0, 1]);
        assert_close(scheduler.visible()[0].x, 340.0);
        assert_close(scheduler.visible()[1].x, 390.0);

        scheduler.update(&track, 0.0, settings, |_, _| 100.0);
        assert_eq!(visible_indices(&scheduler), [0]);
        assert_eq!(scheduler.visible()[0].x, 400.0);
    }

    #[test]
    fn forward_jump_reconstructs_instead_of_emitting_late_barrage() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "old"),
            comment(20.0, Kind::Scroll, "current"),
        ]);
        let settings = layout(400.0, 20.0);
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, settings, |_, _| 100.0);
        scheduler.update(&track, 21.0, settings, |_, _| 100.0);

        assert_eq!(visible_indices(&scheduler), [1]);
        assert_close(scheduler.visible()[0].x, 350.0);
    }

    #[test]
    fn frame_cadence_does_not_change_normal_forward_geometry() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "zero"),
            comment(0.2, Kind::Scroll, "two"),
            comment(0.4, Kind::Scroll, "four"),
        ]);
        let settings = layout(400.0, 100.0);
        let mut stepped = Scheduler::default();
        let mut jumped = Scheduler::default();
        for position in [0.0, 0.2, 0.4, 0.8] {
            stepped.update(&track, position, settings, |_, _| 100.0);
        }
        jumped.update(&track, 0.0, settings, |_, _| 100.0);
        jumped.update(&track, 0.8, settings, |_, _| 100.0);

        assert_eq!(stepped.visible(), jumped.visible());
    }

    #[test]
    fn resize_reconstructs_with_new_viewport_geometry() {
        let track = track(vec![comment(0.0, Kind::Scroll, "resize")]);
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, layout(400.0, 20.0), |_, _| 100.0);
        scheduler.update(&track, 1.0, layout(800.0, 20.0), |_, _| 100.0);

        assert_close(scheduler.visible()[0].x, 710.0);
    }

    #[test]
    fn timing_settings_change_reconstructs_motion() {
        let track = track(vec![comment(0.0, Kind::Scroll, "speed")]);
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, layout(400.0, 20.0), |_, _| 100.0);
        let mut faster = layout(400.0, 20.0);
        faster.scroll_seconds = 5.0;
        scheduler.update(&track, 1.0, faster, |_, _| 100.0);

        assert_close(scheduler.visible()[0].x, 300.0);
    }

    #[test]
    fn track_change_at_same_position_replaces_old_active_indices() {
        let first = track(vec![comment(0.0, Kind::Scroll, "a")]);
        let second = track(vec![comment(0.0, Kind::Scroll, "1234567890")]);
        let settings = layout(400.0, 20.0);
        let mut scheduler = Scheduler::default();
        scheduler.update(&first, 1.0, settings, |_, text| text.len() as f32 * 10.0);
        assert_eq!(scheduler.visible()[0].width, 10.0);

        scheduler.update(&second, 1.0, settings, |_, text| text.len() as f32 * 10.0);
        assert_eq!(scheduler.visible()[0].width, 100.0);
        assert_close(scheduler.visible()[0].x, 350.0);
    }

    #[test]
    fn disabled_kinds_are_not_measured_or_admitted() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "hidden"),
            comment(0.0, Kind::Top, "shown"),
        ]);
        let mut settings = layout(400.0, 40.0);
        settings.kinds = [false, true, false];
        let mut scheduler = Scheduler::default();
        let mut measured = Vec::new();
        scheduler.update(&track, 0.0, settings, |index, _| {
            measured.push(index);
            100.0
        });

        assert_eq!(measured, [1]);
        assert_eq!(visible_indices(&scheduler), [1]);
    }

    #[test]
    fn zero_sized_viewport_drops_work_and_produces_no_geometry() {
        let track = track(vec![comment(0.0, Kind::Scroll, "hidden")]);
        let mut settings = layout(400.0, 20.0);
        settings.width = 0.0;
        let mut scheduler = Scheduler::default();
        let mut measurements = 0;
        scheduler.update(&track, 0.0, settings, |_, _| {
            measurements += 1;
            100.0
        });

        assert!(scheduler.visible().is_empty());
        assert_eq!(measurements, 0);
    }

    #[test]
    fn invalid_layout_scalars_fall_back_to_finite_geometry() {
        let track = track(vec![comment(0.0, Kind::Scroll, "safe")]);
        let settings = Layout {
            width: 400.0,
            height: 100.0,
            line_height: f32::NAN,
            coverage: f32::NAN,
            scroll_seconds: f64::NAN,
            fixed_seconds: -1.0,
            offset_seconds: f64::NAN,
            ..Layout::default()
        };
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, settings, |_, _| 100.0);

        assert_eq!(scheduler.visible().len(), 1);
        let placement = scheduler.visible()[0];
        assert!(placement.x.is_finite() && placement.y.is_finite());
    }

    #[test]
    fn unchanged_nan_layout_does_not_reconstruct_or_revive_dropped_comments() {
        let track = track(vec![
            comment(0.0, Kind::Top, "first"),
            comment(0.1, Kind::Top, "dropped"),
        ]);
        let mut settings = layout(400.0, 40.0);
        settings.coverage = f32::NAN;
        settings.scroll_seconds = 1.0;
        settings.fixed_seconds = 1.0;
        let mut scheduler = Scheduler::default();
        let mut measurements = 0;
        for position in [0.0, 0.1, 1.05] {
            scheduler.update(&track, position, settings, |_, _| {
                measurements += 1;
                100.0
            });
        }

        assert!(scheduler.visible().is_empty());
        assert_eq!(measurements, 1);
    }

    #[test]
    fn invalid_measurements_cannot_create_non_finite_geometry() {
        let track = track(vec![
            comment(0.0, Kind::Scroll, "nan"),
            comment(0.0, Kind::Scroll, "infinite"),
            comment(0.0, Kind::Scroll, "negative"),
        ]);
        let widths = [f32::NAN, f32::INFINITY, -10.0];
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, layout(400.0, 60.0), |index, _| widths[index]);

        assert_eq!(visible_indices(&scheduler), [2]);
        assert_eq!(scheduler.visible()[0].width, 0.0);
    }

    #[test]
    fn invalid_position_clears_state_and_next_valid_update_reconstructs() {
        let track = track(vec![comment(0.0, Kind::Scroll, "safe")]);
        let settings = layout(400.0, 20.0);
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 0.0, settings, |_, _| 100.0);
        scheduler.update(&track, f64::NAN, settings, |_, _| 100.0);
        assert!(scheduler.visible().is_empty());

        scheduler.update(&track, 1.0, settings, |_, _| 100.0);
        assert_eq!(visible_indices(&scheduler), [0]);
        assert_close(scheduler.visible()[0].x, 350.0);
    }

    #[test]
    fn clear_requires_fresh_reconstruction_but_retains_scheduler_reuse() {
        let track = track(vec![comment(0.0, Kind::Scroll, "again")]);
        let settings = layout(400.0, 20.0);
        let mut scheduler = Scheduler::default();
        scheduler.update(&track, 1.0, settings, |_, _| 100.0);
        scheduler.clear();
        assert!(scheduler.visible().is_empty());

        scheduler.update(&track, 1.0, settings, |_, _| 100.0);
        assert_close(scheduler.visible()[0].x, 350.0);
    }
}
