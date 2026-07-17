fn main() {
  println!("cargo:rerun-if-changed=capabilities");

  let capabilities = if std::env::var_os("CARGO_FEATURE_WEBDRIVER").is_some() {
    "./capabilities/**/*"
  } else {
    "./capabilities/default.json"
  };

  tauri_build::try_build(tauri_build::Attributes::new().capabilities_path_pattern(capabilities))
    .expect("failed to build the Tauri application metadata");
}
