//! Now Playing seek/volume adjustment policy shared by the native player bar
//! and the embedded player's transport controls.
//!
//! [`Adjustments`] owns the per-channel draft (slider preview) and committed
//! target for both controls. Dragging a slider stages a draft; releasing it
//! emits one command and keeps the draft on screen until the controller
//! settles. Absolute adjustments emit immediately, and relative steps
//! accumulate against the last committed target so repeated presses do not
//! race a lagging transport position. Command execution, feedback text, and
//! hover state stay with the caller.

/// A Now Playing control that accepts position adjustments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Control {
    /// Timeline position, in seconds.
    Seek,
    /// Output volume, in MPV's 0-100 range.
    Volume,
}

/// One adjustment input from a presentation adapter or the session lifecycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// A slider drag began; the channel stages a fresh draft.
    DragStarted(Control),
    /// The slider's current draft value. An invalid value clears the draft.
    Preview(Control, f64),
    /// The drag ended; emits the staged draft when it still validates. A
    /// release without a preceding `DragStarted` is supported.
    Release(Control),
    /// An absolute target (keyboard or wheel); emits immediately.
    Adjust(Control, f64),
    /// A relative step from the committed target, or from the transport
    /// position when nothing is committed. Updates only the committed
    /// target, never the visible draft.
    Step(Control, f64),
    /// Every drag was cancelled; staged drafts drop while committed targets
    /// stay, since commands already sent cannot be unsent.
    CancelDrags,
    /// The session replaced or retired playback; all adjustment state resets.
    Replace,
    /// The controller settled. While `busy`, drafts stay on screen; otherwise
    /// drafts on channels that are not being dragged retire.
    ControllerSettled { busy: bool },
    /// The embedded presentation's activity and transport settlement.
    /// Inactive always clears drag flags, the active-to-inactive transition
    /// retires committed targets, and an active settled transport retires
    /// them as well. Drafts are never cleared here.
    Presentation { active: bool, settled: bool },
}

/// The transport values an adjustment validates and accumulates against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Values {
    /// Current transport position in seconds.
    pub position_seconds: f64,
    /// Known media duration in seconds; seeks need a positive finite one.
    pub duration_seconds: Option<f64>,
    /// Current volume in MPV's 0-100 range.
    pub volume: f64,
}

/// A validated adjustment for the caller to execute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// Absolute seek target in seconds, clamped to `[0, duration]`.
    Seek(f64),
    /// Absolute volume target, clamped to `[0, 100]`.
    SetVolume(f64),
}

/// The projected adjustment state a presentation layer draws from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct View {
    /// A pointer seek gesture is active.
    pub seek_dragging: bool,
    /// A volume drag is in flight.
    pub volume_dragging: bool,
    /// The staged or committed seek target while it outranks the transport.
    pub seek_preview: Option<f64>,
    /// The staged or committed volume target while it outranks the transport.
    pub volume_preview: Option<f64>,
}

/// Per-channel draft and committed target. The draft is what a drag or an
/// absolute adjustment staged; the committed target is what relative steps
/// accumulate against while the transport catches up.
#[derive(Debug, Default)]
struct Channel {
    dragging: bool,
    preview: Option<f64>,
    desired: Option<f64>,
}

/// Owns seek/volume adjustment state. Pure scalar policy: no allocation, no
/// clocks, no command queue — each input reduces to at most one [`Command`].
#[derive(Debug, Default)]
pub struct Adjustments {
    seek: Channel,
    volume: Channel,
    /// Whether the embedded presentation was active at the last
    /// [`Input::Presentation`]; the active-to-inactive edge retires committed
    /// targets.
    presentation_active: bool,
}

impl Adjustments {
    /// Reduces one input against the current transport projection. `playing`
    /// is `None` while Now Playing is empty, which rejects every command but
    /// still lets lifecycle inputs retire state. Returns the command to
    /// execute when the input produced one.
    pub fn handle(&mut self, input: Input, playing: Option<Values>) -> Option<Command> {
        match input {
            Input::DragStarted(control) => {
                let channel = self.channel_mut(control);
                channel.dragging = true;
                channel.preview = None;
                None
            }
            Input::Preview(control, value) => {
                self.channel_mut(control).preview = validated(control, value, playing);
                None
            }
            Input::Release(control) => {
                let channel = self.channel_mut(control);
                channel.dragging = false;
                let preview = channel.preview?;
                channel.desired = Some(preview);
                validated(control, preview, playing).map(|target| command(control, target))
            }
            Input::Adjust(control, value) => {
                let target = validated(control, value, playing)?;
                let channel = self.channel_mut(control);
                channel.preview = Some(target);
                channel.desired = Some(target);
                Some(command(control, target))
            }
            Input::Step(control, delta) => {
                let playing = playing?;
                let channel = self.channel_mut(control);
                let base = channel.desired.unwrap_or(match control {
                    Control::Seek => playing.position_seconds,
                    Control::Volume => playing.volume,
                });
                let target = validated(control, base + delta, Some(playing))?;
                channel.desired = Some(target);
                Some(command(control, target))
            }
            Input::CancelDrags => {
                for channel in [&mut self.seek, &mut self.volume] {
                    channel.dragging = false;
                    channel.preview = None;
                }
                None
            }
            Input::Replace => {
                *self = Self::default();
                None
            }
            Input::ControllerSettled { busy } => {
                if !busy {
                    if !self.seek.dragging {
                        self.seek.preview = None;
                    }
                    if !self.volume.dragging {
                        self.volume.preview = None;
                    }
                }
                None
            }
            Input::Presentation { active, settled } => {
                if !active {
                    self.seek.dragging = false;
                    self.volume.dragging = false;
                }
                if (self.presentation_active && !active) || (active && settled) {
                    self.seek.desired = None;
                    self.volume.desired = None;
                }
                self.presentation_active = active;
                None
            }
        }
    }

    /// The projected draft/drag state for the presentation layer.
    pub fn view(&self) -> View {
        View {
            seek_dragging: self.seek.dragging,
            volume_dragging: self.volume.dragging,
            seek_preview: self.seek.preview,
            volume_preview: self.volume.preview,
        }
    }

    fn channel_mut(&mut self, control: Control) -> &mut Channel {
        match control {
            Control::Seek => &mut self.seek,
            Control::Volume => &mut self.volume,
        }
    }
}

/// The clamped target for one value, or `None` when the adjustment is not
/// meaningful: playback inactive, the value non-finite, or a seek without a
/// positive finite duration.
fn validated(control: Control, value: f64, playing: Option<Values>) -> Option<f64> {
    let playing = playing?;
    match control {
        Control::Seek => {
            let duration = playing
                .duration_seconds
                .filter(|duration| duration.is_finite() && *duration > 0.0)?;
            value.is_finite().then(|| value.clamp(0.0, duration))
        }
        Control::Volume => value.is_finite().then(|| value.clamp(0.0, 100.0)),
    }
}

fn command(control: Control, value: f64) -> Command {
    match control {
        Control::Seek => Command::Seek(value),
        Control::Volume => Command::SetVolume(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(position_seconds: f64, duration_seconds: Option<f64>, volume: f64) -> Values {
        Values {
            position_seconds,
            duration_seconds,
            volume,
        }
    }

    #[test]
    fn release_commits_the_target_and_steps_accumulate_over_a_lagging_transport() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        let _ = adjustments.handle(Input::DragStarted(Control::Seek), Some(playing));
        let _ = adjustments.handle(Input::Preview(Control::Seek, 42.0), Some(playing));
        assert_eq!(
            adjustments.handle(Input::Release(Control::Seek), Some(playing)),
            Some(Command::Seek(42.0))
        );
        // The transport still reports the pre-seek position; steps accumulate
        // from the committed target instead of snapping back.
        assert_eq!(
            adjustments.handle(Input::Step(Control::Seek, 5.0), Some(playing)),
            Some(Command::Seek(47.0))
        );
        assert_eq!(
            adjustments.handle(Input::Step(Control::Seek, 5.0), Some(playing)),
            Some(Command::Seek(52.0))
        );
    }

    #[test]
    fn settlement_retires_only_the_drafts_of_channels_not_being_dragged() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        let _ = adjustments.handle(Input::DragStarted(Control::Seek), Some(playing));
        let _ = adjustments.handle(Input::Preview(Control::Seek, 30.0), Some(playing));
        let _ = adjustments.handle(Input::Preview(Control::Volume, 80.0), Some(playing));
        assert_eq!(
            adjustments.handle(Input::ControllerSettled { busy: false }, Some(playing)),
            None
        );
        let view = adjustments.view();
        assert_eq!(view.seek_preview, Some(30.0));
        assert!(view.seek_dragging);
        assert_eq!(view.volume_preview, None);
    }

    #[test]
    fn committed_target_survives_busy_settlements_until_the_transport_settles() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        let _ = adjustments.handle(Input::Preview(Control::Seek, 42.0), Some(playing));
        assert_eq!(
            adjustments.handle(Input::Release(Control::Seek), Some(playing)),
            Some(Command::Seek(42.0))
        );
        // A busy settlement keeps the pending draft on screen and the
        // committed target accumulating.
        let _ = adjustments.handle(Input::ControllerSettled { busy: true }, Some(playing));
        assert_eq!(adjustments.view().seek_preview, Some(42.0));
        assert_eq!(
            adjustments.handle(Input::Step(Control::Seek, 3.0), Some(playing)),
            Some(Command::Seek(45.0))
        );
        // Once the transport settles the committed target retires; the next
        // step bases on the reported position again.
        let _ = adjustments.handle(
            Input::Presentation {
                active: true,
                settled: true,
            },
            Some(playing),
        );
        assert_eq!(
            adjustments.handle(Input::Step(Control::Seek, 3.0), Some(playing)),
            Some(Command::Seek(13.0))
        );
    }

    #[test]
    fn cancelled_drag_makes_a_late_release_a_no_op_but_keeps_committed_targets() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        assert_eq!(
            adjustments.handle(Input::Adjust(Control::Volume, 60.0), Some(playing)),
            Some(Command::SetVolume(60.0))
        );
        let _ = adjustments.handle(Input::DragStarted(Control::Seek), Some(playing));
        let _ = adjustments.handle(Input::Preview(Control::Seek, 30.0), Some(playing));
        let _ = adjustments.handle(Input::CancelDrags, Some(playing));
        assert_eq!(adjustments.view(), View::default());
        assert_eq!(
            adjustments.handle(Input::Release(Control::Seek), Some(playing)),
            None
        );
        // The committed volume target survives cancellation: already sent
        // commands cannot be unsent, so steps keep accumulating from it.
        assert_eq!(
            adjustments.handle(Input::Step(Control::Volume, 5.0), Some(playing)),
            Some(Command::SetVolume(65.0))
        );
    }

    #[test]
    fn replace_discards_drafts_and_committed_targets() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        let _ = adjustments.handle(Input::DragStarted(Control::Seek), Some(playing));
        let _ = adjustments.handle(Input::Preview(Control::Seek, 42.0), Some(playing));
        assert_eq!(
            adjustments.handle(Input::Release(Control::Seek), Some(playing)),
            Some(Command::Seek(42.0))
        );
        let _ = adjustments.handle(Input::Replace, Some(playing));
        assert_eq!(adjustments.view(), View::default());
        // The old target is gone: a step accumulates from the new transport
        // position, not from the replaced item's committed target.
        let next = values(5.0, Some(200.0), 40.0);
        assert_eq!(
            adjustments.handle(Input::Step(Control::Seek, 5.0), Some(next)),
            Some(Command::Seek(10.0))
        );
    }

    #[test]
    fn seek_requires_active_playback_and_a_positive_finite_duration() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        for (input, playing) in [
            (Input::Adjust(Control::Seek, 50.0), None),
            (
                Input::Adjust(Control::Seek, 50.0),
                Some(values(10.0, None, 40.0)),
            ),
            (
                Input::Adjust(Control::Seek, 50.0),
                Some(values(10.0, Some(0.0), 40.0)),
            ),
            (
                Input::Adjust(Control::Seek, 50.0),
                Some(values(10.0, Some(f64::NAN), 40.0)),
            ),
            (Input::Adjust(Control::Seek, f64::NAN), Some(playing)),
            (Input::Step(Control::Seek, 5.0), None),
        ] {
            assert_eq!(adjustments.handle(input, playing), None);
        }
        assert_eq!(adjustments.view(), View::default());
        // Targets clamp into the timeline instead of being rejected.
        assert_eq!(
            adjustments.handle(Input::Adjust(Control::Seek, -5.0), Some(playing)),
            Some(Command::Seek(0.0))
        );
        assert_eq!(
            adjustments.handle(Input::Adjust(Control::Seek, 150.0), Some(playing)),
            Some(Command::Seek(100.0))
        );
    }

    #[test]
    fn volume_requires_active_playback_and_a_finite_value() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        assert_eq!(
            adjustments.handle(Input::Adjust(Control::Volume, 50.0), None),
            None
        );
        assert_eq!(
            adjustments.handle(Input::Adjust(Control::Volume, f64::NAN), Some(playing)),
            None
        );
        assert_eq!(
            adjustments.handle(Input::Adjust(Control::Volume, -5.0), Some(playing)),
            Some(Command::SetVolume(0.0))
        );
        assert_eq!(
            adjustments.handle(Input::Adjust(Control::Volume, 125.0), Some(playing)),
            Some(Command::SetVolume(100.0))
        );
    }

    #[test]
    fn preview_and_release_without_drag_start_still_commit() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        let _ = adjustments.handle(Input::Preview(Control::Seek, 25.0), Some(playing));
        assert_eq!(
            adjustments.handle(Input::Release(Control::Seek), Some(playing)),
            Some(Command::Seek(25.0))
        );
        assert!(!adjustments.view().seek_dragging);
        assert_eq!(adjustments.view().seek_preview, Some(25.0));
    }

    #[test]
    fn invalid_preview_clears_the_staged_draft() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        let _ = adjustments.handle(Input::DragStarted(Control::Seek), Some(playing));
        let _ = adjustments.handle(Input::Preview(Control::Seek, 30.0), Some(playing));
        let _ = adjustments.handle(Input::Preview(Control::Seek, f64::NAN), Some(playing));
        assert_eq!(adjustments.view().seek_preview, None);
        assert_eq!(
            adjustments.handle(Input::Release(Control::Seek), Some(playing)),
            None
        );
    }

    #[test]
    fn presentation_inactive_clears_drags_and_only_the_transition_retires_targets() {
        let mut adjustments = Adjustments::default();
        let playing = values(10.0, Some(100.0), 40.0);
        let _ = adjustments.handle(
            Input::Presentation {
                active: true,
                settled: false,
            },
            Some(playing),
        );
        let _ = adjustments.handle(Input::DragStarted(Control::Seek), Some(playing));
        let _ = adjustments.handle(Input::Preview(Control::Seek, 42.0), Some(playing));
        assert_eq!(
            adjustments.handle(Input::Adjust(Control::Volume, 60.0), Some(playing)),
            Some(Command::SetVolume(60.0))
        );
        // Going inactive drops the drag flags but keeps drafts and, on the
        // transition edge, retires committed targets.
        let _ = adjustments.handle(
            Input::Presentation {
                active: false,
                settled: false,
            },
            Some(playing),
        );
        let view = adjustments.view();
        assert!(!view.seek_dragging);
        assert_eq!(view.seek_preview, Some(42.0));
        assert_eq!(view.volume_preview, Some(60.0));
        // The edge already retired the targets; staying inactive is
        // idempotent and a step bases on the transport again.
        let _ = adjustments.handle(
            Input::Presentation {
                active: false,
                settled: false,
            },
            Some(playing),
        );
        assert_eq!(
            adjustments.handle(Input::Step(Control::Volume, 5.0), Some(playing)),
            Some(Command::SetVolume(45.0))
        );
    }
}
