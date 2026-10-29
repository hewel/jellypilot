//! Synthetic, opt-in renderer verification; never opens a media session or a network source.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::advanced::Widget as _;
use iced::widget::{column, container, mouse_area, row, slider, stack, text};
use iced::{Alignment, Element, Fill, Subscription, Task};
use jellypilot_core::danmaku::{Clock, ClockSample, Comment, Kind, Track};
use jellypilot_ui::control_button;
use jellypilot_ui::fonts::{self, BODY_FONT, HEADING_FONT};
use jellypilot_ui::theme::{self, ThemeMode};
use jellypilot_ui::tokens::{DARK_PALETTE, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::danmaku::{danmaku, Style};
use jellypilot_ui::widgets::switch::switch;

const DURATION: f64 = 120.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fonts::initialize()?;
    let smoke = std::env::args().any(|argument| argument == "--smoke");
    let rendered = Arc::new(AtomicBool::new(false));
    let completion = Arc::clone(&rendered);
    let first = Arc::new(synthetic_track(false)?);
    let second = Arc::new(synthetic_track(true)?);
    iced::application(
        move || {
            Scene::new(
                Arc::clone(&first),
                Arc::clone(&second),
                smoke,
                Arc::clone(&completion),
            )
        },
        Scene::update,
        Scene::view,
    )
    .title("JellyPilot · 弹幕渲染验证")
    .theme(|_: &Scene| theme::theme(ThemeMode::Dark))
    .font(BODY_FONT)
    .window_size((1080.0, 760.0))
    .subscription(Scene::subscription)
    .run()?;
    if smoke {
        if !rendered.load(Ordering::Acquire) {
            return Err(std::io::Error::other(
                "Danmaku scene closed before its frame smoke completed",
            )
            .into());
        }
        println!("Danmaku native scene smoke passed (synthetic clock/data; no MPV, network, or visual acceptance).");
    }
    Ok(())
}

fn synthetic_track(alternate: bool) -> Result<Track, jellypilot_core::danmaku::Error> {
    let samples = [
        "合成弹幕 · 正在验证中文字体与平滑滚动",
        "Different lengths must not catch the preceding comment",
        "短句",
        "这是用于排轨验证的较长文本，不代表真实用户评论",
        "Unicode: café · Ελληνικά · العربية",
        "纯文本：{\\pos(0,0)} <b>不会执行标记</b>",
    ];
    let prefix = if alternate { "B" } else { "A" };
    let mut comments = Vec::with_capacity(1100);
    for index in 0..600 {
        comments.push(Comment {
            at_seconds: f64::from(index) * 0.18,
            text: format!("{prefix} · {}", samples[index as usize % samples.len()]),
            color: if index % 7 == 0 { 0xffd166 } else { 0xffffff },
            kind: match index % 13 {
                0 => Kind::Top,
                1 => Kind::Bottom,
                _ => Kind::Scroll,
            },
        });
    }
    // An explicit burst proves the renderer drops excess arrivals instead of queuing them late.
    for index in 0..500 {
        comments.push(Comment {
            at_seconds: 30.0 + f64::from(index) * 0.002,
            text: format!("{prefix} · 密集片段 {index}"),
            color: 0xffffff,
            kind: Kind::Scroll,
        });
    }
    Track::new(comments)
}

struct Scene {
    track: Arc<Track>,
    other: Arc<Track>,
    clock: Clock,
    style: Style,
    sampled_at: Instant,
    position: f64,
    speed: f64,
    paused: bool,
    buffering: bool,
    enabled: bool,
    media_id: u64,
    clicks: usize,
    smoke: bool,
    frames: usize,
    rendered: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
enum Message {
    Observe(Instant),
    Frame,
    Timeout,
    Pause,
    Buffer,
    Enabled,
    Seek(f64),
    Speed(f64),
    Coverage(f32),
    FontSize(f32),
    Opacity(f32),
    Outline,
    Shadow,
    Offset(f64),
    Replace,
    Pointer,
}

impl Scene {
    fn new(
        track: Arc<Track>,
        other: Arc<Track>,
        smoke: bool,
        rendered: Arc<AtomicBool>,
    ) -> (Self, Task<Message>) {
        let now = Instant::now();
        let mut scene = Self {
            track,
            other,
            clock: Clock::default(),
            style: Style {
                kinds: [true, true, true],
                top_inset: 56.0,
                bottom_inset: 64.0,
                ..Style::default()
            },
            sampled_at: now,
            position: 4.0,
            speed: 1.0,
            paused: false,
            buffering: false,
            enabled: true,
            media_id: 1,
            clicks: 0,
            smoke,
            frames: 0,
            rendered,
        };
        scene.observe(now);
        (scene, Task::none())
    }

    fn advance(&mut self, now: Instant) {
        if now < self.sampled_at {
            return;
        }
        if !self.paused && !self.buffering {
            self.position = (self.position
                + now.saturating_duration_since(self.sampled_at).as_secs_f64() * self.speed)
                .min(DURATION);
            if self.position >= DURATION {
                self.paused = true;
            }
        }
        self.sampled_at = now;
    }

    fn observe(&mut self, now: Instant) {
        self.clock.observe(
            ClockSample {
                media_id: self.media_id,
                position_seconds: self.position,
                speed: self.speed,
                paused: self.paused,
                buffering: self.buffering,
                seeking: false,
            },
            now,
        );
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        if matches!(message, Message::Timeout) {
            return iced::exit();
        }
        if matches!(message, Message::Frame) {
            self.frames += 1;
            if self.smoke {
                // Exercise both independent switches through their real handlers
                // across presented frames, then restore the initial appearance.
                match self.frames {
                    1 | 3 => return self.update(Message::Outline),
                    2 | 4 => return self.update(Message::Shadow),
                    _ => {}
                }
                self.rendered.store(true, Ordering::Release);
                return iced::exit();
            }
            return Task::none();
        }
        let now = match message {
            Message::Observe(now) => now,
            _ => Instant::now(),
        };
        self.advance(now);
        match message {
            Message::Pause => self.paused = !self.paused,
            Message::Buffer => self.buffering = !self.buffering,
            Message::Enabled => self.enabled = !self.enabled,
            Message::Seek(seconds) => self.position = seconds.clamp(0.0, DURATION),
            Message::Speed(speed) => self.speed = speed,
            Message::Coverage(value) => self.style.coverage = value,
            Message::FontSize(value) => self.style.font_size = value,
            Message::Opacity(value) => self.style.opacity = value,
            Message::Outline => self.style.outline_enabled = !self.style.outline_enabled,
            Message::Shadow => self.style.shadow_enabled = !self.style.shadow_enabled,
            Message::Offset(delta) => self.style.offset_seconds += delta,
            Message::Replace => {
                std::mem::swap(&mut self.track, &mut self.other);
                self.media_id += 1;
                self.position = 4.0;
            }
            Message::Pointer => self.clicks += 1,
            Message::Observe(_) | Message::Frame | Message::Timeout => {}
        }
        self.observe(now);
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let clock = if !self.paused && !self.buffering {
            iced::time::every(Duration::from_millis(100)).map(Message::Observe)
        } else {
            Subscription::none()
        };
        let frames = if self.smoke {
            iced::window::frames().map(|_| Message::Frame)
        } else {
            Subscription::none()
        };
        let watchdog = if self.smoke {
            iced::time::every(Duration::from_secs(10)).map(|_| Message::Timeout)
        } else {
            Subscription::none()
        };
        Subscription::batch([clock, frames, watchdog])
    }

    fn view(&self) -> Element<'_, Message> {
        let action = |label: &str, message, active| {
            control_button(
                None,
                Some(label.to_owned()),
                if active {
                    ButtonVariant::TonalActive
                } else {
                    ButtonVariant::Tonal
                },
            )
            .label_size(13.0)
            .on_press(message)
        };
        let backdrop = mouse_area(
            container(
                text(format!("合成画布 · 点击穿透验证：{} 次", self.clicks))
                    .size(16)
                    .color(DARK_PALETTE.text.metadata),
            )
            .center_x(Fill)
            .center_y(Fill)
            .style(|theme| theme::surface_variant(theme, SurfaceVariant::Block)),
        )
        .on_press(Message::Pointer);
        let captions = column![
            container(text("副字幕安全区（模拟）").size(16))
                .width(Fill)
                .align_x(Alignment::Center)
                .padding(16),
            iced::widget::space().height(Fill),
            container(text("主字幕安全区（模拟）").size(20))
                .width(Fill)
                .align_x(Alignment::Center)
                .padding(16),
        ];
        let canvas = stack![
            backdrop,
            danmaku(&self.track, &self.clock)
                .style(self.style)
                .enabled(self.enabled)
                .font(HEADING_FONT),
            captions
        ]
        .height(Fill);
        let transport = row![
            action(
                if self.paused { "播放" } else { "暂停" },
                Message::Pause,
                self.paused
            ),
            action("模拟缓冲", Message::Buffer, self.buffering),
            action("显示弹幕", Message::Enabled, self.enabled),
            action("1×", Message::Speed(1.0), self.speed == 1.0),
            action("2×", Message::Speed(2.0), self.speed == 2.0),
            action("密集片段", Message::Seek(30.0), false),
            action("更换数据 / 媒体", Message::Replace, false),
        ]
        .spacing(8);
        let appearance = row![
            action(
                "20px",
                Message::FontSize(20.0),
                self.style.font_size == 20.0
            ),
            action(
                "24px",
                Message::FontSize(24.0),
                self.style.font_size == 24.0
            ),
            action(
                "28px",
                Message::FontSize(28.0),
                self.style.font_size == 28.0
            ),
            action("上 ¼", Message::Coverage(0.25), self.style.coverage == 0.25),
            action("上 ½", Message::Coverage(0.5), self.style.coverage == 0.5),
            action("全区域", Message::Coverage(1.0), self.style.coverage == 1.0),
            action("提前 0.5s", Message::Offset(-0.5), false),
            action("延后 0.5s", Message::Offset(0.5), false),
        ]
        .spacing(8);
        container(
            column![
                text("弹幕渲染验证").font(HEADING_FONT).size(24),
                text("合成数据与时钟 · 不连接 MPV 或在线来源 · 不修改播放器设置")
                    .size(13)
                    .color(DARK_PALETTE.text.metadata),
                canvas,
                row![
                    text(format!("{:.1} / {DURATION:.0}s", self.position)).width(110),
                    slider(0.0..=DURATION, self.position, Message::Seek)
                ]
                .spacing(16)
                .align_y(Alignment::Center),
                transport,
                appearance,
                row![
                    text("描边"),
                    switch(self.style.outline_enabled).on_press(Message::Outline),
                    text("阴影"),
                    switch(self.style.shadow_enabled).on_press(Message::Shadow),
                ]
                .spacing(TOKENS.spacing.s3)
                .align_y(Alignment::Center),
                row![
                    text(format!("不透明度 {:.0}%", self.style.opacity * 100.0)).width(140),
                    slider(0.25..=1.0, self.style.opacity, Message::Opacity).step(0.05),
                    text(format!("偏移 {:+.1}s", self.style.offset_seconds)).width(130)
                ]
                .spacing(16)
                .align_y(Alignment::Center),
            ]
            .spacing(TOKENS.spacing.s3),
        )
        .padding(TOKENS.spacing.s6)
        .width(Fill)
        .height(Fill)
        ._boxed()
    }
}
