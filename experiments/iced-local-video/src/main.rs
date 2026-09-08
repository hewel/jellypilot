use iced::widget::{
  column, container, progress_bar, row, scrollable, slider, stack, text, text_input,
};
use iced::{window, Element, Length::Fill, Subscription, Task, Theme};
use jellypilot_player::{
  AudioOutput, NetworkSource, PlaybackError, PlaybackPhase, PlaybackSource, Player,
};
use jellypilot_ui::{
  control_button,
  theme::{field_variant, surface_variant, theme, ThemeMode},
  tokens::TOKENS,
  variants::{ButtonVariant, FieldVariant, SurfaceVariant},
};
use std::{
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  time::Duration,
};

const DEFAULT_FILE: &str = "test-videos/bbb_h264_1080p_5mb.mp4";
#[derive(Clone)]
struct Options {
  source: Option<PlaybackSource>,
  smoke: bool,
}
impl Options {
  fn parse() -> Result<Self, String> {
    let mut options = Self {
      source: None,
      smoke: false,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
      match argument.as_str() {
        "--smoke-test" if !options.smoke => options.smoke = true,
        "--file" | "--url" if options.source.is_none() => {
          let value = arguments
            .next()
            .filter(|s| !s.trim().is_empty() && !s.starts_with("--"))
            .ok_or("Source option requires a non-empty value")?;
          options.source = Some(if argument == "--file" {
            PlaybackSource::Local(value.into())
          } else {
            PlaybackSource::Network(NetworkSource::new(&value).map_err(|error| error.to_string())?)
          });
        }
        "--url-env" if options.source.is_none() => {
          let value = std::env::var("JELLYPILOT_VIDEO_URL")
            .map_err(|_| "JELLYPILOT_VIDEO_URL requires a valid Unicode URL")?;
          options.source = Some(PlaybackSource::Network(
            NetworkSource::new(&value).map_err(|error| error.to_string())?,
          ));
        }
        _ => return Err("Unknown, conflicting, or duplicate argument".into()),
      }
    }
    Ok(options)
  }
}
#[derive(Clone)]
enum Message {
  PathChanged(String),
  Open,
  BackendChanged,
  TogglePlaying,
  DragSeek(f64),
  CommitSeek,
  Volume(f64),
  ToggleMuted,
  CloseRequested(window::Id),
  Stopped(Result<(), PlaybackError>),
  SmokeDeadline,
}
// Text input may contain signed URLs; iced diagnostics must never expose it.
impl std::fmt::Debug for Message {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(match self {
      Self::PathChanged(_) => "PathChanged([redacted])",
      Self::Open => "Open",
      Self::BackendChanged => "BackendChanged",
      Self::TogglePlaying => "TogglePlaying",
      Self::DragSeek(_) => "DragSeek",
      Self::CommitSeek => "CommitSeek",
      Self::Volume(_) => "Volume",
      Self::ToggleMuted => "ToggleMuted",
      Self::CloseRequested(_) => "CloseRequested",
      Self::Stopped(_) => "Stopped",
      Self::SmokeDeadline => "SmokeDeadline",
    })
  }
}

fn source_presentation(source: &PlaybackSource) -> String {
  match source {
    PlaybackSource::Local(path) => path.to_string_lossy().into_owned(),
    PlaybackSource::Network(source) => source.redacted_url(),
  }
}

struct App {
  path: String,
  selected_source: Option<PlaybackSource>,
  player: Option<Player>,
  drag_position: Option<Duration>,
  error: Option<String>,
  closing: bool,
  window: Option<window::Id>,
  smoke: bool,
  smoke_timeout: Duration,
  failed: Arc<AtomicBool>,
}
impl App {
  fn boot(options: Options, failed: Arc<AtomicBool>) -> (Self, Task<Message>) {
    let auto_open = options.source.is_some() || options.smoke;
    let smoke_timeout = Duration::from_secs(
      if matches!(&options.source, Some(PlaybackSource::Network(_))) {
        45
      } else {
        10
      },
    );
    let mut app = Self {
      path: options
        .source
        .as_ref()
        .map_or_else(|| DEFAULT_FILE.into(), source_presentation),
      selected_source: options.source,
      player: None,
      drag_position: None,
      error: None,
      closing: false,
      window: None,
      smoke: options.smoke,
      smoke_timeout,
      failed,
    };
    let audio = if options.smoke {
      AudioOutput::Discard
    } else {
      AudioOutput::System
    };
    let (player, notifications) = match Player::new(audio) {
      Ok(started) => started,
      Err(error) => {
        app.report(error.to_string(), true);
        let task = app.close();
        return (app, task);
      }
    };
    app.player = Some(player);
    let notifications = Task::run(notifications, |()| Message::BackendChanged);
    let deadline = if options.smoke {
      Task::perform(
        async move { tokio::time::sleep(smoke_timeout).await },
        |()| Message::SmokeDeadline,
      )
    } else {
      Task::none()
    };
    let open = if auto_open {
      app.update(Message::Open)
    } else {
      Task::none()
    };
    (app, Task::batch([notifications, deadline, open]))
  }
  fn report(&mut self, error: String, fatal: bool) {
    eprintln!("{error}");
    self.error = Some(error);
    if fatal {
      self.failed.store(true, Ordering::Relaxed);
    }
  }
  fn command(&mut self, command: impl FnOnce(&mut Player) -> Result<(), PlaybackError>) {
    if let Some(player) = &mut self.player {
      if let Err(error) = command(player) {
        self.report(error.to_string(), self.smoke);
      }
    }
  }
  fn close(&mut self) -> Task<Message> {
    if self.closing {
      return Task::none();
    }
    self.closing = true;
    match &mut self.player {
      Some(player) => player.close().map(Message::Stopped),
      None => Task::done(Message::Stopped(Ok(()))),
    }
  }
  fn ready(&self) -> bool {
    !self.closing
      && self
        .player
        .as_ref()
        .is_some_and(|player| player.status().ready())
  }
  fn can_seek(&self) -> bool {
    !self.closing
      && self
        .player
        .as_ref()
        .is_some_and(|player| player.status().can_seek())
  }
  fn seek_bounds(&self) -> Option<(f64, f64)> {
    if !self.can_seek() {
      return None;
    }
    let range = self.player.as_ref()?.status().seek_range?;
    let start = range.start.as_secs_f64();
    let end = range.end?.as_secs_f64();
    (end > start).then_some((start, end))
  }
  fn update(&mut self, message: Message) -> Task<Message> {
    if let Message::Stopped(result) = message {
      if let Err(error) = result {
        self.report(error.to_string(), true);
      }
      return if let Some(id) = self.window {
        window::close(id)
      } else {
        window::latest().and_then(window::close)
      };
    }
    if self.closing {
      return Task::none();
    }
    match message {
      Message::CloseRequested(id) => {
        self.window = Some(id);
        return self.close();
      }
      Message::PathChanged(path) => {
        // Any edit invalidates the retained credentials, even if the text later matches.
        self.selected_source = None;
        self.path = path;
      }
      Message::Open => {
        if self.path.trim().is_empty() {
          self.error = Some("请输入本地视频路径或 HTTP(S) URL".into());
          return Task::none();
        }
        self.drag_position = None;
        self.error = None;
        let source = if let Some(source) = &self.selected_source {
          source.clone()
        } else {
          let network = self
            .path
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
            || self
              .path
              .get(..8)
              .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"));
          if network {
            match NetworkSource::new(&self.path) {
              Ok(source) => PlaybackSource::Network(source),
              Err(error) => {
                self.report(error.to_string(), self.smoke);
                return if self.smoke {
                  self.close()
                } else {
                  Task::none()
                };
              }
            }
          } else {
            PlaybackSource::Local(self.path.clone().into())
          }
        };
        self.path = source_presentation(&source);
        self.selected_source = Some(source.clone());
        self.command(|player| player.open(source));
      }
      Message::BackendChanged => {
        if let Some(player) = &mut self.player {
          let result = player.refresh();
          let error = match result {
            Err(error) => Some(error.to_string()),
            Ok(()) => player.status().error.as_ref().map(ToString::to_string),
          };
          if let Some(error) = error {
            self.report(error, self.smoke);
          }
        }
        if self.smoke {
          if self.error.is_some() {
            return self.close();
          }
          if let Some(player) = &self.player {
            match player.uploaded_frame() {
              Ok(Some(sequence)) => {
                eprintln!(
                  "Smoke: wgpu uploaded frame {sequence}; appsink queue={}, dropped={}",
                  player.status().queue_depth,
                  player.status().dropped
                );
                return self.close();
              }
              Err(error) => {
                self.report(error.to_string(), true);
                return self.close();
              }
              Ok(None) => {}
            }
          }
        }
      }
      Message::TogglePlaying if self.ready() => {
        self.command(|player| player.set_playing(!player.status().playing));
      }
      Message::DragSeek(seconds) if self.can_seek() && seconds.is_finite() => {
        if let Some((start, end)) = self.seek_bounds() {
          self.drag_position = Some(Duration::from_secs_f64(seconds.clamp(start, end)));
        }
      }
      Message::CommitSeek if self.can_seek() => {
        if let Some(position) = self.drag_position.take() {
          self.error = None;
          self.command(|player| player.seek(position));
        }
      }
      Message::Volume(volume) if volume.is_finite() => {
        self.command(|player| player.set_volume(volume.clamp(0.0, 100.0) / 100.0));
      }
      Message::ToggleMuted => {
        self.command(|player| player.set_muted(!player.status().muted));
      }
      Message::SmokeDeadline if self.smoke => {
        self.report(
          format!(
            "Smoke timed out: no video frame uploaded within {} seconds",
            self.smoke_timeout.as_secs()
          ),
          true,
        );
        return self.close();
      }
      _ => {}
    }
    if self.smoke && self.error.is_some() {
      self.close()
    } else {
      Task::none()
    }
  }
  fn view(&self) -> Element<'_, Message> {
    let status = self.player.as_ref().map(Player::status);
    let phase = if self.closing {
      PlaybackPhase::Closing
    } else {
      status.map_or(PlaybackPhase::Error, |status| status.phase)
    };
    let playing = status.is_some_and(|status| status.playing);
    let volume_percent = status.map_or(100.0, |status| status.volume * 100.0);
    let muted = status.is_some_and(|status| status.muted);
    let duration = status.and_then(|status| status.duration);
    let spacing = TOKENS.spacing.s3;
    let path = text_input("本地视频路径或 HTTP(S) URL", &self.path)
      .padding(spacing)
      .style(|theme, status| field_variant(theme, status, FieldVariant::Filled));
    let path = if self.closing {
      path
    } else {
      path.on_input(Message::PathChanged).on_submit(Message::Open)
    };
    let open = control_button(None, Some("打开".into()), ButtonVariant::Primary)
      .on_press_maybe((!self.closing).then_some(Message::Open));
    let label = match phase {
      PlaybackPhase::Idle => "输入本地视频路径或 HTTP(S) URL 并打开",
      PlaybackPhase::Loading => "加载中",
      PlaybackPhase::Playing => "正在播放",
      PlaybackPhase::Paused => "已暂停",
      PlaybackPhase::Ended => "播放结束",
      PlaybackPhase::Error => "播放错误，可打开其他来源",
      PlaybackPhase::Closing => "正在停止播放",
    };
    let video: Element<'_, Message> = match &self.player {
      Some(player) => player.view(),
      None => container(text(label)).center(Fill).into(),
    };
    let play_label = if phase == PlaybackPhase::Ended {
      "重播"
    } else if playing {
      "暂停"
    } else {
      "播放"
    };
    let label = status.and_then(|status| status.buffering).map_or_else(
      || label.to_owned(),
      |percent| format!("{label} · 缓冲 {percent}%"),
    );
    let transport = container(
      row![
        control_button(None, Some(play_label.into()), ButtonVariant::Tonal)
          .on_press_maybe(self.ready().then_some(Message::TogglePlaying)),
        text(label)
      ]
      .spacing(spacing)
      .align_y(iced::Alignment::Center),
    )
    .padding(spacing)
    .style(|theme| surface_variant(theme, SurfaceVariant::Floating));
    let overlay = container(transport)
      .width(Fill)
      .height(Fill)
      .align_x(iced::Alignment::Center)
      .align_y(iced::Alignment::End)
      .padding(spacing);
    let surface = container(stack![video, overlay])
      .width(Fill)
      .height(Fill)
      .style(|theme| surface_variant(theme, SurfaceVariant::Canvas));
    let position = self
      .drag_position
      .or(status.and_then(|status| status.position));
    let timeline: Element<'_, Message> = if let Some((start, end)) = self.seek_bounds() {
      slider(
        start..=end,
        position
          .map_or(start, |d| d.as_secs_f64())
          .clamp(start, end),
        Message::DragSeek,
      )
      .step(0.01)
      .on_release(Message::CommitSeek)
      .into()
    } else {
      progress_bar(0.0..=1.0, 0.0).girth(4).into()
    };
    let time = text(format!(
      "{} / {}",
      time_label(position),
      time_label(duration)
    ))
    .width(190);
    let volume: Element<'_, Message> = if self.closing || self.player.is_none() {
      text(format!("音量 {}%", volume_percent as u32)).into()
    } else {
      row![
        text("音量"),
        slider(0.0..=100.0, volume_percent, Message::Volume).width(150),
        text(format!("{}%", volume_percent as u32)).width(45),
        control_button(
          None,
          Some(if muted { "取消静音" } else { "静音" }.into()),
          if muted {
            ButtonVariant::TonalActive
          } else {
            ButtonVariant::Tonal
          }
        )
        .on_press(Message::ToggleMuted)
      ]
      .spacing(spacing)
      .align_y(iced::Alignment::Center)
      .into()
    };
    let mut content = column![
      row![path, open].spacing(spacing),
      surface,
      timeline,
      row![time, iced::widget::Space::new().width(Fill), volume]
        .spacing(spacing)
        .align_y(iced::Alignment::Center)
    ]
    .spacing(spacing);
    if let Some(error) = &self.error {
      content = content.push(scrollable(text(error)).height(64));
    }
    content = content.push(
      text(format!(
        "实验 · wgpu · appsink 队列 {} / 2 · 丢帧 {}",
        status.map_or(0, |status| status.queue_depth),
        status.map_or(0, |status| status.dropped)
      ))
      .size(12),
    );
    container(content)
      .padding(spacing)
      .width(Fill)
      .height(Fill)
      .style(|theme| surface_variant(theme, SurfaceVariant::Canvas))
      .into()
  }
}
fn time_label(duration: Option<Duration>) -> String {
  let Some(duration) = duration else {
    return "--:--".into();
  };
  let seconds = duration.as_secs();
  if seconds < 3600 {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
  } else {
    format!(
      "{}:{:02}:{:02}",
      seconds / 3600,
      seconds / 60 % 60,
      seconds % 60
    )
  }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
  let options = Options::parse()?;
  jellypilot_ui::fonts::initialize()?;
  let failed = Arc::new(AtomicBool::new(false));
  let boot_failed = failed.clone();
  let application = iced::application(
    move || App::boot(options.clone(), boot_failed.clone()),
    App::update,
    App::view,
  )
  .title("JellyPilot Local Video — Experiment")
  .backend(iced::Backend::from("wgpu"))
  .window(window::Settings {
    size: iced::Size::new(1100.0, 760.0),
    min_size: Some(iced::Size::new(640.0, 480.0)),
    exit_on_close_request: false,
    ..window::Settings::default()
  })
  .subscription(|_: &App| -> Subscription<Message> {
    window::close_requests().map(Message::CloseRequested)
  })
  .theme(|_: &App| -> Theme { theme(ThemeMode::Dark) })
  .font(jellypilot_ui::fonts::BODY_FONT)
  .line_height(jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT)
  .fonts(jellypilot_ui::fonts::fonts());
  application.run()?;
  if failed.load(Ordering::Relaxed) {
    return Err("Local video experiment failed; see stderr".into());
  }
  Ok(())
}
