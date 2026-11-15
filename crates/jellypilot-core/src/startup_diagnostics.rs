//! Read-only, display-free startup checks. Reports contain only allowlisted text;
//! configuration, environment values, paths and underlying errors never enter them.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::config::{PlaybackBackend, Settings};

const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;

/// Severity of a local startup observation, not a playback readiness guarantee.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckStatus {
    Ok,
    Info,
    Issue,
    NotProbed,
}

impl CheckStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Info => "info",
            Self::Issue => "issue",
            Self::NotProbed => "not probed",
        }
    }
}

/// An allowlisted observation. No user-controlled strings are retained.
#[derive(Clone, Copy, Debug)]
pub struct StartupCheck {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: &'static str,
    pub recommendation: Option<&'static str>,
}

/// A safe-to-share local report; it deliberately excludes logs and raw settings.
#[derive(Debug)]
pub struct StartupReport {
    checks: Vec<StartupCheck>,
}

impl StartupReport {
    pub fn checks(&self) -> &[StartupCheck] {
        &self.checks
    }

    /// True when a selected local dependency or configuration check found an issue.
    pub fn has_blocking_findings(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == CheckStatus::Issue)
    }

    fn add(
        &mut self,
        name: &'static str,
        status: CheckStatus,
        detail: &'static str,
        recommendation: Option<&'static str>,
    ) {
        self.checks.push(StartupCheck {
            name,
            status,
            detail,
            recommendation,
        });
    }
}

impl fmt::Display for StartupReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "JellyPilot local startup diagnostics")?;
        writeln!(formatter, "App version: {}", env!("CARGO_PKG_VERSION"))?;
        writeln!(
            formatter,
            "Platform: {} / {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )?;
        writeln!(
            formatter,
            "Read-only: no desktop, player, keyring or network initialization."
        )?;
        writeln!(
            formatter,
            "Privacy: no paths, environment values, raw configuration or logs included.\n"
        )?;
        for check in &self.checks {
            writeln!(
                formatter,
                "[{}] {}: {}",
                check.status.label(),
                check.name,
                check.detail
            )?;
            if let Some(recommendation) = check.recommendation {
                writeln!(formatter, "  Action: {recommendation}")?;
            }
        }
        writeln!(
            formatter,
            "\nResult: {}",
            if self.has_blocking_findings() {
                "local issues detected; see actions above."
            } else {
                "no blocking local findings; this does not prove desktop or playback readiness."
            }
        )
    }
}

struct Environment {
    config_dir: Option<PathBuf>,
    home_present: bool,
    executable_dir: Option<PathBuf>,
    current_dir: Option<PathBuf>,
    search_path: Option<OsString>,
    windows_directory: Option<PathBuf>,
    display_present: bool,
    wayland_present: bool,
    session_present: bool,
    libmpv_override: Option<PathBuf>,
    baseline_override: Option<PathBuf>,
    host_abi: Option<OsString>,
    renderer_override_present: bool,
}

fn present(variable: &str) -> bool {
    std::env::var_os(variable).is_some_and(|value| !value.is_empty())
}

/// Collects local observations without initializing desktop, player or network APIs.
/// An explicit configuration path is read instead of the platform's settings file.
/// Neither path is migrated or saved, and paths never appear in the report.
pub fn collect_startup_report(config_override: Option<&Path>) -> StartupReport {
    let environment = Environment {
        config_dir: dirs::config_dir(),
        home_present: dirs::home_dir().is_some(),
        executable_dir: std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_owned)),
        current_dir: std::env::current_dir().ok(),
        search_path: std::env::var_os("PATH"),
        windows_directory: std::env::var_os("SystemRoot").map(PathBuf::from),
        display_present: present("DISPLAY"),
        wayland_present: present("WAYLAND_DISPLAY"),
        session_present: present("XDG_SESSION_TYPE") || present("DBUS_SESSION_BUS_ADDRESS"),
        libmpv_override: std::env::var_os("JELLYPILOT_LIBMPV").map(PathBuf::from),
        baseline_override: std::env::var_os("JELLYPILOT_MPV_BASELINE").map(PathBuf::from),
        host_abi: std::env::var_os("JELLYPILOT_MPV_HOST_ABI"),
        renderer_override_present: std::env::var_os("ICED_BACKEND").is_some()
            || std::env::var_os("WGPU_BACKEND").is_some(),
    };
    collect(&environment, config_override)
}

fn collect(environment: &Environment, config_override: Option<&Path>) -> StartupReport {
    let mut report = StartupReport { checks: Vec::new() };
    report.add(
        "Home directory",
        if environment.home_present {
            CheckStatus::Ok
        } else {
            CheckStatus::Issue
        },
        if environment.home_present {
            "Home directory lookup available."
        } else {
            "Home directory lookup unavailable."
        },
        (!environment.home_present)
            .then_some("Run from your normal user account and check home-directory setup."),
    );
    let default_config;
    let config_path = if let Some(path) = config_override {
        report.add(
            "Configuration location",
            CheckStatus::Info,
            "Explicit configuration file selected; platform configuration not read.",
            None,
        );
        path
    } else {
        // Match the application's temporary-directory fallback, but do not create it.
        if environment.config_dir.is_none() {
            report.add("Configuration location", CheckStatus::Issue,
                "Platform configuration directory unavailable; application temporary-directory fallback inspected.",
                Some("Check your user profile and platform configuration-directory setup."));
        }
        default_config = environment
            .config_dir
            .clone()
            .unwrap_or_else(std::env::temp_dir)
            .join("jellypilot/config.json");
        &default_config
    };
    let settings = inspect_config(config_path, config_override.is_some(), &mut report);
    let backend = settings
        .as_ref()
        .map(Settings::startup_playback_backend)
        .unwrap_or_default();
    report.add("Selected playback backend", CheckStatus::Info, match backend {
        PlaybackBackend::Embedded => "Embedded MPV (ordinary startup; includes pending Linux default revision policy).",
        PlaybackBackend::External => "External MPV (ordinary startup).",
    }, Some("Desktop --external/--embedded launch overrides are not applied by this diagnostics command."));
    inspect_renderer(environment, settings.as_ref(), backend, &mut report);
    inspect_session(environment, &mut report);
    inspect_external(
        environment,
        settings.as_ref().and_then(Settings::mpv_path),
        backend,
        &mut report,
    );
    inspect_embedded(environment, backend, &mut report);
    for (name, detail, action) in [
        ("GPU and desktop", "Not probed: GPU drivers, Vulkan, display connection and desktop initialization.", "Use a desktop startup reproduction to investigate GPU/display failures; file presence is not driver compatibility."),
        ("Playback", "Not probed: player execution, runtime ABI, shared-library dependencies, decoding, audio and media playback.", "Test playback separately after resolving local findings; no configured executable was run."),
        ("Credentials", "Not probed: credential storage, keyring access or sign-in.", "Check sign-in in the application; never attach credentials or raw configuration to an issue."),
        ("Network", "Not probed: server reachability, DNS, TLS and remote services.", "Check server connectivity separately; no network requests were made."),
    ] {
        report.add(name, CheckStatus::NotProbed, detail, Some(action));
    }
    report
}

fn inspect_renderer(
    environment: &Environment,
    settings: Option<&Settings>,
    playback: PlaybackBackend,
    report: &mut StartupReport,
) {
    report.add(
        "Configured renderer",
        CheckStatus::Info,
        settings.map_or("unavailable; default preference is auto", |settings| {
            settings.render_backend().as_str()
        }),
        Some("Renderer preferences apply next launch; desktop CLI overrides are not applied here."),
    );
    report.add(
        "Renderer environment override",
        CheckStatus::Info,
        if environment.renderer_override_present {
            "ICED_BACKEND or WGPU_BACKEND is present; value omitted."
        } else {
            "No ICED_BACKEND or WGPU_BACKEND override is present."
        },
        None,
    );
    if environment.renderer_override_present || settings.is_none() {
        report.add(
            "Expected renderer",
            CheckStatus::NotProbed,
            "Not determined: environment selection or configuration requires ordinary startup.",
            None,
        );
    } else if let Some(settings) = settings {
        let configured = settings.render_backend();
        let embedded = playback == PlaybackBackend::Embedded;
        if configured.supported(std::env::consts::OS, embedded) {
            report.add(
                "Expected renderer",
                CheckStatus::Info,
                configured
                    .resolve_auto(std::env::consts::OS, embedded)
                    .as_str(),
                Some("Expected ordinary startup request only; not an observed GPU backend."),
            );
        } else {
            report.add(
                "Expected renderer",
                CheckStatus::Issue,
                "Configured renderer is incompatible with the platform or embedded playback.",
                Some("Choose a supported renderer; embedded playback requires auto or vulkan."),
            );
        }
    }
    report.add(
        "Actual renderer",
        CheckStatus::NotProbed,
        "Not probed: no compositor or GPU initialization.",
        None,
    );
}

fn inspect_config(path: &Path, explicit: bool, report: &mut StartupReport) -> Option<Settings> {
    match read_config(path) {
        Ok(settings) => {
            report.add("Configuration", CheckStatus::Ok,
                "Existing configuration parsed read-only (limit: 4 MiB); no migration or save performed.", None);
            Some(settings)
        }
        Err(ConfigFinding::Missing) if !explicit => {
            report.add("Configuration", CheckStatus::Info,
                "No configuration file found; ordinary startup uses platform defaults. Nothing created.", None);
            Some(Settings::default())
        }
        Err(finding) => {
            report.add("Configuration", CheckStatus::Issue, match finding {
                ConfigFinding::Oversized => "Configuration exceeds the 4 MiB diagnostic limit; not parsed.",
                ConfigFinding::Invalid => "Configuration could not be parsed as application settings; contents and parser errors omitted.",
                ConfigFinding::Missing => "Explicit configuration file not found; platform configuration was not read.",
                _ => "Configuration is unreadable or is not a regular file; underlying errors omitted.",
            }, Some("Back up configuration privately, then check permissions or restore a known-good file. Diagnostics made no repairs; backend checks use platform defaults."));
            None
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ConfigFinding {
    Missing,
    Unreadable,
    Oversized,
    Invalid,
}

fn config_io_error(error: io::Error) -> ConfigFinding {
    if error.kind() == io::ErrorKind::NotFound {
        ConfigFinding::Missing
    } else {
        ConfigFinding::Unreadable
    }
}

fn read_config(path: &Path) -> Result<Settings, ConfigFinding> {
    let metadata = fs::metadata(path).map_err(config_io_error)?;
    // In particular, do not open FIFOs/devices supplied as the config file.
    if !metadata.is_file() {
        return Err(ConfigFinding::Unreadable);
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(ConfigFinding::Oversized);
    }
    let mut contents = Vec::new();
    File::open(path)
        .map_err(config_io_error)?
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut contents)
        .map_err(config_io_error)?;
    if contents.len() as u64 > MAX_CONFIG_BYTES {
        return Err(ConfigFinding::Oversized);
    }
    serde_json::from_slice(&contents).map_err(|_| ConfigFinding::Invalid)
}

fn inspect_session(environment: &Environment, report: &mut StartupReport) {
    if cfg!(target_os = "linux") {
        report.add("Display environment", if environment.display_present || environment.wayland_present { CheckStatus::Ok } else { CheckStatus::Issue },
            match (environment.display_present, environment.wayland_present) {
                (true, true) => "X11 and Wayland display variables present; values omitted; connections not tested.",
                (true, false) => "X11 display variable present; value omitted; connection not tested.",
                (false, true) => "Wayland display variable present; value omitted; connection not tested.",
                (false, false) => "Neither X11 nor Wayland display variable present.",
            }, (!(environment.display_present || environment.wayland_present)).then_some("Launch the desktop app from a graphical user session. This helper works without one."));
        report.add(
            "Session environment",
            CheckStatus::Info,
            if environment.session_present {
                "Session indicators present; values omitted; session access not tested."
            } else {
                "Session indicators absent; session access not tested."
            },
            None,
        );
    } else {
        report.add("Display/session presence", CheckStatus::NotProbed,
            "Native desktop session presence not probed on this platform; no desktop APIs initialized.", None);
    }
}

fn executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

fn search_executable(name: &str, environment: &Environment) -> Option<PathBuf> {
    let cwd = environment.current_dir.as_ref()?;
    which::which_in(name, environment.search_path.as_ref(), cwd).ok()
}

fn configured_executable_available(path: &str, environment: &Environment) -> bool {
    let path = Path::new(path);
    if cfg!(target_os = "windows") {
        return windows_configured_executable_available(path, environment);
    }
    if path.components().count() == 1 && !path.is_absolute() {
        return path
            .to_str()
            .and_then(|name| search_executable(name, environment))
            .is_some();
    }
    executable_file(path)
}

// Command::new on Windows accepts omitted .exe, but does not use PATHEXT.
// For a bare configured name it searches the application/system directories
// before the inherited PATH. Automatic MPV discovery still uses `which`.
fn windows_configured_executable_available(path: &Path, environment: &Environment) -> bool {
    let candidate = |path: &Path| {
        executable_file(path)
            || (path.extension().is_none() && executable_file(&path.with_extension("exe")))
    };
    if path.is_absolute() {
        return candidate(path);
    }
    if path.components().count() > 1 {
        return environment
            .current_dir
            .as_ref()
            .is_some_and(|cwd| candidate(&cwd.join(path)));
    }
    let system_directory = environment
        .windows_directory
        .as_ref()
        .map(|root| root.join("System32"));
    if [
        environment.executable_dir.as_deref(),
        system_directory.as_deref(),
        environment.windows_directory.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|directory| candidate(&directory.join(path)))
    {
        return true;
    }
    environment.search_path.as_ref().is_some_and(|search_path| {
        std::env::split_paths(search_path).any(|directory| candidate(&directory.join(path)))
    })
}

fn common_mpv_available() -> bool {
    let paths: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Program Files\mpv\mpv.exe",
            r"C:\Program Files (x86)\mpv\mpv.exe",
            r"C:\mpv\mpv.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/usr/local/bin/mpv",
            "/opt/homebrew/bin/mpv",
            "/Applications/mpv.app/Contents/MacOS/mpv",
        ]
    } else if cfg!(target_os = "linux") {
        &["/usr/bin/mpv", "/usr/local/bin/mpv"]
    } else {
        &[]
    };
    paths.iter().any(|path| executable_file(Path::new(path)))
}

fn inspect_external(
    environment: &Environment,
    configured: Option<&str>,
    backend: PlaybackBackend,
    report: &mut StartupReport,
) {
    let required = backend == PlaybackBackend::External;
    let configured_available =
        configured.is_some_and(|path| configured_executable_available(path, environment));
    report.add("Configured external MPV", if configured_available { CheckStatus::Ok } else if required && configured.is_some() { CheckStatus::Issue } else { CheckStatus::Info },
        if configured_available { "Configured executable candidate found; not executed." } else if configured.is_some() { "Configured executable candidate unavailable; application does not fall back when a path is configured." } else { "No executable configured; application searches PATH, then platform common locations." },
        (required && configured.is_some() && !configured_available).then_some("Correct or clear the external MPV setting after privately backing up configuration."));
    let path_available = search_executable("mpv", environment).is_some();
    report.add(
        "PATH external MPV",
        if path_available {
            CheckStatus::Ok
        } else {
            CheckStatus::Info
        },
        if path_available {
            "Executable candidate found on PATH; not executed."
        } else {
            "No executable candidate found on PATH."
        },
        None,
    );
    let bundled_available = environment.executable_dir.as_ref().is_some_and(|root| {
        let name = if cfg!(target_os = "windows") {
            "mpv.exe"
        } else {
            "mpv"
        };
        executable_file(&root.join(name)) || executable_file(&root.join("bin").join(name))
    });
    report.add("Bundled external MPV", CheckStatus::Info,
        if bundled_available { "Executable candidate present beside helper or in its bin directory; not executed. No automatic bundle preference: it must be configured or discoverable by application search." } else { "No external executable found beside helper or in its bin directory; not required when another external candidate or Embedded is selected." }, None);
    let available = if configured.is_some() {
        configured_available
    } else {
        path_available || common_mpv_available()
    };
    report.add("External MPV selection", if available { CheckStatus::Ok } else if required { CheckStatus::Issue } else { CheckStatus::Info },
        if available { "Selected external executable candidate available by configured/PATH/common-location precedence; execution not tested." } else if required { "No usable selected external executable candidate found." } else { "External executable unavailable; inactive for selected Embedded backend." },
        (required && !available).then_some("Install MPV or configure a usable external executable. Candidate presence does not verify playback."));
}

fn bundled_asset(root: Option<&Path>, relative: &str) -> Option<PathBuf> {
    let root = root?;
    // Preserve first-canonicalizable precedence even if that candidate is invalid.
    [Some(root), root.parent()]
        .into_iter()
        .flatten()
        .find_map(|directory| directory.join(relative).canonicalize().ok())
}

fn readable_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) && File::open(path).is_ok()
}

fn inspect_embedded(
    environment: &Environment,
    backend: PlaybackBackend,
    report: &mut StartupReport,
) {
    let required = backend == PlaybackBackend::Embedded;
    if !cfg!(any(target_os = "linux", target_os = "windows")) {
        report.add(
            "Embedded runtime",
            if required {
                CheckStatus::Issue
            } else {
                CheckStatus::Info
            },
            "Embedded MPV is supported only on Linux and Windows.",
            required.then_some("Select External MPV on this platform."),
        );
        return;
    }
    let library = if cfg!(target_os = "windows") {
        "lib/jellypilot/libmpv-2.dll"
    } else {
        "lib/jellypilot/libmpv.so"
    };
    for (name, bundled_name, relative, override_path) in [
        (
            "Embedded runtime selection",
            "Bundled embedded runtime",
            library,
            environment.libmpv_override.as_deref(),
        ),
        (
            "Embedded baseline selection",
            "Bundled embedded baseline",
            "share/jellypilot/mpv-baseline.conf",
            environment.baseline_override.as_deref(),
        ),
    ] {
        let bundled = bundled_asset(environment.executable_dir.as_deref(), relative);
        let bundled_available = bundled.as_deref().is_some_and(readable_file);
        report.add(bundled_name, if bundled_available { CheckStatus::Ok } else { CheckStatus::Info },
            if bundled_available { "Readable bundled file found beside executable or under executable prefix; content/ABI not validated." } else { "Readable bundled file unavailable beside executable and under executable prefix." }, None);
        let available = match override_path {
            Some(path) => path.is_absolute() && readable_file(path),
            None => bundled_available,
        };
        report.add(name, if available { CheckStatus::Ok } else if required { CheckStatus::Issue } else { CheckStatus::Info },
            match (override_path.is_some(), available) {
                (true, true) => "Environment override selects a readable absolute file; no fallback; content/ABI not validated.",
                (true, false) => "Environment override is not a readable absolute file; application does not fall back to bundled assets.",
                (false, true) => "Bundled file selected; system libmpv is not used; content/ABI not validated.",
                (false, false) => "Selected bundled file unavailable; system libmpv is not used.",
            }, (required && !available).then_some("Reinstall the complete application bundle, or use trusted pinned runtime/baseline overrides. Start the desktop with --external for external playback."));
    }
    let abi_valid = environment
        .host_abi
        .as_deref()
        .is_none_or(|abi| abi == "legacy" || abi == "3");
    report.add("Embedded host ABI setting", if abi_valid { CheckStatus::Info } else if required { CheckStatus::Issue } else { CheckStatus::Info },
        if abi_valid { "Setting accepted (default or supported selector); runtime ABI compatibility not probed." } else { "Unsupported host ABI selector; value omitted." },
        (!abi_valid && required).then_some("Unset JELLYPILOT_MPV_HOST_ABI or set it to legacy or 3 for the matching trusted runtime."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::RenderBackend;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "jellypilot-startup-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self { root }
        }

        fn environment(&self) -> Environment {
            Environment {
                config_dir: Some(self.root.clone()),
                home_present: true,
                executable_dir: Some(self.root.join("bin")),
                current_dir: Some(self.root.clone()),
                search_path: Some(OsString::new()),
                windows_directory: None,
                display_present: true,
                wayland_present: false,
                session_present: false,
                libmpv_override: None,
                baseline_override: None,
                host_abi: None,
                renderer_override_present: false,
            }
        }

        fn config(&self, contents: &[u8]) -> PathBuf {
            let directory = self.root.join("jellypilot");
            fs::create_dir(&directory).unwrap();
            let path = directory.join("config.json");
            fs::write(&path, contents).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn corrupt_config_is_reported_without_repair_or_auxiliary_files() {
        let fixture = Fixture::new();
        let contents = b"{secret-invalid-json";
        let path = fixture.config(contents);
        let report = collect(&fixture.environment(), None);
        assert_eq!(report.checks()[1].status, CheckStatus::Issue);
        assert_eq!(fs::read(&path).unwrap(), contents);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn legacy_config_predicts_startup_backend_without_migrating() {
        let fixture = Fixture::new();
        let mut settings = serde_json::to_value(Settings::default()).unwrap();
        settings["playback_backend"] = "external".into();
        settings["settings_revision"] = 0.into();
        let contents = serde_json::to_vec(&settings).unwrap();
        let path = fixture.config(&contents);
        let report = collect(&fixture.environment(), None).to_string();
        assert!(report.contains(if cfg!(target_os = "linux") {
            "Embedded MPV (ordinary startup"
        } else {
            "External MPV (ordinary startup"
        }));
        assert_eq!(fs::read(path).unwrap(), contents);
    }

    #[test]
    fn report_excludes_secrets_paths_environment_values_and_raw_parser_errors() {
        let fixture = Fixture::new();
        let secret = "PRIVATE-secret-https://user:password@example.invalid/?token=private";
        let mut settings = serde_json::to_value(Settings::default()).unwrap();
        for field in [
            "server_url",
            "username",
            "provider",
            "mpv_path",
            "tmdb_api_key",
        ] {
            settings[field] = secret.into();
        }
        settings["mpv_args"] = serde_json::json!([secret]);
        fixture.config(&serde_json::to_vec(&settings).unwrap());
        let mut environment = fixture.environment();
        environment.libmpv_override = Some(PathBuf::from(secret));
        environment.baseline_override = Some(PathBuf::from(secret));
        environment.host_abi = Some(secret.into());
        environment.search_path = Some(secret.into());
        let report = collect(&environment, None).to_string();
        assert!(!report.contains(secret));
        assert!(!report.contains(&fixture.root.to_string_lossy().to_string()));
        assert!(!report.contains("example.invalid"));
        assert!(report.contains("[not probed] Credentials"));
        assert!(report.contains("[not probed] Network"));
    }

    #[test]
    fn renderer_environment_presence_prevents_claiming_a_selected_or_actual_backend() {
        let fixture = Fixture::new();
        let mut settings = serde_json::to_value(Settings::default()).unwrap();
        settings["render_backend"] = "software".into();
        settings["playback_backend"] = "external".into();
        fixture.config(&serde_json::to_vec(&settings).unwrap());
        let mut environment = fixture.environment();
        environment.renderer_override_present = true;
        let report = collect(&environment, None);
        for name in ["Expected renderer", "Actual renderer"] {
            assert_eq!(
                report
                    .checks()
                    .iter()
                    .find(|check| check.name == name)
                    .unwrap()
                    .status,
                CheckStatus::NotProbed
            );
        }
        assert!(report.to_string().contains("Configured renderer: software"));
        assert!(report.to_string().contains("value omitted"));
    }

    #[test]
    fn renderer_prediction_is_not_reported_as_an_actual_backend() {
        let fixture = Fixture::new();
        let mut settings = serde_json::to_value(Settings::default()).unwrap();
        settings["render_backend"] = "software".into();
        settings["playback_backend"] = "external".into();
        fixture.config(&serde_json::to_vec(&settings).unwrap());
        let report = collect(&fixture.environment(), None);
        assert_eq!(
            report
                .checks()
                .iter()
                .find(|check| check.name == "Expected renderer")
                .unwrap()
                .detail,
            RenderBackend::Software.as_str()
        );
        assert_eq!(
            report
                .checks()
                .iter()
                .find(|check| check.name == "Actual renderer")
                .unwrap()
                .status,
            CheckStatus::NotProbed
        );
    }

    #[test]
    fn missing_configuration_does_not_create_directory() {
        let fixture = Fixture::new();
        let report = collect(&fixture.environment(), None);
        assert_eq!(report.checks()[1].status, CheckStatus::Info);
        assert!(!fixture.root.join("jellypilot").exists());
    }

    #[test]
    fn oversized_configuration_is_rejected_without_reading_or_modifying_it() {
        let fixture = Fixture::new();
        let path = fixture.config(b"");
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_CONFIG_BYTES + 1)
            .unwrap();
        assert_eq!(read_config(&path).unwrap_err(), ConfigFinding::Oversized);
        assert_eq!(fs::metadata(path).unwrap().len(), MAX_CONFIG_BYTES + 1);
    }

    #[test]
    fn non_regular_configuration_is_reported_unreadable() {
        let fixture = Fixture::new();
        assert_eq!(
            read_config(&fixture.root).unwrap_err(),
            ConfigFinding::Unreadable
        );
    }

    #[test]
    fn unavailable_home_and_config_location_still_produce_a_safe_report() {
        let fixture = Fixture::new();
        let mut environment = fixture.environment();
        environment.home_present = false;
        environment.config_dir = None;
        let report = collect(&environment, None);
        assert!(report.has_blocking_findings());
        assert!(report
            .to_string()
            .contains("Home directory lookup unavailable"));
    }

    #[test]
    fn invalid_override_does_not_fall_back_to_readable_bundle() {
        let fixture = Fixture::new();
        let root = fixture.root.join("bin");
        let library = if cfg!(target_os = "windows") {
            "lib/jellypilot/libmpv-2.dll"
        } else {
            "lib/jellypilot/libmpv.so"
        };
        let library_path = root.join(library);
        fs::create_dir_all(library_path.parent().unwrap()).unwrap();
        fs::write(library_path, b"not loaded").unwrap();
        let mut environment = fixture.environment();
        environment.libmpv_override = Some(PathBuf::from("relative-untrusted-override"));
        let mut report = StartupReport { checks: Vec::new() };
        inspect_embedded(&environment, PlaybackBackend::Embedded, &mut report);
        if cfg!(any(target_os = "linux", target_os = "windows")) {
            assert_eq!(
                report
                    .checks
                    .iter()
                    .find(|check| check.name == "Embedded runtime selection")
                    .unwrap()
                    .status,
                CheckStatus::Issue
            );
        }
    }

    #[test]
    fn explicit_missing_configuration_is_not_replaced_with_platform_settings() {
        let fixture = Fixture::new();
        let path = fixture.config(&serde_json::to_vec(&Settings::default()).unwrap());
        let missing = fixture.root.join("missing.json");
        let report = collect(&fixture.environment(), Some(&missing));
        let check = report
            .checks()
            .iter()
            .find(|check| check.name == "Configuration")
            .unwrap();
        assert_eq!(check.status, CheckStatus::Issue);
        assert!(path.is_file());
        assert!(!missing.exists());
    }

    #[test]
    fn windows_configured_player_accepts_exe_suffix_and_application_directory() {
        let fixture = Fixture::new();
        let environment = fixture.environment();
        let directory = environment.executable_dir.as_ref().unwrap();
        fs::create_dir(directory).unwrap();
        let executable = directory.join("mpv.exe");
        fs::write(&executable, b"not executed").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert!(windows_configured_executable_available(
            &directory.join("mpv"),
            &environment
        ));
        assert!(windows_configured_executable_available(
            Path::new("mpv"),
            &environment
        ));
        assert!(!windows_configured_executable_available(
            Path::new("missing"),
            &environment
        ));
        assert!(!windows_configured_executable_available(
            Path::new("mpv.cmd"),
            &environment
        ));
    }
}
