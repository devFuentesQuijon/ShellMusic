use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
struct Candidate {
    name: &'static str,
    binaries: &'static [&'static str],
}

const CANDIDATES: &[Candidate] = &[
    Candidate {
        name: "firefox",
        binaries: &["firefox"],
    },
    Candidate {
        name: "chrome",
        binaries: &["google-chrome", "chrome"],
    },
    Candidate {
        name: "chromium",
        binaries: &["chromium", "chromium-browser"],
    },
    Candidate {
        name: "brave",
        binaries: &["brave", "brave-browser"],
    },
    Candidate {
        name: "edge",
        binaries: &["microsoft-edge", "msedge"],
    },
    Candidate {
        name: "opera",
        binaries: &["opera"],
    },
    Candidate {
        name: "vivaldi",
        binaries: &["vivaldi"],
    },
    #[cfg(target_os = "macos")]
    Candidate {
        name: "safari",
        binaries: &[],
    },
];

/// Navegadores instalados que yt-dlp soporta en `--cookies-from-browser`,
/// ordenados con firefox primero (el más fiable para yt-dlp). Un navegador
/// cuenta como instalado si existe su directorio de perfil o su binario
/// está en el PATH.
pub fn detect_installed() -> Vec<&'static str> {
    CANDIDATES
        .iter()
        .filter(|c| has_profile(c.name) || c.binaries.iter().any(|b| binary_in_path(b)))
        .map(|c| c.name)
        .collect()
}

fn has_profile(name: &str) -> bool {
    profile_dirs(name).iter().any(|dir| dir.is_dir())
}

fn binary_in_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidate = dir.join(name);
        candidate.is_file() || dir.join(format!("{name}.exe")).is_file()
    })
}

#[cfg(target_os = "linux")]
fn profile_dirs(name: &str) -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let rel: &[&str] = match name {
        "firefox" => &[".mozilla/firefox"],
        "chrome" => &[".config/google-chrome"],
        "chromium" => &[".config/chromium"],
        "brave" => &[".config/BraveSoftware/Brave-Browser"],
        "edge" => &[".config/microsoft-edge"],
        "opera" => &[".config/opera"],
        "vivaldi" => &[".config/vivaldi"],
        _ => &[],
    };
    rel.iter().map(|r| home.join(r)).collect()
}

#[cfg(target_os = "macos")]
fn profile_dirs(name: &str) -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let rel: &[&str] = match name {
        "firefox" => &["Library/Application Support/Firefox"],
        "chrome" => &["Library/Application Support/Google/Chrome"],
        "chromium" => &["Library/Application Support/Chromium"],
        "brave" => &["Library/Application Support/BraveSoftware/Brave-Browser"],
        "edge" => &["Library/Application Support/Microsoft Edge"],
        "opera" => &["Library/Application Support/com.operasoftware.Opera"],
        "vivaldi" => &["Library/Application Support/Vivaldi"],
        "safari" => &["Library/Safari", "Library/Cookies"],
        _ => &[],
    };
    rel.iter().map(|r| home.join(r)).collect()
}

#[cfg(target_os = "windows")]
fn profile_dirs(name: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    match name {
        "firefox" => {
            if let Some(base) = roaming {
                dirs.push(base.join("Mozilla").join("Firefox"));
            }
        }
        "chrome" | "chromium" | "brave" | "edge" | "vivaldi" => {
            if let Some(base) = local {
                let rel: &[&str] = match name {
                    "chrome" => &["Google", "Chrome", "User Data"],
                    "chromium" => &["Chromium", "User Data"],
                    "brave" => &["BraveSoftware", "Brave-Browser", "User Data"],
                    "edge" => &["Microsoft", "Edge", "User Data"],
                    _ => &["Vivaldi", "User Data"],
                };
                dirs.push(rel.iter().fold(base, |path, part| path.join(part)));
            }
        }
        "opera" => {
            if let Some(base) = roaming {
                dirs.push(base.join("Opera Software").join("Opera Stable"));
            }
        }
        _ => {}
    }
    dirs
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn profile_dirs(_name: &str) -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firefox_is_first_when_detected() {
        let detected = detect_installed();
        if detected.contains(&"firefox") {
            assert_eq!(detected[0], "firefox");
        }
    }
}
