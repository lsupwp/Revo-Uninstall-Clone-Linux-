use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Icon themes searched in order, from generic to desktop-specific.
const THEMES: &[&str] = &[
    "hicolor",
    "breeze",
    "breeze-dark",
    "Adwaita",
    "Papirus",
    "Papirus-Dark",
];

/// Size directories. 128 px scales well in the list and details header.
const SIZES: &[&str] = &[
    "128x128", "64x64", "256x256", "96x96", "48x48", "32x32", "scalable", "16x16",
];

fn roots() -> &'static [PathBuf] {
    static ROOTS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let mut v = Vec::new();
        if let Some(home) = dirs::home_dir() {
            v.push(home.join(".local/share/icons"));
            v.push(home.join(".local/share/flatpak/exports/share/icons"));
        }
        v.push(PathBuf::from("/usr/share/icons"));
        v.push(PathBuf::from("/usr/local/share/icons"));
        v.push(PathBuf::from("/var/lib/flatpak/exports/share/icons"));
        v
    })
}

fn pixmap_dirs() -> Vec<PathBuf> {
    let mut v = vec![
        PathBuf::from("/usr/share/pixmaps"),
        PathBuf::from("/usr/local/share/pixmaps"),
    ];
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".local/share/pixmaps"));
    }
    v
}

/// Read the Icon= value from a .desktop file (Desktop Entry section only).
fn desktop_icon_name(desktop: &Path) -> Option<String> {
    let text = std::fs::read_to_string(desktop).ok()?;
    let mut in_main = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if in_main {
            if let Some(value) = line.strip_prefix("Icon=") {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Read the display name from a .desktop file, preferring the current
/// locale's Name[xx] entry.
pub fn desktop_display_name(desktop: &Path) -> Option<String> {
    let text = std::fs::read_to_string(desktop).ok()?;
    let lang = std::env::var("LANG")
        .ok()
        .and_then(|l| l.split(['_', '.', '@']).next().map(str::to_string))
        .filter(|l| l.len() == 2);

    if let Some(lang) = &lang {
        let tag = format!("Name[{lang}]=");
        let mut in_main = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_main = line == "[Desktop Entry]";
                continue;
            }
            if !in_main {
                continue;
            }
            if let Some(value) = line.strip_prefix(&tag) {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    let mut in_main = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        if let Some(value) = line.strip_prefix("Name=") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Resolve the icon of a desktop entry, falling back to the desktop file
/// name and then to the package hint.
pub fn resolve_from_desktop(desktop: &Path, hint: &str) -> Option<PathBuf> {
    let mut names: Vec<String> = Vec::new();
    if let Some(name) = desktop_icon_name(desktop) {
        names.push(name);
    }
    if let Some(stem) = desktop.file_stem().and_then(|s| s.to_str()) {
        names.push(stem.to_string());
    }
    names.push(hint.to_string());

    for name in names {
        if let Some(path) = find(&name) {
            return Some(path);
        }
    }
    None
}

/// Find an installed icon file by icon name (without extension).
pub fn find(name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let direct = PathBuf::from(name);
    if direct.is_absolute() {
        return direct.is_file().then_some(direct);
    }

    // Some desktop files already include an extension; strip it and use the
    // same extension list so "foo.png" and "foo" behave the same.
    let stem = name
        .strip_suffix(".png")
        .or_else(|| name.strip_suffix(".svg"))
        .or_else(|| name.strip_suffix(".xpm"))
        .unwrap_or(name);
    let exts = ["png", "svg"];

    for root in roots() {
        for theme in THEMES {
            for size in SIZES {
                // Standard hicolor layout: <theme>/<size>/apps/icon.png
                for ext in exts {
                    let p = root
                        .join(theme)
                        .join(size)
                        .join("apps")
                        .join(format!("{stem}.{ext}"));
                    if p.is_file() {
                        return Some(p);
                    }
                }
                // Breeze/KDE layout: <theme>/apps/<size>/icon.svg
                for ext in exts {
                    let p = root
                        .join(theme)
                        .join("apps")
                        .join(size)
                        .join(format!("{stem}.{ext}"));
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
            // A few themes keep icons directly in apps/.
            for ext in exts {
                let p = root.join(theme).join("apps").join(format!("{stem}.{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    for dir in pixmap_dirs() {
        for ext in ["png", "svg", "xpm"] {
            let p = dir.join(format!("{stem}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Resolve the icon for a flatpak app from its exported desktop file.
pub fn resolve_flatpak(id: &str) -> Option<PathBuf> {
    let mut desktops = Vec::new();
    if let Some(home) = dirs::home_dir() {
        desktops.push(
            home.join(".local/share/flatpak/exports/share/applications")
                .join(format!("{id}.desktop")),
        );
    }
    desktops.push(
        PathBuf::from("/var/lib/flatpak/exports/share/applications").join(format!("{id}.desktop")),
    );
    for desktop in desktops {
        if desktop.is_file() {
            if let Some(path) = resolve_from_desktop(&desktop, id) {
                return Some(path);
            }
        }
    }
    find(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_icon_returns_none() {
        assert!(find("").is_none());
        assert!(find("this-icon-does-not-exist-revolinux").is_none());
    }

    #[test]
    fn finds_known_hicolor_icon() {
        // Not every test machine has this file; only check the search logic
        // when the file is present.
        let candidate = Path::new("/usr/share/icons/hicolor/64x64/apps");
        if candidate.is_dir() {
            if let Some(entry) = std::fs::read_dir(candidate)
                .ok()
                .and_then(|mut it| it.find_map(|e| e.ok()))
            {
                let stem = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap()
                    .to_string();
                assert!(find(&stem).is_some(), "icon {stem} not resolved");
            }
        }
    }
}
