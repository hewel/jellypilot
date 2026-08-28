use crate::config::IntroMode;

pub const SUBTITLE_LANGUAGE_OPTIONS: [&str; 8] =
    ["eng", "spa", "fra", "deu", "ita", "por", "jpn", "zho"];

#[must_use]
pub const fn config_intro_mode(selected: u32) -> IntroMode {
    match selected {
        1 => IntroMode::Manual,
        2 => IntroMode::Off,
        _ => IntroMode::Automatic,
    }
}

#[must_use]
pub const fn intro_mode_selection(mode: IntroMode) -> u32 {
    match mode {
        IntroMode::Automatic => 0,
        IntroMode::Manual => 1,
        IntroMode::Off => 2,
    }
}

#[must_use]
pub fn format_byte_count(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}
