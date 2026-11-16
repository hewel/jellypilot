use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::{process, util};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scenario {
  Tray,
  External,
  Gpu,
  All,
}

impl Scenario {
  fn name(self) -> &'static str {
    match self {
      Self::Tray => "tray",
      Self::External => "external",
      Self::Gpu => "gpu",
      Self::All => "all",
    }
  }

  fn selected(self) -> &'static [Self] {
    match self {
      Self::Tray => &[Self::Tray],
      Self::External => &[Self::External],
      Self::Gpu => &[Self::Gpu],
      Self::All => &[Self::Tray, Self::External, Self::Gpu],
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize)]
pub enum HostAbi {
  #[value(name = "legacy")]
  #[serde(rename = "legacy")]
  Legacy,
  #[value(name = "3")]
  #[serde(rename = "3")]
  Timeline,
}

impl HostAbi {
  fn name(self) -> &'static str {
    match self {
      Self::Legacy => "legacy",
      Self::Timeline => "3",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Hwdec {
  No,
  Vaapi,
  VaapiCopy,
}

impl Hwdec {
  fn name(self) -> &'static str {
    match self {
      Self::No => "no",
      Self::Vaapi => "vaapi",
      Self::VaapiCopy => "vaapi-copy",
    }
  }
}

#[derive(Debug, clap::Args)]
pub struct Args {
  #[arg(value_enum)]
  pub scenario: Scenario,
  #[arg(long)]
  pub file: Option<PathBuf>,
  #[arg(long = "out", default_value = "target/native-regression")]
  pub output: PathBuf,
  #[arg(long, value_enum)]
  pub hwdec: Option<Hwdec>,
  #[arg(long, value_enum, default_value = "legacy")]
  pub host_abi: HostAbi,
  #[arg(long = "duration-secs", value_parser = parse_duration)]
  pub duration_seconds: Option<u16>,
}

impl Args {
  pub fn validate(&self) -> Result<()> {
    let media_scenario = matches!(self.scenario, Scenario::Gpu | Scenario::All);
    ensure!(
      !media_scenario || self.file.is_some(),
      "gpu/all requires --file <media>; no media or color coverage is fabricated."
    );
    ensure!(
      media_scenario || self.file.is_none(),
      "--file applies only to iced regress gpu/all."
    );
    ensure!(
      media_scenario || self.hwdec.is_none(),
      "--hwdec applies only to iced regress gpu/all."
    );
    ensure!(
      self.duration_seconds.is_none() || self.scenario == Scenario::Gpu,
      "--duration-secs applies only to iced regress gpu, not tray/external/all."
    );
    if let Some(duration) = self.duration_seconds {
      ensure!(
        (1..=3600).contains(&duration),
        "--duration-secs must be from 1 to 3600."
      );
    }
    ensure!(
      !self.output.as_os_str().to_string_lossy().trim().is_empty(),
      "--out requires a non-empty path."
    );
    if let Some(file) = &self.file {
      ensure!(
        !file.as_os_str().to_string_lossy().trim().is_empty(),
        "--file requires a non-empty path."
      );
    }
    Ok(())
  }
}

fn parse_duration(value: &str) -> std::result::Result<u16, String> {
  if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
    if let Ok(duration) = value.parse::<u16>() {
      if (1..=3600).contains(&duration) {
        return Ok(duration);
      }
    }
  }
  Err("--duration-secs requires ASCII decimal digits for an integer from 1 to 3600".into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
  Pass,
  Fail,
  Unavailable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioResult {
  schema_version: u8,
  run_id: String,
  scenario: Scenario,
  requested_host_abi: HostAbi,
  actual_host_abi: Option<u32>,
  status: Status,
  checks: Vec<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  error: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  native: Option<Value>,
}

impl ScenarioResult {
  fn unavailable(
    scenario: Scenario,
    run_id: &str,
    host_abi: HostAbi,
    error: impl Into<String>,
  ) -> Self {
    Self {
      schema_version: 1,
      run_id: run_id.to_owned(),
      scenario,
      requested_host_abi: host_abi,
      actual_host_abi: None,
      status: Status::Unavailable,
      checks: Vec::new(),
      error: Some(error.into()),
      native: None,
    }
  }
}

fn integer(value: &Value, maximum: u64) -> Option<u64> {
  value
    .as_f64()
    .filter(|value| {
      value.is_finite() && value.fract() == 0.0 && *value >= 0.0 && *value <= maximum as f64
    })
    .map(|value| value as u64)
}

fn native_report(
  value: Value,
  scenario: Scenario,
  run_id: &str,
  host_abi: HostAbi,
  duration: Option<u16>,
) -> Result<ScenarioResult> {
  let invalid = "Native regression report is missing, invalid, or belongs to a different run.";
  ensure!(
    value.is_object()
      && value["schemaVersion"].as_f64() == Some(1.0)
      && value["runId"].as_str() == Some(run_id)
      && value["scenario"].as_str() == Some(scenario.name()),
    "{invalid}"
  );
  let status = match value["status"].as_str() {
    Some("pass") => Status::Pass,
    Some("fail") => Status::Fail,
    Some("unavailable") => Status::Unavailable,
    _ => bail!("{invalid}"),
  };
  let checks = value["checks"]
    .as_array()
    .context(invalid)?
    .iter()
    .map(|check| check.as_str().map(str::to_owned).context(invalid))
    .collect::<Result<Vec<_>>>()?;
  let requested_abi = match value["requestedHostAbi"].as_str() {
    Some("legacy") => HostAbi::Legacy,
    Some("3") => HostAbi::Timeline,
    _ => bail!("{invalid}"),
  };
  let actual = value.get("actualHostAbi").context(invalid)?;
  let actual_host_abi = if actual.is_null() {
    None
  } else {
    Some(
      integer(actual, u64::from(u32::MAX))
        .filter(|value| *value >= 1)
        .context(invalid)? as u32,
    )
  };
  let abi_matches = match host_abi {
    HostAbi::Timeline => actual_host_abi == Some(3),
    HostAbi::Legacy => matches!(actual_host_abi, Some(1 | 2)),
  };
  let abi_error = if requested_abi != host_abi {
    Some(format!(
      "Native report requested ABI {}, expected {}.",
      requested_abi.name(),
      host_abi.name()
    ))
  } else if status == Status::Pass && !abi_matches {
    Some(format!(
      "Native report claimed pass with actual host ABI {}, expected {}.",
      actual_host_abi.map_or_else(|| "null".into(), |abi| abi.to_string()),
      if host_abi == HostAbi::Timeline {
        "3"
      } else {
        "1 or 2"
      }
    ))
  } else {
    None
  };
  let sustained = value.get("sustained").filter(|value| !value.is_null());
  let sustained_error = if status != Status::Pass {
    None
  } else if let Some(duration) = duration {
    let valid = scenario == Scenario::Gpu
      && sustained.is_some_and(|sustained| {
        sustained.is_object()
          && sustained["requestedSeconds"].as_f64() == Some(f64::from(duration))
          && sustained["elapsedSeconds"]
            .as_f64()
            .is_some_and(|elapsed| elapsed.is_finite() && elapsed >= f64::from(duration))
          && integer(&sustained["completedCycles"], 9_007_199_254_740_991)
            .is_some_and(|cycles| cycles >= 1)
      });
    (!valid).then(|| format!("Native report claimed pass without completing a GPU cycle after at least {duration} seconds."))
  } else {
    sustained.map(|_| "Native report claimed sustained success for a default short run.".to_owned())
  };
  let report_error = abi_error.or(sustained_error);
  Ok(ScenarioResult {
    schema_version: 1,
    run_id: run_id.to_owned(),
    scenario,
    requested_host_abi: host_abi,
    actual_host_abi,
    status: if report_error.is_some() {
      Status::Fail
    } else {
      status
    },
    checks,
    error: report_error.or_else(|| value["error"].as_str().map(str::to_owned)),
    native: Some(value),
  })
}

fn audio_session_environment(environment: &BTreeMap<String, String>) -> BTreeMap<String, String> {
  let mut audio = BTreeMap::new();
  if let Some(runtime) = environment
    .get("PIPEWIRE_RUNTIME_DIR")
    .or_else(|| environment.get("XDG_RUNTIME_DIR"))
  {
    audio.insert("PIPEWIRE_RUNTIME_DIR".into(), runtime.clone());
  }
  if let Some(runtime) = environment.get("PULSE_RUNTIME_PATH") {
    audio.insert("PULSE_RUNTIME_PATH".into(), runtime.clone());
  } else if let Some(runtime) = environment
    .get("XDG_RUNTIME_DIR")
    .filter(|value| !value.is_empty())
  {
    audio.insert(
      "PULSE_RUNTIME_PATH".into(),
      Path::new(runtime)
        .join("pulse")
        .to_string_lossy()
        .into_owned(),
    );
  }
  for name in ["PIPEWIRE_REMOTE", "PULSE_SERVER"] {
    if let Some(value) = environment.get(name) {
      audio.insert(name.into(), value.clone());
    }
  }
  audio
}

fn display_environment(environment: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
  let wayland = environment
    .get("WAYLAND_DISPLAY")
    .filter(|value| !value.is_empty());
  let runtime = environment
    .get("XDG_RUNTIME_DIR")
    .filter(|value| !value.is_empty());
  ensure!((wayland.is_some() || environment.get("DISPLAY").is_some_and(|value| !value.is_empty()))
    && !wayland.is_some_and(|wayland| !Path::new(wayland).is_absolute() && runtime.is_none()),
    "Native tray/GPU probes require the current display and its Wayland runtime directory (or DISPLAY).");
  let mut display = BTreeMap::new();
  if let Some(wayland) = wayland {
    display.insert(
      "WAYLAND_DISPLAY".into(),
      absolute_path(
        Path::new(runtime.map_or("/", String::as_str)),
        Path::new(wayland),
      )
      .to_string_lossy()
      .into_owned(),
    );
  }
  Ok(display)
}

fn child(
  arguments: &[String],
  environment: &BTreeMap<String, String>,
  timeout: Duration,
  headless: bool,
) -> Result<Output> {
  let mut command = process::command("dbus-run-session");
  command.arg("--");
  if headless {
    command.args(["xvfb-run", "-a"]);
  }
  command
    .arg(crate::root().join("target/debug/jellypilot"))
    .args(arguments)
    .envs(environment);
  process::run_with(
    &mut command,
    process::RunOptions {
      capture: headless,
      allow_failure: true,
      timeout: Some(timeout),
    },
  )
}

fn run_scenario(
  args: &Args,
  scenario: Scenario,
  run_id: &str,
  media: Option<&Path>,
  report: &Path,
  environment: &BTreeMap<String, String>,
) -> Result<ScenarioResult> {
  let directory = tempfile::Builder::new()
    .prefix(".runtime-")
    .tempdir_in(report.parent().context("Regression report has no parent")?)?;
  let runtime = directory.path().join("runtime");
  fs::create_dir(&runtime)?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
  }
  let mut isolated = BTreeMap::from([
    (
      "XDG_CONFIG_HOME".into(),
      directory
        .path()
        .join("config")
        .to_string_lossy()
        .into_owned(),
    ),
    (
      "XDG_DATA_HOME".into(),
      directory.path().join("data").to_string_lossy().into_owned(),
    ),
    (
      "XDG_CACHE_HOME".into(),
      directory
        .path()
        .join("cache")
        .to_string_lossy()
        .into_owned(),
    ),
    (
      "XDG_RUNTIME_DIR".into(),
      runtime.to_string_lossy().into_owned(),
    ),
    ("GIO_USE_VFS".into(), "local".into()),
    ("NO_AT_BRIDGE".into(), "1".into()),
    ("WINIT_X11_SCALE_FACTOR".into(), "1".into()),
    ("JELLYPILOT_SMOKE_SIZE".into(), "480x320".into()),
    (
      "JELLYPILOT_MPV_HOST_ABI".into(),
      args.host_abi.name().into(),
    ),
  ]);
  if scenario == Scenario::External {
    isolated.extend([
      ("WAYLAND_DISPLAY".into(), String::new()),
      ("WINIT_UNIX_BACKEND".into(), "x11".into()),
      (
        "JELLYPILOT_LIBMPV".into(),
        directory
          .path()
          .join("absent-libmpv.so")
          .to_string_lossy()
          .into_owned(),
      ),
      (
        "JELLYPILOT_MPV_BASELINE".into(),
        directory
          .path()
          .join("absent-baseline.conf")
          .to_string_lossy()
          .into_owned(),
      ),
      (
        "VK_DRIVER_FILES".into(),
        directory
          .path()
          .join("absent-vulkan-driver.json")
          .to_string_lossy()
          .into_owned(),
      ),
    ]);
    let rejected = child(
      &["--smoke-test".into(), "--embedded".into()],
      &isolated,
      Duration::from_secs(60),
      true,
    )?;
    let mut result = ScenarioResult::unavailable(
      scenario,
      run_id,
      args.host_abi,
      "Missing embedded assets did not produce the expected explicit startup rejection.",
    );
    result.status = Status::Fail;
    if rejected.status.success()
      || !String::from_utf8_lossy(&rejected.stderr).contains("Embedded MPV asset missing:")
    {
      return Ok(result);
    }
    let recovered = child(
      &["--smoke-test".into(), "--external".into()],
      &isolated,
      Duration::from_secs(60),
      true,
    )?;
    result.status = if recovered.status.success() {
      Status::Pass
    } else {
      Status::Fail
    };
    result
      .checks
      .push("missing embedded assets rejected without a system libmpv fallback".into());
    if recovered.status.success() {
      result.checks.extend([
        "explicit External startup succeeds with missing embedded library, baseline and Vulkan driver".into(),
        "daemon exits successfully".into(),
      ]);
      result.error = None;
    } else {
      result.error = Some(format!(
        "External recovery exited with {}.",
        recovered.status
      ));
    }
    return Ok(result);
  }
  let display = match display_environment(environment) {
    Ok(display) => display,
    Err(error) => {
      return Ok(ScenarioResult::unavailable(
        scenario,
        run_id,
        args.host_abi,
        error.to_string(),
      ))
    }
  };
  let assets = match crate::mpv::embedded_environment() {
    Ok(assets) => assets,
    Err(error) => {
      return Ok(ScenarioResult::unavailable(
        scenario,
        run_id,
        args.host_abi,
        format!("{error:#}"),
      ))
    }
  };
  let locales = process::capture(process::command("locale").arg("-a"))?;
  let locale_text = String::from_utf8_lossy(&locales.stdout);
  let numeric_locale = locale_text.lines().find(|locale| {
    let lower = locale.to_ascii_lowercase();
    !locale.is_empty()
      && !["c", "posix"].into_iter().any(|prefix| {
        lower
          .strip_prefix(prefix)
          .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '@']))
      })
  });
  let Some(numeric_locale) = numeric_locale else {
    return Ok(ScenarioResult::unavailable(
      scenario,
      run_id,
      args.host_abi,
      "No installed non-C locale; install one before running the tray/GPU regression.",
    ));
  };
  let mut arguments = vec![
    "--smoke-test".into(),
    "--embedded".into(),
    "--native-regression".into(),
    scenario.name().into(),
    "--regression-report".into(),
    report.to_string_lossy().into_owned(),
    "--regression-run-id".into(),
    run_id.into(),
  ];
  if scenario == Scenario::Gpu {
    if let Some(media) = media {
      arguments.extend([
        "--regression-media".into(),
        media.to_string_lossy().into_owned(),
      ]);
    }
    if let Some(hwdec) = args.hwdec {
      arguments.extend(["--regression-hwdec".into(), hwdec.name().into()]);
    }
  }
  if let Some(duration) = args.duration_seconds {
    arguments.extend(["--regression-duration-secs".into(), duration.to_string()]);
  }
  isolated.extend(assets);
  isolated.extend(display);
  isolated.extend(audio_session_environment(environment));
  isolated.insert("LC_ALL".into(), numeric_locale.into());
  let process_result = child(
    &arguments,
    &isolated,
    Duration::from_secs(u64::from(args.duration_seconds.unwrap_or(0)) + 180),
    false,
  )?;
  let value = util::read_json(report)?;
  let mut decoded = native_report(
    value,
    scenario,
    run_id,
    args.host_abi,
    args.duration_seconds,
  )?;
  if decoded.status == Status::Pass && !process_result.status.success() {
    decoded.status = Status::Fail;
    decoded.error = Some(format!(
      "Native assertions reported pass but process exited with {}.",
      process_result.status
    ));
  } else if decoded.status == Status::Pass {
    decoded.checks.extend([
      "daemon exits successfully".into(),
      format!("non-C environment locale: {numeric_locale}"),
    ]);
  }
  Ok(decoded)
}

fn absolute_path(root: &Path, path: &Path) -> PathBuf {
  let mut output = PathBuf::new();
  for component in root.join(path).components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir => {
        output.pop();
      }
      other => output.push(other.as_os_str()),
    }
  }
  output
}

fn validate_media_file(path: &Path) -> Result<()> {
  ensure!(
    fs::metadata(path)
      .with_context(|| format!("Could not inspect regression media: {}", path.display()))?
      .is_file(),
    "Regression media must be a readable regular local file."
  );
  let mut options = File::options();
  options.read(true);
  #[cfg(target_os = "linux")]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(nix::libc::O_NONBLOCK);
  }
  let file = options.open(path).with_context(|| {
    format!(
      "Regression media must be a readable regular local file: {}",
      path.display()
    )
  })?;
  ensure!(
    file.metadata()?.is_file(),
    "Regression media must be a readable regular local file."
  );
  Ok(())
}

struct RunReport<'a> {
  args: &'a Args,
  run_id: String,
  started_at: String,
  environment: BTreeMap<String, String>,
  results: Vec<ScenarioResult>,
  sources: Option<Value>,
  media: Option<Value>,
  launcher_sha256: Option<String>,
}

impl RunReport<'_> {
  fn write(&self, output: &Path, finished: bool) -> Result<()> {
    let mut report = json!({
      "schemaVersion": 1, "runId": self.run_id, "startedAt": self.started_at,
      "requestedHostAbi": self.args.host_abi, "requestedDurationSeconds": self.args.duration_seconds,
      "platform": if cfg!(target_os = "windows") { "win32" } else { std::env::consts::OS },
      "architecture": match std::env::consts::ARCH { "x86_64" => "x64", "aarch64" => "arm64", "x86" => "ia32", other => other },
      "sources": self.sources,
      "displayEnvironment": {
        "display": self.environment.get("DISPLAY"), "waylandDisplay": self.environment.get("WAYLAND_DISPLAY"),
        "runtimeDirectory": self.environment.get("XDG_RUNTIME_DIR"), "vulkanDriverFiles": self.environment.get("VK_DRIVER_FILES"),
        "vulkanIcdFilenames": self.environment.get("VK_ICD_FILENAMES"),
      },
      "media": self.media, "launcherSha256": self.launcher_sha256,
      "status": if self.results.iter().any(|result| result.status == Status::Fail) { Status::Fail }
        else if self.results.iter().any(|result| result.status == Status::Unavailable) { Status::Unavailable } else { Status::Pass },
      "results": self.results,
      "manualColorComparison": {
        "status": "unavailable",
        "reason": "Automatic lifecycle probes do not perform human three-way SDR/HDR10/Profile5 color acceptance. References are never updated by this command.",
      },
    });
    if finished {
      report["finishedAt"] = json!(util::now_iso());
    }
    util::write_json(&output.join("report.json"), &report)
  }

  fn prepare(&mut self, media: Option<&Path>) -> Result<()> {
    ensure!(cfg!(target_os = "linux"), "Native regressions require Linux, a native display for tray/GPU, and a private D-Bus session.");
    let root = crate::root();
    let environment = &self.environment;
    ensure!(!environment.get("CARGO_TARGET_DIR").is_some_and(|path| absolute_path(root, Path::new(path)) != absolute_path(root, Path::new("target")))
      && !environment.get("CARGO_BUILD_TARGET").is_some_and(|value| !value.is_empty()),
      "Native regression requires the normal target/debug host build; unset CARGO_TARGET_DIR/CARGO_BUILD_TARGET overrides.");
    let application = process::capture(process::command("git").args(["rev-parse", "HEAD"]))?;
    let changes = process::capture(process::command("git").args(["status", "--porcelain"]))?;
    let assets = (|| -> Result<Value> {
      let paths = crate::mpv::embedded_environment()?;
      let library = paths
        .get("JELLYPILOT_LIBMPV")
        .context("Missing embedded library path")?;
      let baseline = paths
        .get("JELLYPILOT_MPV_BASELINE")
        .context("Missing embedded baseline path")?;
      Ok(json!({
        "library": { "path": library, "sha256": util::sha256_file(Path::new(library))? },
        "baseline": { "path": baseline, "sha256": util::sha256_file(Path::new(baseline))? },
      }))
    })()
    .unwrap_or_else(|error| json!({ "status": "unavailable", "error": format!("{error:#}") }));
    let manifest = root.join("target/embedded-mpv/manifest.json");
    self.sources = Some(json!({
      "application": {
        "revision": String::from_utf8_lossy(&application.stdout).trim(),
        "workingTree": String::from_utf8_lossy(&changes.stdout),
        "dispatcherSha256": util::sha256_file(&root.join("xtask/src/regression.rs"))?,
      },
      "iced": util::read_json::<Value>(&root.join("tools/embedded-mpv/iced-source.json"))?,
      "mpv": util::read_json::<Value>(&root.join("tools/embedded-mpv/source.json"))?,
      "cargoLockSha256": util::sha256_file(&root.join("Cargo.lock"))?,
      "desktopCargoLockSha256": util::sha256_file(&root.join("src-iced/Cargo.lock"))?,
      "nativeBuild": if manifest.try_exists()? { Some(util::read_json::<Value>(&manifest)?) } else { None },
      "actualEmbeddedAssets": assets,
    }));
    if let Some(media) = media {
      let metadata = process::capture(process::command("ffprobe").args([
        "-v", "error", "-show_entries",
        "format=duration,format_name,size:stream=index,codec_type,codec_name,width,height,pix_fmt,color_range,color_space,color_transfer,color_primaries:stream_side_data",
        "-of", "json",
      ]).arg(media))?;
      self.media = Some(json!({
        "name": media.file_name().context("Media path has no filename")?.to_string_lossy(),
        "sha256": util::sha256_file(media)?,
        "metadata": serde_json::from_slice::<Value>(&metadata.stdout).context("ffprobe returned invalid media metadata")?,
      }));
    }
    crate::iced::prepare(None)?;
    crate::iced::build(false)?;
    self.launcher_sha256 = Some(util::sha256_file(&root.join("target/debug/jellypilot"))?);
    Ok(())
  }
}

pub fn run(args: Args) -> Result<()> {
  // Validation must precede output creation, dependency preparation, builds and subprocesses.
  args.validate()?;
  let media = args
    .file
    .as_ref()
    .map(|file| absolute_path(crate::root(), file));
  if let Some(media) = &media {
    validate_media_file(media)?;
  }
  let output = absolute_path(crate::root(), &args.output);
  fs::create_dir_all(&output)?;
  let run_id = uuid::Uuid::new_v4().to_string();
  let mut report = RunReport {
    results: args
      .scenario
      .selected()
      .iter()
      .map(|scenario| {
        ScenarioResult::unavailable(
          *scenario,
          &run_id,
          args.host_abi,
          "Not executed in this run.",
        )
      })
      .collect(),
    args: &args,
    run_id,
    started_at: util::now_iso(),
    environment: std::env::vars().collect(),
    sources: None,
    media: None,
    launcher_sha256: None,
  };
  for result in &report.results {
    util::write_json(
      &output.join(format!("{}.json", result.scenario.name())),
      result,
    )?;
  }
  report.write(&output, false)?;
  match report.prepare(media.as_deref()) {
    Err(error) => {
      for result in &mut report.results {
        result.error = Some(format!("{error:#}"));
      }
    }
    Ok(()) => {
      for (index, scenario) in args.scenario.selected().iter().copied().enumerate() {
        let path = output.join(format!("{}.json", scenario.name()));
        let result = run_scenario(
          &args,
          scenario,
          &report.run_id,
          media.as_deref(),
          &path,
          &report.environment,
        )
        .unwrap_or_else(|error| {
          let mut result = ScenarioResult::unavailable(
            scenario,
            &report.run_id,
            args.host_abi,
            format!("{error:#}"),
          );
          result.status = Status::Fail;
          result
        });
        report.results[index] = result;
        util::write_json(&path, &report.results[index])?;
        report.write(&output, false)?;
        if process::interrupted() {
          break;
        }
      }
    }
  }
  for result in &report.results {
    util::write_json(
      &output.join(format!("{}.json", result.scenario.name())),
      result,
    )?;
  }
  report.write(&output, true)?;
  println!(
    "Native regression {}: {}",
    report.run_id,
    output.join("report.json").display()
  );
  ensure!(
    report
      .results
      .iter()
      .all(|result| result.status == Status::Pass),
    "Native regression did not pass every requested scenario; inspect the current run report."
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use clap::Parser;

  const RUN_ID: &str = "9389f864-d104-424a-aef9-fc3098201923";

  fn timeline_report() -> Value {
    json!({
      "schemaVersion": 1, "runId": RUN_ID, "scenario": "gpu", "status": "pass",
      "requestedHostAbi": "3", "actualHostAbi": 3,
      "checks": ["stop acknowledged", "retained compositor cleanup completed"],
      "decoderSamples": [{ "hwdec-current": "vaapi", "current-ao": "pipewire" }],
    })
  }

  fn decode(value: Value, duration: Option<u16>) -> ScenarioResult {
    native_report(value, Scenario::Gpu, RUN_ID, HostAbi::Timeline, duration).unwrap()
  }

  #[test]
  fn timeline_pass_requires_actual_abi_three() {
    for actual in [json!(1), json!(2), Value::Null] {
      let mut value = timeline_report();
      value["actualHostAbi"] = actual;
      assert_eq!(decode(value, None).status, Status::Fail);
    }
  }

  #[test]
  fn legacy_pass_requires_actual_abi_one_or_two() {
    for actual in [1, 2, 3] {
      let mut value = timeline_report();
      value["requestedHostAbi"] = json!("legacy");
      value["actualHostAbi"] = json!(actual);
      let decoded = native_report(value, Scenario::Gpu, RUN_ID, HostAbi::Legacy, None).unwrap();
      assert_eq!(
        decoded.status,
        if actual == 3 {
          Status::Fail
        } else {
          Status::Pass
        }
      );
    }
  }

  #[test]
  fn mismatched_requested_abi_cannot_satisfy_the_run() {
    let mut value = timeline_report();
    value["requestedHostAbi"] = json!("legacy");
    assert_eq!(decode(value, None).status, Status::Fail);
  }

  #[test]
  fn unavailable_prerequisites_do_not_manufacture_abi_or_sustained_evidence() {
    for status in ["unavailable", "fail"] {
      let mut value = timeline_report();
      value["status"] = json!(status);
      value["actualHostAbi"] = Value::Null;
      value["error"] = json!("No native hardware Vulkan adapter");
      value["sustained"] =
        json!({"requestedSeconds": 60, "elapsedSeconds": 12, "completedCycles": 0});
      let decoded = decode(value.clone(), Some(60));
      assert_eq!(
        decoded.error.as_deref(),
        Some("No native hardware Vulkan adapter")
      );
      assert_eq!(decoded.native, Some(value));
      assert_eq!(decoded.actual_host_abi, None);
      assert_eq!(
        decoded.status,
        if status == "fail" {
          Status::Fail
        } else {
          Status::Unavailable
        }
      );
    }
  }

  #[test]
  fn stale_or_malformed_native_reports_are_rejected() {
    for (key, invalid) in [
      ("runId", json!("previous-run")),
      ("scenario", json!("tray")),
      ("schemaVersion", json!(2)),
      ("checks", json!(["good", 3])),
      ("actualHostAbi", json!("3")),
      ("actualHostAbi", json!(2.5)),
      ("actualHostAbi", json!(0)),
      ("actualHostAbi", json!(4_294_967_296_u64)),
      ("requestedHostAbi", json!(3)),
      ("status", json!("success")),
    ] {
      let mut value = timeline_report();
      value[key] = invalid;
      assert!(
        native_report(value, Scenario::Gpu, RUN_ID, HostAbi::Timeline, None).is_err(),
        "accepted invalid {key}"
      );
    }
    for missing in ["actualHostAbi", "requestedHostAbi", "checks", "runId"] {
      let mut value = timeline_report();
      value.as_object_mut().unwrap().remove(missing);
      assert!(native_report(value, Scenario::Gpu, RUN_ID, HostAbi::Timeline, None).is_err());
    }
  }

  #[test]
  fn sustained_success_requires_elapsed_minimum_and_a_completed_cycle() {
    for elapsed in [60.0, 62.5] {
      let mut value = timeline_report();
      value["sustained"] =
        json!({"requestedSeconds": 60, "elapsedSeconds": elapsed, "completedCycles": 1});
      assert_eq!(decode(value, Some(60)).status, Status::Pass);
    }
  }

  #[test]
  fn sustained_pass_rejects_wrong_duration_missing_evidence_and_fabricated_cycles() {
    let invalid = [
      Value::Null,
      json!({}),
      json!(60),
      json!({"requestedSeconds": 30, "elapsedSeconds": 62, "completedCycles": 2}),
      json!({"requestedSeconds": "60", "elapsedSeconds": 62, "completedCycles": 2}),
      json!({"requestedSeconds": 60, "completedCycles": 1}),
      json!({"requestedSeconds": 60, "elapsedSeconds": 60}),
      json!({"requestedSeconds": 60, "elapsedSeconds": 59.999, "completedCycles": 1}),
      json!({"requestedSeconds": 60, "elapsedSeconds": -1, "completedCycles": 1}),
      json!({"requestedSeconds": 60, "elapsedSeconds": "60", "completedCycles": 1}),
      json!({"requestedSeconds": 60, "elapsedSeconds": 60, "completedCycles": 0}),
      json!({"requestedSeconds": 60, "elapsedSeconds": 60, "completedCycles": -1}),
      json!({"requestedSeconds": 60, "elapsedSeconds": 60, "completedCycles": 0.5}),
      json!({"requestedSeconds": 60, "elapsedSeconds": 60, "completedCycles": "1"}),
      json!({"requestedSeconds": 60, "elapsedSeconds": 60, "completedCycles": 9_007_199_254_740_992_u64}),
    ];
    assert_eq!(decode(timeline_report(), Some(60)).status, Status::Fail);
    for sustained in invalid {
      let mut value = timeline_report();
      value["sustained"] = sustained;
      assert_eq!(decode(value, Some(60)).status, Status::Fail);
    }
  }

  #[test]
  fn short_runs_reject_sustained_success_but_allow_explicit_null() {
    let mut value = timeline_report();
    value["sustained"] =
      json!({"requestedSeconds": 60, "elapsedSeconds": 60, "completedCycles": 1});
    assert_eq!(decode(value.clone(), None).status, Status::Fail);
    value["sustained"] = Value::Null;
    assert_eq!(decode(value, None).status, Status::Pass);
  }

  #[derive(Debug, Parser)]
  struct Cli {
    #[command(flatten)]
    args: Args,
  }

  #[test]
  fn sustained_cli_rejects_non_exact_integer_spelling_and_duplicates() {
    for value in [
      "",
      "0",
      "3601",
      "-1",
      "+1",
      " 60",
      "60 ",
      "1.5",
      "1e2",
      "٦٠",
      "99999999999999999999999",
    ] {
      assert!(
        Cli::try_parse_from([
          "probe",
          "gpu",
          "--file",
          "clip.mkv",
          "--duration-secs",
          value
        ])
        .is_err(),
        "accepted {value:?}"
      );
    }
    assert!(Cli::try_parse_from([
      "probe",
      "gpu",
      "--file",
      "clip.mkv",
      "--duration-secs",
      "1",
      "--duration-secs",
      "2"
    ])
    .is_err());
    for value in ["1", "3600", "0060"] {
      assert!(Cli::try_parse_from([
        "probe",
        "gpu",
        "--file",
        "clip.mkv",
        "--duration-secs",
        value
      ])
      .unwrap()
      .args
      .validate()
      .is_ok());
    }
  }

  #[test]
  fn cli_relationships_are_validated_before_side_effects() {
    for argv in [
      vec!["probe", "gpu"],
      vec!["probe", "all"],
      vec!["probe", "tray", "--file", "clip.mkv"],
      vec!["probe", "external", "--hwdec", "no"],
      vec!["probe", "tray", "--duration-secs", "1"],
      vec!["probe", "external", "--duration-secs", "1"],
      vec!["probe", "all", "--file", "clip.mkv", "--duration-secs", "1"],
      vec!["probe", "tray", "--out", " "],
    ] {
      let args = Cli::try_parse_from(argv).unwrap().args;
      assert!(args.validate().is_err());
    }
  }

  #[test]
  fn invalid_media_is_rejected_without_creating_reports_or_starting_preparation() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("reports");
    let args = Args {
      scenario: Scenario::Gpu,
      file: Some(root.path().join("missing.mkv")),
      output: output.clone(),
      hwdec: None,
      host_abi: HostAbi::Legacy,
      duration_seconds: None,
    };
    assert!(run(args).is_err());
    assert!(!output.exists());
  }

  #[test]
  fn local_media_validation_rejects_missing_paths_and_directories() {
    let root = tempfile::tempdir().unwrap();
    assert!(validate_media_file(&root.path().join("missing.mkv")).is_err());
    assert!(validate_media_file(root.path()).is_err());
    fs::write(root.path().join("clip.mkv"), "media").unwrap();
    assert!(validate_media_file(&root.path().join("clip.mkv")).is_ok());
  }

  #[test]
  fn display_lookup_preserves_session_socket_while_runtime_is_private() {
    let environment = BTreeMap::from([
      ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
      ("XDG_RUNTIME_DIR".into(), "/run/user/1000".into()),
    ]);
    assert_eq!(
      display_environment(&environment).unwrap()["WAYLAND_DISPLAY"],
      "/run/user/1000/wayland-1"
    );
    assert!(display_environment(&BTreeMap::from([(
      "WAYLAND_DISPLAY".into(),
      "wayland-1".into()
    )]))
    .is_err());
    assert!(
      display_environment(&BTreeMap::from([("DISPLAY".into(), ":0".into())]))
        .unwrap()
        .is_empty()
    );
  }

  #[test]
  fn explicit_audio_routing_is_opaque_and_never_replaced() {
    let explicit = BTreeMap::from([
      (
        "PIPEWIRE_RUNTIME_DIR".into(),
        "/session/custom-pipewire".into(),
      ),
      (
        "PIPEWIRE_REMOTE".into(),
        "[ studio, /absolute/socket ]".into(),
      ),
      ("PULSE_RUNTIME_PATH".into(), "/session/custom-pulse".into()),
      (
        "PULSE_SERVER".into(),
        "unix:/session/custom-pulse/native".into(),
      ),
    ]);
    let mut environment = explicit.clone();
    environment.insert("XDG_RUNTIME_DIR".into(), "/other-session".into());
    assert_eq!(audio_session_environment(&environment), explicit);
    assert!(audio_session_environment(&BTreeMap::new()).is_empty());
  }

  #[cfg(unix)]
  #[test]
  fn audio_endpoints_remain_reachable_after_runtime_isolation() {
    use std::os::unix::net::{UnixListener, UnixStream};
    let root = tempfile::tempdir().unwrap();
    let session = root.path().join("session");
    let private = root.path().join("private");
    fs::create_dir_all(session.join("pulse")).unwrap();
    fs::create_dir(&private).unwrap();
    let _pipewire = UnixListener::bind(session.join("pipewire-0")).unwrap();
    let _pulse = UnixListener::bind(session.join("pulse/native")).unwrap();
    let environment = BTreeMap::from([(
      "XDG_RUNTIME_DIR".into(),
      session.to_string_lossy().into_owned(),
    )]);
    let audio = audio_session_environment(&environment);
    let mut isolated = BTreeMap::from([(
      "XDG_RUNTIME_DIR".into(),
      private.to_string_lossy().into_owned(),
    )]);
    isolated.extend(audio);
    UnixStream::connect(Path::new(&isolated["PIPEWIRE_RUNTIME_DIR"]).join("pipewire-0")).unwrap();
    UnixStream::connect(Path::new(&isolated["PULSE_RUNTIME_PATH"]).join("native")).unwrap();
    assert_eq!(Path::new(&isolated["XDG_RUNTIME_DIR"]), private);
  }

  #[test]
  fn result_serialization_keeps_null_abi_and_omits_absent_optional_fields() {
    let value = serde_json::to_value(ScenarioResult::unavailable(
      Scenario::Gpu,
      RUN_ID,
      HostAbi::Timeline,
      "no adapter",
    ))
    .unwrap();
    assert!(value["actualHostAbi"].is_null());
    assert!(value.get("native").is_none());
    assert_eq!(value["requestedHostAbi"], "3");
    let pass = serde_json::to_value(decode(timeline_report(), None)).unwrap();
    assert!(pass.get("error").is_none());
    assert_eq!(
      pass["native"]["decoderSamples"][0]["hwdec-current"],
      "vaapi"
    );
  }
}
