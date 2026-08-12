use relm4::gtk::prelude::*;
use relm4::{gtk, ComponentParts, ComponentSender, RelmApp, SimpleComponent};

use crate::library_browse::{LibraryBrowseView, NativeLibraryBrowse};

const APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview";
const SMOKE_APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview.Smoke";
struct AppModel {
  library: NativeLibraryBrowse,
}

#[relm4::component]
impl SimpleComponent for AppModel {
  type Init = ();
  type Input = ();
  type Output = ();

  view! {
    #[root]
    main_window = gtk::ApplicationWindow {
      set_title: Some("JellyPilot"),
      set_default_size: (1280, 720),

      #[wrap(Some)]
      set_titlebar = &gtk::HeaderBar {
        set_show_title_buttons: true,
      },

      gtk::Box {
        set_orientation: gtk::Orientation::Horizontal,

        gtk::Box {
          set_orientation: gtk::Orientation::Vertical,
          set_width_request: 240,

          #[name = "navigation"]
          gtk::StackSidebar {
            set_vexpand: true,
            set_hexpand: true,
          },

          gtk::Separator {},

          gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_margin_top: 12,
            set_margin_bottom: 12,
            set_margin_start: 12,
            set_margin_end: 12,
            set_accessible_role: gtk::AccessibleRole::Status,
            set_tooltip_text: Some("Library Browser adapter status"),

            gtk::Image {
              set_icon_name: Some("dialog-information-symbolic"),
              set_pixel_size: 16,
            },
            gtk::Label {
              #[watch]
              set_label: library_status(&model.library.view()),
              set_xalign: 0.0,
              set_wrap: true,
              set_hexpand: true,
              add_css_class: "dim-label",
            },
          },
        },

        gtk::Separator {
          set_orientation: gtk::Orientation::Vertical,
        },

        #[name = "content_stack"]
        gtk::Stack {
          set_hexpand: true,
          set_vexpand: true,
          add_named[Some("video-home")] = &page(
            "Video Home",
            "Browse your connected video libraries.",
            "Library Browser is unavailable",
            "This GTK preview has no media-server transport yet, so it cannot load libraries, search, or start playback.",
          ),
          add_named[Some("now-playing")] = &page(
            "Now Playing",
            "Playback Target status and transport controls.",
            "Playback state is unavailable",
            "This GTK preview does not yet expose the MPV process and JSON IPC adapter to the native shell.",
          ),
          add_named[Some("settings")] = &page(
            "Settings",
            "Connection and Playback Engine preferences.",
            "Settings are not connected yet",
            "Saved Service Profiles and configuration persistence will appear here when their native adapters are available.",
          ),
        },
      },
    }
  }

  fn init(
    _init: Self::Init,
    _root: Self::Root,
    _sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
    let model = Self {
      // The reducer is kept dormant until this shell owns a real server transport.
      library: NativeLibraryBrowse::default(),
    };
    let widgets = view_output!();

    widgets.navigation.set_stack(&widgets.content_stack);
    set_stack_page_title(&widgets.content_stack, "video-home", "Video Home");
    set_stack_page_title(&widgets.content_stack, "now-playing", "Now Playing");
    set_stack_page_title(&widgets.content_stack, "settings", "Settings");

    ComponentParts { model, widgets }
  }

  fn update(&mut self, _message: Self::Input, _sender: ComponentSender<Self>) {}
}

fn library_status(_view: &LibraryBrowseView) -> &'static str {
  "Library Browser adapter unavailable"
}

fn set_stack_page_title(stack: &gtk::Stack, name: &str, title: &str) {
  if let Some(child) = stack.child_by_name(name) {
    stack.page(&child).set_title(title);
  }
}

fn page(title: &str, subtitle: &str, status_title: &str, status_copy: &str) -> gtk::Widget {
  let page = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(24)
    .margin_top(32)
    .margin_bottom(32)
    .margin_start(32)
    .margin_end(32)
    .build();

  let heading = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(6)
    .build();
  let title = gtk::Label::builder().label(title).xalign(0.0).build();
  title.add_css_class("title");
  let subtitle = gtk::Label::builder()
    .label(subtitle)
    .xalign(0.0)
    .wrap(true)
    .build();
  subtitle.add_css_class("dim-label");
  heading.append(&title);
  heading.append(&subtitle);

  let status = gtk::Box::builder()
    .orientation(gtk::Orientation::Horizontal)
    .spacing(12)
    .margin_top(12)
    .build();
  status.set_accessible_role(gtk::AccessibleRole::Status);
  let icon = gtk::Image::from_icon_name("dialog-information-symbolic");
  icon.set_pixel_size(24);
  let copy = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(4)
    .build();
  let status_title = gtk::Label::builder()
    .label(status_title)
    .xalign(0.0)
    .wrap(true)
    .build();
  status_title.add_css_class("heading");
  let status_copy = gtk::Label::builder()
    .label(status_copy)
    .xalign(0.0)
    .wrap(true)
    .max_width_chars(72)
    .build();
  status_copy.add_css_class("dim-label");
  copy.append(&status_title);
  copy.append(&status_copy);
  status.append(&icon);
  status.append(&copy);

  page.append(&heading);
  page.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
  page.append(&status);
  page.upcast()
}

pub(crate) fn run(smoke_test: bool) {
  let app = RelmApp::new(if smoke_test { SMOKE_APP_ID } else { APP_ID });
  if smoke_test {
    app.allow_multiple_instances(true);
    let application = relm4::main_application();
    application.connect_window_added(move |application, window| {
      let application = application.clone();
      window.connect_map(move |_| {
        let application = application.clone();
        gtk::glib::idle_add_local_once(move || application.quit());
      });
    });
  }
  if smoke_test {
    app
      .with_args(vec!["jellypilot-gtk-smoke".to_owned()])
      .run::<AppModel>(());
  } else {
    app.run::<AppModel>(());
  }
}
