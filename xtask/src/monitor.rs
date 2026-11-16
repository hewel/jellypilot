use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::json;

use crate::{process, util};

#[derive(Debug, clap::Args)]
pub struct Args {
  #[arg(long, value_parser = parse_pid)]
  pub pid: u32,
  #[arg(long, default_value = "301", value_parser = parse_positive)]
  pub samples: u64,
  #[arg(long, default_value = "1000", value_parser = parse_positive)]
  pub interval_ms: u64,
  #[arg(long = "out")]
  pub output: PathBuf,
  #[arg(long)]
  pub label: Option<String>,
}

fn parse_positive(value: &str) -> std::result::Result<u64, String> {
  if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
    return Err("requires a positive ASCII decimal integer".into());
  }
  value
    .parse::<u64>()
    .ok()
    .filter(|value| *value > 0 && *value <= 9_007_199_254_740_991)
    .ok_or_else(|| "requires a positive safe integer".into())
}

fn parse_pid(value: &str) -> std::result::Result<u32, String> {
  let pid = parse_positive(value)?;
  u32::try_from(pid).map_err(|_| "process id exceeds the supported range".into())
}

#[derive(Debug, PartialEq)]
struct ProcSnapshot {
  name: String,
  utime_ticks: u64,
  stime_ticks: u64,
  start_time_ticks: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryState {
  available: bool,
  #[serde(rename = "rssKiB")]
  rss_kib: Option<u64>,
  #[serde(rename = "pssKiB")]
  pss_kib: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  reason: Option<String>,
}

impl MemoryState {
  fn unavailable(reason: String) -> Self {
    Self {
      available: false,
      rss_kib: None,
      pss_kib: None,
      reason: Some(reason),
    }
  }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GpuMetricState {
  available: bool,
  resident_bytes: Option<u64>,
  engine_nanoseconds: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  reason: Option<String>,
}

impl GpuMetricState {
  fn unavailable(reason: String) -> Self {
    Self {
      available: false,
      resident_bytes: None,
      engine_nanoseconds: None,
      reason: Some(reason),
    }
  }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessObservation {
  elapsed_milliseconds: f64,
  name: String,
  start_time_ticks: u64,
  memory: MemoryState,
  cpu_time_milliseconds: f64,
  context_switches: u64,
  gpu: GpuMetricState,
}

#[derive(Debug, Serialize)]
struct NumericSummary {
  mean: f64,
  median: f64,
  p95: f64,
  max: f64,
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
  if sorted.is_empty() {
    return 0.0;
  }
  let index = ((fraction * sorted.len() as f64).ceil() as usize)
    .saturating_sub(1)
    .min(sorted.len() - 1);
  sorted[index]
}

fn numeric_summary(mut values: Vec<f64>) -> Option<NumericSummary> {
  if values.is_empty() {
    return None;
  }
  values.sort_by(f64::total_cmp);
  Some(NumericSummary {
    mean: values.iter().sum::<f64>() / values.len() as f64,
    median: percentile(&values, 0.5),
    p95: percentile(&values, 0.95),
    max: values[values.len() - 1],
  })
}

fn summarize_observations(observations: &[ProcessObservation]) -> serde_json::Value {
  let rss = numeric_summary(
    observations
      .iter()
      .filter(|sample| sample.memory.available)
      .filter_map(|sample| sample.memory.rss_kib.map(|value| value as f64))
      .collect(),
  );
  let pss = numeric_summary(
    observations
      .iter()
      .filter(|sample| sample.memory.available)
      .filter_map(|sample| sample.memory.pss_kib.map(|value| value as f64))
      .collect(),
  );
  let resident = numeric_summary(
    observations
      .iter()
      .filter(|sample| sample.gpu.available)
      .filter_map(|sample| sample.gpu.resident_bytes.map(|value| value as f64))
      .collect(),
  );
  let mut elapsed = 0.0;
  let mut cpu_delta = 0.0;
  let mut context_delta = 0.0;
  let mut engine_delta = None;
  if let (Some(first), Some(last)) = (observations.first(), observations.last()) {
    elapsed = (last.elapsed_milliseconds - first.elapsed_milliseconds).max(0.0) / 1000.0;
    cpu_delta = last.cpu_time_milliseconds - first.cpu_time_milliseconds;
    context_delta = last.context_switches as f64 - first.context_switches as f64;
    if first.gpu.available {
      engine_delta = first
        .gpu
        .engine_nanoseconds
        .zip(last.gpu.engine_nanoseconds)
        .and_then(|(first, last)| last.checked_sub(first));
    }
  }
  json!({
    "memory": { "available": rss.is_some() || pss.is_some(), "rssKiB": rss, "pssKiB": pss },
    "cpu": {
      "timeMillisecondsDelta": cpu_delta,
      "timeMillisecondsPerSecond": if elapsed > 0.0 { cpu_delta / elapsed } else { 0.0 },
      "contextSwitchesPerSecond": if elapsed > 0.0 { context_delta / elapsed } else { 0.0 },
    },
    "gpu": {
      "residentBytes": resident,
      "engineNanosecondsPerSecond": engine_delta.filter(|_| elapsed > 0.0)
        .map(|delta| delta as f64 / elapsed),
    },
  })
}

pub fn run(args: Args) -> Result<()> {
  ensure!(
    cfg!(target_os = "linux"),
    "The process monitor requires Linux /proc."
  );
  ensure!(
    args.pid > 0 && args.samples > 0 && args.interval_ms > 0,
    "Monitor pid, samples and interval must be positive."
  );
  let observation_ms = (args.samples - 1)
    .checked_mul(args.interval_ms)
    .context("Monitor observation duration overflows milliseconds.")?;
  ensure!(
    !args.output.as_os_str().is_empty(),
    "Monitor requires --out."
  );
  let output_path = resolve_output_path(crate::root(), &args.output)?;
  let clock_output = process::capture(process::command("getconf").arg("CLK_TCK"))
    .context("Could not initialize monitor clocks")?;
  let hertz: f64 = String::from_utf8_lossy(&clock_output.stdout)
    .trim()
    .parse()
    .context("Could not read the clock tick frequency")?;
  ensure!(
    hertz.is_finite() && hertz > 0.0,
    "Invalid clock tick frequency."
  );
  let started_at = Instant::now();
  let wall_clock_started_at = util::now_iso();
  let mut samples = Vec::new();
  let first = observe_process(args.pid, started_at.elapsed().as_secs_f64() * 1000.0, hertz)?;
  let expected_start_time = first.start_time_ticks;
  samples.push(first);
  for index in 1..args.samples {
    let target = Duration::from_millis(index * args.interval_ms);
    if let Some(delay) = target.checked_sub(started_at.elapsed()) {
      process::sleep_interruptible(delay)?;
    }
    ensure!(!process::interrupted(), "Monitor sampling interrupted.");
    let sample = observe_process(args.pid, started_at.elapsed().as_secs_f64() * 1000.0, hertz)?;
    ensure_identity(expected_start_time, sample.start_time_ticks)?;
    samples.push(sample);
  }
  let report = json!({
    "schema": 1,
    "platform": "linux",
    "protocol": {
      "kind": "raw sampler",
      "stabilization": "external; the caller completes the selected five-minute state before starting this command",
      "observation": format!("samples t=0 through t={observation_ms} ms"),
    },
    "pid": args.pid,
    "label": args.label,
    "processStartTimeTicks": expected_start_time,
    "samples": samples.len(),
    "intervalMilliseconds": args.interval_ms,
    "startedAt": wall_clock_started_at,
    "first": samples.first(),
    "last": samples.last(),
    "summary": summarize_observations(&samples),
  });
  ensure!(!process::interrupted(), "Monitor sampling interrupted.");
  let encoded = serde_json::to_string(&report)?;
  append_report(crate::root(), &output_path, &encoded).context("Could not write monitor output")?;
  println!("{encoded}");
  Ok(())
}

fn ensure_identity(expected: u64, actual: u64) -> Result<()> {
  ensure!(
    expected == actual,
    "The target process identity changed from start time {expected} to {actual}."
  );
  Ok(())
}

fn observe_process(pid: u32, elapsed_ms: f64, hertz: f64) -> Result<ProcessObservation> {
  let snapshot = read_process(pid)?;
  let memory = read_memory(pid)?;
  let context_switches = read_context_switches(pid)?;
  let gpu = read_gpu(pid)?;
  // PID reuse can happen while reading the separate proc files, not just between samples.
  ensure_identity(
    snapshot.start_time_ticks,
    read_process(pid)?.start_time_ticks,
  )?;
  Ok(ProcessObservation {
    elapsed_milliseconds: elapsed_ms,
    name: snapshot.name,
    start_time_ticks: snapshot.start_time_ticks,
    memory,
    cpu_time_milliseconds: (snapshot.utime_ticks as f64 + snapshot.stime_ticks as f64) * 1000.0
      / hertz,
    context_switches,
    gpu,
  })
}

fn read_process(pid: u32) -> Result<ProcSnapshot> {
  let text = fs::read_to_string(format!("/proc/{pid}/stat"))
    .with_context(|| format!("Could not read /proc/{pid}"))?;
  parse_proc_stat(&text, pid)
}

fn parse_proc_stat(text: &str, pid: u32) -> Result<ProcSnapshot> {
  let open = text
    .find('(')
    .context("proc stat is missing process name")?;
  let close = text
    .rfind(')')
    .filter(|close| *close > open)
    .context("proc stat has an unexpected shape")?;
  ensure!(
    text[..open].trim().parse::<u32>()? == pid,
    "proc stat belongs to a different PID"
  );
  let fields: Vec<_> = text[close + 1..].split_whitespace().collect();
  let field = |index: usize| -> Result<u64> {
    fields
      .get(index)
      .context("proc stat is missing a field")?
      .parse::<u64>()
      .with_context(|| format!("/proc/{pid}/stat has an invalid field"))
  };
  Ok(ProcSnapshot {
    name: text[open + 1..close].to_owned(),
    utime_ticks: field(11)?,
    stime_ticks: field(12)?,
    start_time_ticks: field(19)?,
  })
}

fn status_counter(text: &str, name: &str) -> Result<u64> {
  let value = text
    .lines()
    .find_map(|line| {
      line
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(':'))
    })
    .with_context(|| format!("Status is missing {name}"))?;
  value
    .split_whitespace()
    .next()
    .context("Status counter has no value")?
    .parse()
    .with_context(|| format!("Status has invalid {name}"))
}

fn parse_memory(text: &str) -> Result<MemoryState> {
  let read = |name: &str| -> Result<u64> {
    let line = text
      .lines()
      .find_map(|line| {
        line
          .strip_prefix(name)
          .and_then(|rest| rest.strip_prefix(':'))
      })
      .with_context(|| format!("smaps_rollup is missing {name}"))?;
    let mut fields = line.split_whitespace();
    let value = fields.next().context("Missing memory value")?.parse()?;
    ensure!(
      fields.next() == Some("kB") && fields.next().is_none(),
      "Invalid smaps_rollup units"
    );
    Ok(value)
  };
  Ok(MemoryState {
    available: true,
    rss_kib: Some(read("Rss")?),
    pss_kib: Some(read("Pss")?),
    reason: None,
  })
}

fn read_memory(pid: u32) -> Result<MemoryState> {
  match fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) {
    Ok(text) => parse_memory(&text).context("Could not read process memory"),
    Err(error)
      if matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
      ) =>
    {
      Ok(MemoryState::unavailable(format!(
        "Could not read process memory: {error}"
      )))
    }
    Err(error) => Err(error).context("Could not read process memory"),
  }
}

fn read_context_switches(pid: u32) -> Result<u64> {
  let directory = PathBuf::from(format!("/proc/{pid}/task"));
  let mut total = 0_u64;
  for thread in fs::read_dir(&directory).context("Could not read process threads")? {
    let text = fs::read_to_string(thread?.path().join("status"))
      .context("Could not read process thread context switches")?;
    for name in ["voluntary_ctxt_switches", "nonvoluntary_ctxt_switches"] {
      total = total
        .checked_add(status_counter(&text, name)?)
        .context("Process context switch counter overflow")?;
    }
  }
  Ok(total)
}

fn parse_gpu<'a>(fdinfo: impl IntoIterator<Item = &'a str>) -> Result<GpuMetricState> {
  let mut seen_clients = BTreeSet::new();
  let mut resident = None::<u64>;
  let mut engine = None::<u64>;
  for text in fdinfo {
    if !text.contains("drm-driver:") {
      continue;
    }
    let field = |name: &str| {
      text
        .lines()
        .find_map(|line| line.strip_prefix(name).map(str::trim))
    };
    if let Some(client) = field("drm-client-id:") {
      if client.bytes().all(|byte| byte.is_ascii_digit())
        && !client.is_empty()
        && !seen_clients.insert((field("drm-pdev:").unwrap_or("unknown"), client))
      {
        continue;
      }
    }
    for line in text.lines() {
      let Some((key, value)) = line.split_once(':') else {
        continue;
      };
      let mut parts = value.split_whitespace();
      let Some(number) = parts.next().and_then(|part| part.parse::<u64>().ok()) else {
        continue;
      };
      let unit = parts.next();
      if parts.next().is_some() {
        continue;
      }
      let (target, multiplier) = if key.starts_with("drm-resident-") {
        let multiplier = match unit {
          None => 1,
          Some("KiB") => 1024,
          Some("MiB") => 1024 * 1024,
          _ => continue,
        };
        (&mut resident, multiplier)
      } else if key.starts_with("drm-engine-") && unit == Some("ns") {
        (&mut engine, 1)
      } else {
        continue;
      };
      let value = number
        .checked_mul(multiplier)
        .context("DRM metric overflow")?;
      *target = Some(
        target
          .unwrap_or(0)
          .checked_add(value)
          .context("DRM total overflow")?,
      );
    }
  }
  if resident.is_none() && engine.is_none() {
    return Ok(GpuMetricState::unavailable(
      "No attributable DRM resident or engine counters in process fdinfo.".into(),
    ));
  }
  Ok(GpuMetricState {
    available: true,
    resident_bytes: resident,
    engine_nanoseconds: engine,
    reason: None,
  })
}

fn read_gpu(pid: u32) -> Result<GpuMetricState> {
  let entries = match fs::read_dir(format!("/proc/{pid}/fdinfo")) {
    Ok(entries) => entries,
    Err(error) if error.kind() == io::ErrorKind::NotFound => {
      return Ok(GpuMetricState::unavailable(format!(
        "Could not read DRM fdinfo: {error}"
      )));
    }
    Err(error) => return Err(error).context("Could not read DRM fdinfo"),
  };
  let mut texts = Vec::new();
  for entry in entries {
    match fs::read_to_string(entry?.path()) {
      Ok(text) => texts.push(text),
      Err(error) if error.kind() == io::ErrorKind::NotFound => {}
      Err(error) => return Err(error).context("Could not read DRM fdinfo"),
    }
  }
  parse_gpu(texts.iter().map(String::as_str))
}

fn absolute_normalized(root: &Path, path: &Path) -> PathBuf {
  let mut result = PathBuf::new();
  for component in root.join(path).components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir => {
        result.pop();
      }
      other => result.push(other.as_os_str()),
    }
  }
  result
}

fn resolve_output_path(root: &Path, output: &Path) -> Result<PathBuf> {
  let lexical = absolute_normalized(root, output);
  let target = absolute_normalized(root, Path::new("target"));
  ensure!(
    lexical.starts_with(&target) && lexical != target,
    "Monitor output must stay under {}: {}",
    target.display(),
    output.display()
  );
  fs::create_dir_all(&target)?;
  let real_target = fs::canonicalize(&target)?;
  let mut nearest = lexical.as_path();
  loop {
    match fs::symlink_metadata(nearest) {
      Ok(_) => break,
      Err(error) if error.kind() == io::ErrorKind::NotFound => {
        nearest = nearest.parent().context("Output has no existing parent")?;
      }
      Err(error) => return Err(error).context("Monitor output is not confined"),
    }
  }
  let real_existing = fs::canonicalize(nearest).context("Monitor output is not confined")?;
  ensure!(
    real_existing.starts_with(&real_target),
    "Monitor output resolves outside {}",
    real_target.display()
  );
  match fs::symlink_metadata(&lexical) {
    Ok(metadata) => ensure!(
      metadata.is_file() && !metadata.file_type().is_symlink(),
      "Monitor output must be a regular file, not a symlink"
    ),
    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
    Err(error) => return Err(error).context("Could not inspect monitor output"),
  }
  Ok(lexical)
}

fn append_report(root: &Path, output: &Path, encoded: &str) -> Result<()> {
  let checked = resolve_output_path(root, output)?;
  let parent = checked.parent().context("Monitor output has no parent")?;
  fs::create_dir_all(parent)?;
  // Recheck after creating parents so symlinked ancestors cannot redirect a new output.
  let checked = resolve_output_path(root, &checked)?;
  #[cfg(target_os = "linux")]
  let parent_handle = fs::File::open(checked.parent().context("Monitor output has no parent")?)?;
  #[cfg(target_os = "linux")]
  let checked = {
    use std::os::fd::AsRawFd;
    // Open through a pinned directory instead of following the checked ancestors a second time.
    let pinned = PathBuf::from(format!("/proc/self/fd/{}", parent_handle.as_raw_fd()));
    ensure!(
      fs::canonicalize(&pinned)?.starts_with(fs::canonicalize(root.join("target"))?),
      "Monitor output parent resolves outside target"
    );
    pinned.join(
      checked
        .file_name()
        .context("Monitor output has no filename")?,
    )
  };
  let mut options = OpenOptions::new();
  options.create(true).append(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
    #[cfg(target_os = "linux")]
    options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
  }
  let mut file = options.open(checked)?;
  ensure!(
    file.metadata()?.is_file(),
    "Monitor output must be a regular file"
  );
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
  }
  writeln!(file, "{encoded}")?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn observation(
    elapsed: f64,
    cpu: f64,
    context: u64,
    rss: u64,
    pss: u64,
    resident: u64,
    engine: u64,
  ) -> ProcessObservation {
    ProcessObservation {
      elapsed_milliseconds: elapsed,
      name: "jellypilot".into(),
      start_time_ticks: 100,
      memory: MemoryState {
        available: true,
        rss_kib: Some(rss),
        pss_kib: Some(pss),
        reason: None,
      },
      cpu_time_milliseconds: cpu,
      context_switches: context,
      gpu: GpuMetricState {
        available: true,
        resident_bytes: Some(resident),
        engine_nanoseconds: Some(engine),
        reason: None,
      },
    }
  }

  #[test]
  fn summarizes_process_rates_and_nearest_rank_memory() {
    let samples = [
      observation(0.0, 10.0, 2, 100, 60, 1024, 100),
      observation(1000.0, 20.0, 4, 120, 70, 2048, 300),
    ];
    let summary = summarize_observations(&samples);
    assert_eq!(summary["memory"]["rssKiB"]["mean"], 110.0);
    assert_eq!(summary["memory"]["pssKiB"]["median"], 60.0);
    assert_eq!(summary["cpu"]["timeMillisecondsDelta"], 10.0);
    assert_eq!(summary["cpu"]["timeMillisecondsPerSecond"], 10.0);
    assert_eq!(summary["cpu"]["contextSwitchesPerSecond"], 2.0);
    assert_eq!(summary["gpu"]["residentBytes"]["max"], 2048.0);
    assert_eq!(summary["gpu"]["engineNanosecondsPerSecond"], 200.0);
  }

  #[test]
  fn unavailable_metrics_remain_null_with_diagnostic_reasons() {
    let mut sample = observation(0.0, 0.0, 0, 0, 0, 0, 0);
    sample.memory = MemoryState::unavailable("permission denied".into());
    sample.gpu = GpuMetricState::unavailable("no DRM counters".into());
    let encoded = serde_json::to_value(&sample).unwrap();
    assert_eq!(encoded["memory"]["reason"], "permission denied");
    assert_eq!(encoded["gpu"]["reason"], "no DRM counters");
    let summary = summarize_observations(&[sample]);
    assert_eq!(
      summary["memory"],
      json!({"available": false, "rssKiB": null, "pssKiB": null})
    );
    assert_eq!(
      summary["gpu"],
      json!({"residentBytes": null, "engineNanosecondsPerSecond": null})
    );
  }

  #[test]
  fn engine_counter_resets_do_not_produce_negative_rates() {
    let samples = [
      observation(0.0, 0.0, 0, 0, 0, 0, 200),
      observation(1000.0, 0.0, 0, 0, 0, 0, 100),
    ];
    assert!(summarize_observations(&samples)["gpu"]["engineNanosecondsPerSecond"].is_null());
  }

  #[test]
  fn percentiles_use_nearest_rank() {
    assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 0.5), 2.0);
    assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 0.95), 4.0);
    assert_eq!(percentile(&[], 0.95), 0.0);
  }

  #[test]
  fn proc_stat_handles_parentheses_and_spaces_in_process_names() {
    let mut fields = vec!["0"; 50];
    fields[11] = "123";
    fields[12] = "45";
    fields[19] = "6789";
    let parsed = parse_proc_stat(
      &format!("9999 (comm with ) spaces) {}", fields.join(" ")),
      9999,
    )
    .unwrap();
    assert_eq!(
      parsed,
      ProcSnapshot {
        name: "comm with ) spaces".into(),
        utime_ticks: 123,
        stime_ticks: 45,
        start_time_ticks: 6789
      }
    );
  }

  #[test]
  fn proc_stat_rejects_malformed_counters_and_wrong_pid() {
    for text in [
      "9999 no parentheses",
      "9999 (name) R 0",
      "8888 (name) R 0",
      "9999 (name) R",
    ] {
      assert!(parse_proc_stat(text, 9999).is_err());
    }
    assert!(ensure_identity(100, 101).is_err());
  }

  #[test]
  fn malformed_memory_is_not_silently_unavailable() {
    assert!(parse_memory("Rss: 123 kB\n").is_err());
    assert!(parse_memory("Rss: 123 bytes\nPss: 42 kB\n").is_err());
  }

  #[test]
  fn gpu_deduplicates_clients_per_device_and_converts_units() {
    let first = "drm-driver: amdgpu\ndrm-pdev: 0000:03:00.0\ndrm-client-id: 1\ndrm-resident-vram: 2 MiB\ndrm-resident-gtt: 1 KiB\ndrm-engine-gfx: 100 ns\n";
    let second = "drm-driver: amdgpu\ndrm-pdev: 0000:04:00.0\ndrm-client-id: 1\ndrm-resident-vram: 3\ndrm-engine-gfx: 200 ns\n";
    let gpu = parse_gpu([first, first, second]).unwrap();
    assert_eq!(gpu.resident_bytes, Some(2 * 1024 * 1024 + 1024 + 3));
    assert_eq!(gpu.engine_nanoseconds, Some(300));
    let absent = parse_gpu(["pos: 0\n"]).unwrap();
    assert!(!absent.available && absent.resident_bytes.is_none() && absent.reason.is_some());
  }

  #[test]
  fn output_rejects_lexical_escapes_and_non_files() {
    let root = tempfile::tempdir().unwrap();
    for path in ["outside.ndjson", "target/../outside.ndjson", "target"] {
      assert!(resolve_output_path(root.path(), Path::new(path)).is_err());
    }
    fs::create_dir_all(root.path().join("target/directory")).unwrap();
    assert!(resolve_output_path(root.path(), Path::new("target/directory")).is_err());
    assert!(resolve_output_path(root.path(), Path::new("target/new/report.ndjson")).is_ok());
  }

  #[cfg(unix)]
  #[test]
  fn output_rejects_symlink_escapes_and_dangling_symlinks() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("target")).unwrap();
    symlink(outside.path(), root.path().join("target/escape")).unwrap();
    symlink(
      root.path().join("target/missing"),
      root.path().join("target/dangling"),
    )
    .unwrap();
    fs::write(root.path().join("target/existing"), "").unwrap();
    symlink(
      root.path().join("target/existing"),
      root.path().join("target/link"),
    )
    .unwrap();
    for path in ["target/escape/new", "target/dangling", "target/link"] {
      assert!(resolve_output_path(root.path(), Path::new(path)).is_err());
    }
  }

  #[cfg(unix)]
  #[test]
  fn output_appends_private_ndjson() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("target/resources/run.ndjson");
    append_report(root.path(), &output, "{\"sample\":1}").unwrap();
    append_report(root.path(), &output, "{\"sample\":2}").unwrap();
    assert_eq!(
      fs::read_to_string(&output).unwrap(),
      "{\"sample\":1}\n{\"sample\":2}\n"
    );
    assert_eq!(
      fs::metadata(output).unwrap().permissions().mode() & 0o777,
      0o600
    );
  }

  #[test]
  fn numeric_arguments_reject_non_decimal_and_zero_values() {
    for value in [
      "",
      "0",
      "-1",
      "+1",
      "1.5",
      "1e3",
      " 2",
      "٢",
      "18446744073709551616",
    ] {
      assert!(parse_positive(value).is_err(), "accepted {value:?}");
    }
  }
}
