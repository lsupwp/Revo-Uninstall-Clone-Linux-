use std::fmt;

/// Where an installed application comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Dnf,
    Flatpak,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Dnf => "DNF",
            Source::Flatpak => "Flatpak",
        }
    }

    pub fn color(self) -> egui::Color32 {
        match self {
            Source::Dnf => egui::Color32::from_rgb(0x4f, 0xa3, 0xff),
            Source::Flatpak => egui::Color32::from_rgb(0x59, 0xc9, 0x8f),
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_vscode_alias() {
        let pkg = Package::synthetic("vscode");
        assert_eq!(pkg.id, "code");
        assert_eq!(pkg.name, "Visual Studio Code");
        assert!(pkg.name_tokens().contains(&"code".to_string()));
    }

    #[test]
    fn synthetic_plain_name() {
        let pkg = Package::synthetic("gimp");
        assert_eq!(pkg.id, "gimp");
        assert!(pkg.name_tokens().contains(&"gimp".to_string()));
    }

    #[test]
    fn synthetic_flatpak_id() {
        let pkg = Package::synthetic("org.gnu.pspp");
        assert_eq!(pkg.source, Source::Flatpak);
        assert!(pkg.name_tokens().contains(&"pspp".to_string()));
    }
}

/// An installed application as reported by a package backend.
#[derive(Clone, Debug)]
pub struct Package {
    /// dnf package name or flatpak application ID.
    pub id: String,
    pub name: String,
    pub version: String,
    pub summary: String,
    /// Installed size in bytes (0 when unknown).
    pub size: u64,
    pub source: Source,
    /// Flatpak installation: "user", "system" or empty.
    pub installation: String,
    /// True when the package ships a .desktop file, i.e. it appears in the
    /// desktop application menu.
    pub gui: bool,
    /// Resolved icon file (PNG or SVG), when one was found.
    pub icon: Option<std::path::PathBuf>,
}

impl Package {
    /// Build a package description for an app that is no longer installed.
    /// Used to scan leftovers by name (e.g. after uninstalling outside
    /// RevoLinux).
    pub fn synthetic(query: &str) -> Package {
        let query = query.trim();
        let (id, name) = match query.to_lowercase().as_str() {
            "vscode" | "vs code" | "vsc" | "visual studio code" => {
                ("code".to_string(), "Visual Studio Code".to_string())
            }
            _ => (query.to_string(), query.to_string()),
        };
        // Flatpak application IDs are reverse-DNS names.
        let lower = id.to_lowercase();
        let is_flatpak = ["org.", "com.", "io.", "net.", "dev.", "app.", "page."]
            .iter()
            .any(|prefix| lower.starts_with(prefix));
        Package {
            id,
            name,
            version: String::new(),
            summary: String::new(),
            size: 0,
            source: if is_flatpak {
                Source::Flatpak
            } else {
                Source::Dnf
            },
            installation: if is_flatpak {
                "user".to_string()
            } else {
                "system".to_string()
            },
            gui: is_flatpak,
            icon: None,
        }
    }

    /// Stable UI key: source plus id.
    pub fn key(&self) -> String {
        format!("{}:{}", self.source.label(), self.id)
    }

    /// Tokens used to look for leftover files belonging to this package.
    pub fn name_tokens(&self) -> Vec<String> {
        fn push(out: &mut Vec<String>, s: &str) {
            let s = s.trim().to_lowercase();
            if s.len() >= 3 && !out.contains(&s) {
                out.push(s);
            }
        }

        let mut out: Vec<String> = Vec::new();
        push(&mut out, &self.id);
        push(&mut out, &self.name);

        // Reverse-DNS IDs: org.videolan.VLC -> also "vlc". RPM names with
        // dots get the same treatment, harmless for short segments.
        if let Some(last) = self.id.rsplit('.').next() {
            push(&mut out, last);
        }

        // RPM names often use a short config name: akonadi-server -> akonadi.
        if self.id.contains('-') {
            for segment in self.id.split('-') {
                push(&mut out, segment);
            }
        }

        // Names with spaces: "Visual Studio Code" -> "visual-studio-code".
        if self.name.contains(' ') {
            push(&mut out, &self.name.replace(' ', "-"));
            push(&mut out, &self.name.replace(' ', "_"));
            push(&mut out, &self.name.replace(' ', ""));
            // Last word: "Visual Studio Code" -> "code".
            let words: Vec<&str> = self.name.split_whitespace().collect();
            if words.len() > 1 {
                push(&mut out, words[words.len() - 1]);
            }
        }

        // RPM packaging suffixes: google-chrome-stable -> google-chrome.
        const SUFFIXES: &[&str] = &[
            "-stable",
            "-beta",
            "-unstable",
            "-nightly",
            "-devel",
            "-desktop",
            "-bin",
            "-app",
            "-git",
            "-x86_64",
            "-x86_64-64",
        ];
        for base in out.clone() {
            for suffix in SUFFIXES {
                if let Some(stripped) = base.strip_suffix(suffix) {
                    push(&mut out, stripped);
                }
            }
        }
        out
    }
}

/// How sure we are that a leftover path belongs to the uninstalled package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Confidence::High => "HIGH",
            Confidence::Medium => "MED",
            Confidence::Low => "LOW",
        }
    }

    pub fn color(self) -> egui::Color32 {
        match self {
            Confidence::High => egui::Color32::from_rgb(0xff, 0x9d, 0x3c),
            Confidence::Medium => egui::Color32::from_rgb(0xe8, 0xd0, 0x5a),
            Confidence::Low => egui::Color32::from_rgb(0x99, 0x99, 0x99),
        }
    }
}

/// A file or directory that probably belongs to a package which is gone.
#[derive(Clone, Debug)]
pub struct Leftover {
    pub path: std::path::PathBuf,
    pub size: u64,
    pub files: usize,
    pub is_dir: bool,
    pub confidence: Confidence,
    /// Human readable root label, e.g. "~/.config".
    pub root: &'static str,
    /// True when deleting needs root (polkit prompt).
    pub needs_root: bool,
    /// The token that matched.
    pub token: String,
}

impl Leftover {
    pub fn display_path(&self) -> String {
        let home = dirs::home_dir();
        match home {
            Some(h) if self.path.starts_with(&h) => {
                format!(
                    "~/{}",
                    self.path.strip_prefix(&h).unwrap_or(&self.path).display()
                )
            }
            _ => self.path.display().to_string(),
        }
    }
}
