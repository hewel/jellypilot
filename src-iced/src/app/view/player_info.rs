//! Native, demand-owned playback information; independent of MPV's stats OSD.

use std::sync::LazyLock;

use iced::advanced::graphics::text::{cosmic_text::fontdb, font_system};
use iced::widget::{column, container, row, scrollable, text, Column};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_mpv::statistics::PlaybackStatistics;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::overlay::{
  focus_tooltip, popover, PopoverAppearance, PopoverOptions, TooltipOptions,
};
use jellypilot_ui::tokens::{DARK_PALETTE, TOKENS};
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::control_button::control_button;
use jellypilot_ui::widgets::embedded_player as cinema;

use crate::app::embedded_player;
use crate::app::message::Message;
use crate::app::state::State;
use jellypilot_ui::fonts::MONO_FONT;

pub(super) fn button(state: &State) -> Element<'_, Message> {
  let open = embedded_player::information_open(state);
  let trigger =
    focus_tooltip(
      control_button(
        Some(Icon::Info),
        None,
        if open {
          ButtonVariant::TonalActive
        } else {
          ButtonVariant::Tonal
        },
      )
      .style(cinema::top_control)
      .icon_size(IconSize::Custom(16.0))
      .padding(0)
      .width(Length::Fixed(40.0))
      .min_height(40.0)
      .content_centered(true)
      .on_press_maybe((!state.playback.view.lifecycle.replacing).then_some(
        Message::EmbeddedPlayer(embedded_player::Message::InformationToggled),
      )),
      state.t("player-information"),
      TooltipOptions::default(),
    );
  // Closed popovers retain their widget tree, but do not format unseen statistics.
  let content = if open { panel(state) } else { column![].into() };
  popover(
    trigger,
    content,
    open,
    PopoverOptions {
      width: Some(380.0_f32.min((state.shell.window_size.width - 24.0).max(1.0))),
      alignment: jellypilot_ui::overlay::Alignment::End,
      appearance: PopoverAppearance::PlaybackInformation,
      ..PopoverOptions::default()
    },
    Message::EmbeddedPlayer(embedded_player::Message::InformationDismissed),
  )
}

fn information_font() -> iced::Font {
  // A generic monospace family makes cosmic-text search every installed
  // monospace face for words containing CJK or localization isolation marks.
  // Resolve the primary face once; named families use the normal Unicode fallback.
  static FAMILY: LazyLock<Option<String>> = LazyLock::new(|| {
    let mut system = font_system().write().ok()?;
    let db = system.raw().db();
    let id = db
      .query(&fontdb::Query {
        families: &[fontdb::Family::Monospace],
        ..fontdb::Query::default()
      })
      .or_else(|| {
        db.faces()
          .find(|face| face.monospaced && face.weight == fontdb::Weight::NORMAL)
          .map(|face| face.id)
      })?;
    db.face(id)?
      .families
      .first()
      .map(|(family, _)| family.clone())
  });
  FAMILY.as_deref().map_or(MONO_FONT, iced::Font::new)
}

fn panel(state: &State) -> Element<'_, Message> {
  let sample = embedded_player::information(state);
  let filename = sample.and_then(|sample| sample.filename.as_deref());
  let title = state
    .playback
    .view
    .now_playing
    .as_ref()
    .map(|now| now.item.title.as_str())
    .unwrap_or("");
  let identity = column![
    text(
      filename
        .map(str::to_owned)
        .unwrap_or_else(|| state.t("player-information-filename-unavailable"))
    )
    .font(information_font())
    .size(11)
    .line_height(iced::Pixels(15.0))
    .wrapping(text::Wrapping::Glyph)
    .color(DARK_PALETTE.text.heading),
    text(title)
      .size(12)
      .line_height(iced::Pixels(14.0))
      .color(DARK_PALETTE.text.metadata),
  ]
  .spacing(3)
  .width(Fill);
  let close = focus_tooltip(
    control_button(Some(Icon::Close), None, ButtonVariant::Tonal)
      .style(cinema::control)
      .icon_size(IconSize::Custom(14.0))
      .padding(0)
      .width(Length::Fixed(40.0))
      .min_height(40.0)
      .content_centered(true)
      .on_press(Message::EmbeddedPlayer(
        embedded_player::Message::InformationDismissed,
      )),
    state.t("common-close"),
    TooltipOptions::default(),
  );
  let mut actions = row![].spacing(4).align_y(Alignment::Center);
  if let Some(method) =
    state
      .playback
      .view
      .now_playing
      .as_ref()
      .and_then(|now| match now.item.play_method.as_str() {
        "DirectPlay" => Some("player-information-direct-play"),
        "DirectStream" => Some("player-information-direct-stream"),
        _ => None,
      })
  {
    actions = actions.push(cinema::play_method_badge(
      state.t(method),
      information_font(),
    ));
  }
  actions = actions.push(close);
  let header = container(
    row![identity, actions]
      .spacing(10)
      .align_y(Alignment::Start),
  )
  .padding(iced::Padding {
    top: 6.0,
    right: 10.0,
    bottom: 8.0,
    left: 10.0,
  });
  let body = if let Some(sample) = sample {
    statistics(state, sample)
  } else {
    column![text(state.t(if embedded_player::information_failed(state) {
      "player-information-unavailable"
    } else {
      "player-information-loading"
    }))
    .size(12)
    .color(DARK_PALETTE.text.metadata)]
  };
  // Scroll the entire panel so even very short windows retain a reachable Close.
  container(
    scrollable(column![
      header,
      container(text(""))
        .height(1)
        .width(Fill)
        .style(cinema::separator),
      body
    ])
    .height(Length::Shrink),
  )
  .height(Length::Fit.max((state.shell.window_size.height - 104.0).max(40.0)))
  .width(Fill)
  .into()
}

fn statistics<'a>(state: &'a State, sample: &'a PlaybackStatistics) -> Column<'a, Message> {
  let output = &sample.output;
  let mut content = column![
    section(state, "player-information-file", Icon::Folder),
    stat_row(
      state,
      "player-information-size",
      join([
        sample.file_size_bytes.map(bytes),
        sample.container_format.clone(),
      ])
    ),
    stat_row(
      state,
      "player-information-cache",
      join([
        sample
          .cache
          .total_bytes
          .or(sample.cache.forward_bytes)
          .map(bytes),
        sample.cache.duration_seconds.map(|seconds| state.format(
          "player-information-seconds",
          &[("seconds", format!("{seconds:.1}").into())]
        )),
      ])
    ),
    section(state, "player-information-display", Icon::Tv),
    stat_row(
      state,
      "player-information-output",
      join([output.vo.clone(), output.gpu_context.clone()])
    ),
    stat_row(
      state,
      "player-information-refresh",
      join([
        output.display_fps.map(|value| format!("{value:.3} Hz")),
        output.av_sync_seconds.map(|value| state.format(
          "player-information-av",
          &[("offset", format!("{:+.1}", value * 1000.0).into())]
        )),
      ])
    ),
    stat_row(
      state,
      "player-information-drops",
      Some(
        state.format(
          "player-information-drop-values",
          &[
            (
              "decoder",
              output
                .decoder_dropped_frames
                .map(|value| value.to_string())
                .unwrap_or_else(|| state.t("player-information-value-unavailable"))
                .into()
            ),
            (
              "output",
              output
                .output_dropped_frames
                .map(|value| value.to_string())
                .unwrap_or_else(|| state.t("player-information-value-unavailable"))
                .into()
            ),
          ]
        )
      )
    ),
  ];
  let timing = output
    .pass_timings
    .iter()
    .find(|timing| timing.frame_type == "fresh")
    .or_else(|| output.pass_timings.first());
  content = content.push(focus_tooltip(
    stat_row(
      state,
      "player-information-timing",
      timing.map(|timing| {
        format!(
          "{:.0} / {:.0} / {:.0} µs",
          timing.last_ns as f64 / 1000.0,
          timing.avg_ns as f64 / 1000.0,
          timing.peak_ns as f64 / 1000.0
        )
      }),
    ),
    state.t("player-information-timing-hint"),
    TooltipOptions::default(),
  ));
  content = content.push(section(state, "player-information-video", Icon::Movie));
  let video = sample.video.as_ref();
  content = content
    .push(stat_row(
      state,
      "player-information-codec",
      video.and_then(|video| {
        join([
          video.codec.clone(),
          video.codec_profile.clone(),
          video.hwdec.as_ref().map(|decoder| {
            if decoder == "no" {
              state.t("player-information-software")
            } else {
              decoder.clone()
            }
          }),
        ])
      }),
    ))
    .push(stat_row(
      state,
      "player-information-format",
      video.and_then(|video| {
        join([
          video
            .width
            .zip(video.height)
            .map(|(width, height)| format!("{width} × {height}")),
          video.container_fps.map(|fps| format!("{fps:.3} fps")),
        ])
      }),
    ))
    .push(stat_row(
      state,
      "player-information-color",
      video.and_then(|video| {
        join([
          video.primaries.clone(),
          video.gamma.clone(),
          video
            .hw_pixel_format
            .clone()
            .or_else(|| video.pixel_format.clone()),
          video
            .dolby_vision_profile
            .map(|profile| format!("Dolby Vision P{profile}")),
        ])
      }),
    ))
    .push(stat_row(
      state,
      "player-information-bitrate",
      video.and_then(|video| video.bitrate).map(bitrate),
    ))
    .push(section(state, "player-information-audio", Icon::AudioTrack));
  let audio = sample.audio.as_ref();
  content
    .push(stat_row(
      state,
      "player-information-track",
      audio.and_then(|audio| {
        join([
          audio.language.clone(),
          audio.codec.clone(),
          audio.channels.clone(),
          audio.bitrate.map(bitrate),
        ])
      }),
    ))
    .push(stat_row(
      state,
      "player-information-output",
      audio.and_then(|audio| {
        join([
          audio.ao.clone(),
          audio
            .output_sample_rate
            .map(|rate| format!("{:.1} kHz", f64::from(rate) / 1000.0)),
          audio.volume.map(|volume| {
            state.format(
              "player-information-volume",
              &[("volume", format!("{volume:.0}").into())],
            )
          }),
          audio
            .muted
            .filter(|muted| *muted)
            .map(|_| state.t("player-information-muted")),
        ])
      }),
    ))
}

fn section<'a>(state: &'a State, label: &'static str, icon: Icon) -> Element<'a, Message> {
  container(
    row![
      icon_with_color(icon, IconSize::Custom(12.0), DARK_PALETTE.text.metadata),
      text(state.t(label))
        .size(10)
        .color(DARK_PALETTE.text.metadata),
      container(text(""))
        .width(Fill)
        .height(1)
        .style(cinema::separator),
    ]
    .spacing(6)
    .align_y(Alignment::Center),
  )
  .padding(iced::Padding {
    top: 8.0,
    right: 10.0,
    bottom: 3.0,
    left: 10.0,
  })
  .into()
}

fn stat_row<'a>(
  state: &'a State,
  label: &'static str,
  value: Option<String>,
) -> Element<'a, Message> {
  container(
    row![
      text(state.t(label))
        .size(12)
        .line_height(iced::Pixels(16.0))
        .width(82)
        .color(DARK_PALETTE.text.metadata),
      text(value.unwrap_or_else(|| state.t("player-information-value-unavailable")))
        .font(information_font())
        .size(12)
        .line_height(iced::Pixels(16.0))
        .wrapping(text::Wrapping::Glyph)
        .width(Fill)
        .align_x(Alignment::End)
        .color(DARK_PALETTE.text.secondary),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Start),
  )
  .padding([4, 10])
  .width(Fill)
  .into()
}

fn join(parts: impl IntoIterator<Item = Option<String>>) -> Option<String> {
  let mut result = String::new();
  for part in parts.into_iter().flatten().filter(|part| !part.is_empty()) {
    if !result.is_empty() {
      result.push_str(" · ");
    }
    result.push_str(&part);
  }
  (!result.is_empty()).then_some(result)
}

fn bytes(value: u64) -> String {
  const MIB: f64 = 1024.0 * 1024.0;
  const GIB: f64 = MIB * 1024.0;
  if value as f64 >= GIB {
    format!("{:.1} GiB", value as f64 / GIB)
  } else {
    format!("{:.1} MiB", value as f64 / MIB)
  }
}

fn bitrate(value: u64) -> String {
  if value >= 1_000_000 {
    format!("{:.1} Mbps", value as f64 / 1_000_000.0)
  } else {
    format!("{:.0} kbps", value as f64 / 1000.0)
  }
}
