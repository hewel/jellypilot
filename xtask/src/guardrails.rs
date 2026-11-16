use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::root;

const FORBIDDEN_CORE_DEPS: &[&str] = &[
  "iced",
  "iced_core",
  "iced_runtime",
  "iced_renderer",
  "iced_wgpu",
  "iced_mpv_preview",
  "jellypilot-ui",
  "jellypilot-mpv-host",
  "jellypilot-iced",
  "jellypilot-launcher",
  "gtk",
  "tray-icon",
  "wgpu",
  "wayland-client",
  "wayland-protocols",
  "x11rb",
  "zbus",
];

const DESKTOP_ONLY_PATHS: &[&str] = &[
  "crates/jellypilot-ui",
  "crates/jellypilot-mpv-host",
  "crates/jellypilot-launcher",
  "src-iced",
];

const UNSAFE_ALLOWLISTED_PACKAGES: &[&str] = &["jellypilot-mpv-host", "jellypilot-launcher"];

const BUILTIN_ICED_THEMES: &[&str] = &[
  "Theme::Light",
  "Theme::Dark",
  "Theme::Dracula",
  "Theme::Nord",
  "Theme::SolarizedLight",
  "Theme::SolarizedDark",
  "Theme::GruvboxLight",
  "Theme::GruvboxDark",
  "Theme::CatppuccinLatte",
  "Theme::CatppuccinFrappe",
  "Theme::CatppuccinMacchiato",
  "Theme::CatppuccinMocha",
  "Theme::TokyoNight",
  "Theme::TokyoNightStorm",
  "Theme::TokyoNightLight",
  "Theme::KanagawaWave",
  "Theme::KanagawaDragon",
  "Theme::KanagawaLotus",
  "Theme::Moonfly",
  "Theme::Nightfly",
  "Theme::Oxocarbon",
  "Theme::Ferra",
];

fn read_toml(path: &Path) -> Value {
  let content = fs::read_to_string(path)
    .unwrap_or_else(|error| panic!("Failed to read {}: {error}", path.display()));
  toml::from_str(&content)
    .unwrap_or_else(|error| panic!("Failed to parse {}: {error}", path.display()))
}

fn collect_dependency_names(table: &toml::map::Map<String, Value>, out: &mut BTreeSet<String>) {
  for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
    if let Some(deps) = table.get(section).and_then(Value::as_table) {
      out.extend(deps.keys().cloned());
    }
  }
  if let Some(targets) = table.get("target").and_then(Value::as_table) {
    for target in targets.values() {
      if let Some(target_table) = target.as_table() {
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
          if let Some(deps) = target_table.get(section).and_then(Value::as_table) {
            out.extend(deps.keys().cloned());
          }
        }
      }
    }
  }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
  let entries = fs::read_dir(dir)
    .unwrap_or_else(|error| panic!("Failed to read directory {}: {error}", dir.display()));
  for entry in entries {
    let path = entry
      .unwrap_or_else(|error| panic!("Failed to read entry in {}: {error}", dir.display()))
      .path();
    if path.is_dir() {
      collect_rs_files(&path, out);
    } else if path.extension().is_some_and(|ext| ext == "rs") {
      out.push(path);
    }
  }
}

fn has_prefix_with_boundary(line: &str, needle: &str) -> bool {
  let mut start = 0;
  while let Some(offset) = line[start..].find(needle) {
    let idx = start + offset;
    let end = idx + needle.len();
    let before_ok = idx == 0
      || !line[..idx]
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    let after_ok = end == line.len()
      || !line[end..]
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if before_ok && after_ok {
      return true;
    }
    start = end;
  }
  false
}

fn strip_code_from_markdown(input: &str) -> String {
  let mut out = String::with_capacity(input.len());
  let mut fence: Option<char> = None;
  for line in input.lines() {
    let trimmed = line.trim_start();
    if let Some(marker) = fence {
      if trimmed.starts_with(&marker.to_string().repeat(3)) {
        fence = None;
      }
      out.push('\n');
      continue;
    }
    if trimmed.starts_with("```") {
      fence = Some('`');
      out.push('\n');
      continue;
    }
    if trimmed.starts_with("~~~") {
      fence = Some('~');
      out.push('\n');
      continue;
    }
    let mut in_inline_code = false;
    for ch in line.chars() {
      if ch == '`' {
        in_inline_code = !in_inline_code;
      } else if !in_inline_code {
        out.push(ch);
      }
    }
    out.push('\n');
  }
  out
}

fn extract_markdown_targets(cleaned: &str) -> Vec<String> {
  let mut targets = Vec::new();
  let bytes = cleaned.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b']' && bytes.get(i + 1) == Some(&b'(') {
      let start = i + 2;
      let mut depth = 1usize;
      let mut j = start;
      while j < bytes.len() && depth > 0 {
        match bytes[j] {
          b'(' => depth += 1,
          b')' => depth -= 1,
          b'\n' => break,
          _ => {}
        }
        if depth > 0 {
          j += 1;
        }
      }
      if depth == 0 {
        let raw = cleaned[start..j].trim();
        let target = raw
          .strip_prefix('<')
          .and_then(|s| s.strip_suffix('>'))
          .unwrap_or_else(|| raw.split_whitespace().next().unwrap_or(""));
        if !target.is_empty() {
          targets.push(target.to_owned());
        }
        i = j + 1;
        continue;
      }
    }
    i += 1;
  }
  targets
}

fn find_ts_files(dir: &Path, out: &mut Vec<PathBuf>) {
  let Ok(entries) = fs::read_dir(dir) else {
    return;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {
      find_ts_files(&path, out);
    } else if path
      .extension()
      .is_some_and(|ext| ext == "ts" || ext == "tsx")
    {
      out.push(path);
    }
  }
}

#[test]
fn jellypilot_core_stays_display_and_desktop_free() {
  let manifest_path = root().join("crates/jellypilot-core/Cargo.toml");
  let parsed = read_toml(&manifest_path);
  let table = parsed.as_table().expect("Cargo.toml root must be a table");
  let mut deps = BTreeSet::new();
  collect_dependency_names(table, &mut deps);

  let forbidden: Vec<&str> = FORBIDDEN_CORE_DEPS
    .iter()
    .copied()
    .filter(|name| deps.contains(*name))
    .collect();
  assert!(
    forbidden.is_empty(),
    "crates/jellypilot-core must remain display-free (shared with Android via jellypilot-sdk/FFI), but depends on desktop/UI crate(s): {forbidden:?}. Move UI orchestration to src-iced or design-system code to crates/jellypilot-ui."
  );
}

#[test]
fn workspace_isolation_and_unsafe_boundaries_hold() {
  let root_toml = read_toml(&root().join("Cargo.toml"));
  let desktop_toml = read_toml(&root().join("src-iced/Cargo.toml"));

  let members: Vec<&str> = root_toml["workspace"]["members"]
    .as_array()
    .expect("root workspace.members array")
    .iter()
    .filter_map(Value::as_str)
    .collect();
  let excluded: Vec<&str> = root_toml["workspace"]["exclude"]
    .as_array()
    .expect("root workspace.exclude array")
    .iter()
    .filter_map(Value::as_str)
    .collect();

  for desktop_pkg in DESKTOP_ONLY_PATHS {
    assert!(
      !members.contains(desktop_pkg),
      "Root Cargo.toml [workspace.members] must exclude desktop package {desktop_pkg:?} so shared/Android builds never pull the pinned iced fork."
    );
    assert!(
      excluded.contains(desktop_pkg),
      "Root Cargo.toml [workspace.exclude] must list desktop package {desktop_pkg:?}."
    );
  }

  for (label, manifest) in [
    ("Cargo.toml", &root_toml),
    ("src-iced/Cargo.toml", &desktop_toml),
  ] {
    let unsafe_lint = manifest["workspace"]["lints"]["rust"]["unsafe_code"].as_str();
    assert_eq!(
      unsafe_lint,
      Some("forbid"),
      "{label} must set [workspace.lints.rust] unsafe_code = \"forbid\"."
    );
  }

  let mut manifests = vec![
    root().join("src-iced/Cargo.toml"),
    root().join("xtask/Cargo.toml"),
  ];
  for entry in fs::read_dir(root().join("crates")).expect("crates directory exists") {
    let path = entry.expect("crates entry").path();
    if path.is_dir()
      && path.file_name().and_then(|n| n.to_str()) != Some("media-server-api")
      && path.join("Cargo.toml").is_file()
    {
      manifests.push(path.join("Cargo.toml"));
    }
  }

  for manifest_path in manifests {
    let parsed = read_toml(&manifest_path);
    let Some(name) = parsed
      .get("package")
      .and_then(|pkg| pkg.get("name"))
      .and_then(Value::as_str)
    else {
      continue;
    };
    if UNSAFE_ALLOWLISTED_PACKAGES.contains(&name) {
      let policy = parsed
        .get("lints")
        .and_then(|l| l.get("rust"))
        .and_then(|r| r.get("unsafe_op_in_unsafe_fn"))
        .and_then(Value::as_str);
      assert_eq!(
        policy,
        Some("deny"),
        "Audited FFI/handoff crate {name:?} ({}) must set [lints.rust] unsafe_op_in_unsafe_fn = \"deny\".",
        manifest_path.display()
      );
    } else {
      let inherits = parsed
        .get("lints")
        .and_then(|l| l.get("workspace"))
        .and_then(Value::as_bool);
      assert_eq!(
        inherits,
        Some(true),
        "Maintained crate {name:?} ({}) must inherit `[lints] workspace = true` so unsafe_code = \"forbid\" and disallowed_methods apply.",
        manifest_path.display()
      );
    }
  }
}

#[test]
fn iced_views_do_not_use_builtin_iced_themes() {
  let mut files = Vec::new();
  for rel in ["src-iced/src/app/view", "src-iced/src/app/tv"] {
    collect_rs_files(&root().join(rel), &mut files);
  }
  assert!(!files.is_empty(), "Expected desktop view source files");

  let mut violations = Vec::new();
  for path in files {
    let content = fs::read_to_string(&path)
      .unwrap_or_else(|error| panic!("Failed to read {}: {error}", path.display()));
    let prod = content
      .split("\n#[cfg(test)]")
      .next()
      .unwrap_or(content.as_str());
    for (idx, line) in prod.lines().enumerate() {
      let trimmed = line.trim_start();
      if trimmed.starts_with("//") {
        continue;
      }
      for variant in BUILTIN_ICED_THEMES {
        if has_prefix_with_boundary(line, variant) {
          violations.push(format!("{}:{}: {variant}", path.display(), idx + 1));
        }
      }
    }
  }

  assert!(
    violations.is_empty(),
    "Built-in iced::Theme variants are forbidden in app views because jellypilot-ui resolves palettes by exact JellyPilot background color and unknown themes silently fall back to DARK_PALETTE. Use jellypilot_ui::theme::theme(ThemeMode) and state.palette() instead:\n{}",
    violations.join("\n")
  );
}

#[test]
fn agent_docs_links_and_retired_tooling_stay_clean() {
  let mut docs = vec![root().join("AGENTS.md"), root().join("GLOSSARY.md")];
  for entry in fs::read_dir(root().join("docs/agents")).expect("docs/agents directory") {
    let path = entry.expect("docs/agents entry").path();
    if path.extension().is_some_and(|ext| ext == "md") {
      docs.push(path);
    }
  }

  let mut broken_links = Vec::new();
  for doc in docs {
    let content = fs::read_to_string(&doc)
      .unwrap_or_else(|error| panic!("Failed to read {}: {error}", doc.display()));
    let cleaned = strip_code_from_markdown(&content);
    let base_dir = doc.parent().expect("markdown file has parent directory");
    for target in extract_markdown_targets(&cleaned) {
      if target.starts_with('#')
        || target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("file://")
        || target.starts_with("conversation://")
      {
        continue;
      }
      let path_part = target.split('#').next().unwrap_or("");
      if path_part.is_empty() {
        continue;
      }
      let resolved = base_dir.join(path_part);
      if !resolved.exists() {
        broken_links.push(format!(
          "{} -> {target:?} (resolved to {})",
          doc.display(),
          resolved.display()
        ));
      }
    }
  }
  assert!(
    broken_links.is_empty(),
    "Agent documentation contains broken relative links:\n{}",
    broken_links.join("\n")
  );

  let mut leftover_ts = Vec::new();
  let scripts_dir = root().join("scripts");
  if scripts_dir.exists() {
    find_ts_files(&scripts_dir, &mut leftover_ts);
  }
  assert!(
    leftover_ts.is_empty(),
    "Retired Bun/Effect-TS scripts/ tooling must not contain TypeScript files; use native Rust xtask instead: {leftover_ts:?}"
  );

  let rust_skill = root().join(".agents/skills/rust-best-practices/SKILL.md");
  if rust_skill.is_file() {
    let text = fs::read_to_string(&rust_skill)
      .unwrap_or_else(|error| panic!("Failed to read {}: {error}", rust_skill.display()));
    assert!(
      !text.contains("bun run task"),
      "{} still references retired `bun run task`; update it to `cargo xtask`.",
      rust_skill.display()
    );
  }
}
