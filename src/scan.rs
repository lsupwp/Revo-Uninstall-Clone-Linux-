use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use walkdir::WalkDir;

pub use crate::model::Leftover;

use crate::model::{Confidence, Package};

struct Root {
    label: &'static str,
    path: PathBuf,
    /// Maximum walk depth relative to the root (1 = direct children).
    depth: usize,
}

fn roots(home: &Path) -> Vec<Root> {
    let mut r = Vec::new();
    let mut add = |label: &'static str, rel: &str, depth: usize| {
        let path = if let Some(stripped) = rel.strip_prefix("~/") {
            home.join(stripped)
        } else if rel == "~" {
            home.to_path_buf()
        } else {
            PathBuf::from(rel)
        };
        r.push(Root { label, path, depth });
    };

    add("~/.config", "~/.config", 1);
    add("~/.local/share", "~/.local/share", 1);
    add("~/.local/state", "~/.local/state", 1);
    add("~/.cache", "~/.cache", 1);
    add("~/.local/bin", "~/.local/bin", 1);
    add("~/.var/app", "~/.var/app", 1);
    add(
        "~/.local/share/applications",
        "~/.local/share/applications",
        1,
    );
    add("~/.config/autostart", "~/.config/autostart", 1);
    add("~/.local/share/icons", "~/.local/share/icons", 3);

    add("/tmp", "/tmp", 2);
    add("/var/tmp", "/var/tmp", 2);
    add("/var/cache", "/var/cache", 1);
    add("/var/log", "/var/log", 1);
    add("/var/lib", "/var/lib", 1);
    add("/etc", "/etc", 1);
    add("/etc/xdg/autostart", "/etc/xdg/autostart", 1);
    add("/etc/systemd/system", "/etc/systemd/system", 1);
    add("/opt", "/opt", 1);
    add("/usr/local", "/usr/local", 2);
    add("/usr/share/applications", "/usr/share/applications", 1);
    add("/usr/share/pixmaps", "/usr/share/pixmaps", 1);
    add("/usr/share/doc", "/usr/share/doc", 1);
    add("/usr/share/icons/hicolor", "/usr/share/icons/hicolor", 3);
    add("/usr/lib/systemd/system", "/usr/lib/systemd/system", 1);

    r
}

/// Tokens that are too generic to be used for leftover detection.
const STOP_TOKENS: &[&str] = &[
    "app", "apps", "bin", "data", "share", "config", "cache", "lib", "lib64", "local", "system",
    "user", "desktop", "common", "core", "test", "tests", "tools", "default", "support", "client",
    "server", "linux", "fedora", "gnome", "kde", "plasma", "icons", "fonts", "doc", "docs",
    "media", "player", "manager", "service", "helper", "daemon", "runtime", "stable", "beta",
    "nightly", "devel", "dev", "common", "utils", "util", "agent", "virtual", "guest",
];

fn usable_tokens(pkg: &Package) -> Vec<String> {
    pkg.name_tokens()
        .into_iter()
        .filter(|t| t.len() >= 3 && !STOP_TOKENS.contains(&t.as_str()))
        .collect()
}

/// Guess how strongly a file name matches the package tokens.
/// Returns the best match found.
pub fn match_confidence(file_name: &str, tokens: &[String]) -> Option<(Confidence, String)> {
    fn consider(best: &mut Option<(Confidence, String)>, conf: Confidence, token: &str) {
        let better = match best {
            None => true,
            Some((c, _)) => conf > *c,
        };
        if better {
            *best = Some((conf, token.to_string()));
        }
    }

    let lower = file_name.to_lowercase();
    let trimmed = lower.trim_start_matches('.');
    let stem = lower.strip_suffix(".desktop").unwrap_or(&lower);
    let stem_trimmed = stem.trim_start_matches('.');

    let mut best: Option<(Confidence, String)> = None;

    for token in tokens {
        // Exact name.
        if trimmed == token || stem_trimmed == token {
            consider(&mut best, Confidence::High, token);
            continue;
        }

        // Prefix followed by a separator, a digit, or "rc" (vim -> vimrc).
        for base in [trimmed, stem_trimmed] {
            if let Some(rest) = base.strip_prefix(token.as_str()) {
                let sep = rest.starts_with(['-', '_', '.', ' ', '@', '+'])
                    || rest
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false)
                    || rest.starts_with("rc");
                if sep && !rest.is_empty() {
                    let conf = if token.len() >= 4 || rest.starts_with("rc") {
                        Confidence::High
                    } else {
                        Confidence::Medium
                    };
                    consider(&mut best, conf, token);
                }
            }
        }
    }

    if best.is_some() {
        return best;
    }

    // Weak substring match, only for reasonably long tokens.
    for token in tokens {
        if token.len() >= 5 && trimmed.contains(token.as_str()) {
            consider(&mut best, Confidence::Low, token);
        }
    }
    best
}

fn measure(path: &Path) -> (u64, usize) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return (0, 0);
    };
    if !meta.is_dir() {
        return (meta.len(), 1);
    }
    let mut size = 0u64;
    let mut files = 0usize;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .take(500_000)
    {
        let Ok(entry) = entry else { continue };
        if let Ok(m) = entry.metadata() {
            if m.is_file() {
                size = size.saturating_add(m.len());
                files += 1;
            }
        }
    }
    (size, files)
}

fn needs_root(path: &Path, home: &Path) -> bool {
    let s = path.to_string_lossy();
    if path.starts_with(home) {
        return false;
    }
    !(s.starts_with("/tmp/") || s.starts_with("/var/tmp/"))
}

/// Scan standard locations for files left behind by `pkg`.
pub fn scan_package(pkg: &Package) -> Vec<Leftover> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home"));
    scan_package_in(pkg, &home)
}

pub fn scan_package_in(pkg: &Package, home: &Path) -> Vec<Leftover> {
    let tokens = usable_tokens(pkg);
    if tokens.is_empty() {
        return Vec::new();
    }
    let owned: HashSet<PathBuf> = crate::backends::owned_paths(pkg).into_iter().collect();
    let mut items: Vec<Leftover> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for root in roots(home) {
        if !root.path.is_dir() {
            continue;
        }
        let skip: RefCell<HashSet<PathBuf>> = RefCell::new(HashSet::new());
        let walker = WalkDir::new(&root.path)
            .max_depth(root.depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !skip.borrow().contains(e.path()));

        for entry in walker {
            let Ok(entry) = entry else { continue };
            if entry.depth() == 0 {
                continue;
            }
            let path = entry.path();
            if owned.contains(path) || seen.contains(path) {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some((confidence, token)) = match_confidence(file_name, &tokens) else {
                continue;
            };
            let Ok(meta) = std::fs::symlink_metadata(path) else {
                continue;
            };
            let (size, files) = measure(path);
            seen.insert(path.to_path_buf());
            items.push(Leftover {
                path: path.to_path_buf(),
                size,
                files,
                is_dir: meta.is_dir(),
                confidence,
                root: root.label,
                needs_root: needs_root(path, home),
                token,
            });
            if meta.is_dir() {
                skip.borrow_mut().insert(path.to_path_buf());
            }
        }
    }

    items.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then_with(|| b.size.cmp(&a.size))
            .then_with(|| a.path.cmp(&b.path))
    });

    // Files under /usr, /etc, /opt, ... may belong to another installed RPM.
    // Never offer those as leftovers.
    if items.iter().any(|i| i.needs_root) {
        let owned_all = crate::backends::dnf::all_owned_files();
        items.retain(|i| !(i.needs_root && owned_all.contains(&i.path)));
    }

    items
}

/// A path may only be deleted when it lives below a known scan root and is
/// not the root itself.
pub fn is_allowed_to_delete(path: &Path, home: &Path) -> bool {
    if path == Path::new("/") || path == home || path.components().count() < 3 {
        return false;
    }
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    roots(home)
        .iter()
        .any(|root| path.starts_with(&root.path) && path != root.path)
}

pub struct DeleteReport {
    pub deleted: usize,
    pub failures: Vec<String>,
    pub root_output: String,
}

/// Delete selected leftovers. Root-owned paths are removed in one pkexec call.
pub fn delete_items(items: &[Leftover]) -> DeleteReport {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home"));
    let mut report = DeleteReport {
        deleted: 0,
        failures: Vec::new(),
        root_output: String::new(),
    };
    let mut root_paths: Vec<PathBuf> = Vec::new();

    for item in items {
        if !is_allowed_to_delete(&item.path, &home) {
            report
                .failures
                .push(format!("refused unsafe path: {}", item.path.display()));
            continue;
        }
        if item.needs_root && !crate::exec::is_root() {
            root_paths.push(item.path.clone());
            continue;
        }
        if remove_path(&item.path, &mut report.failures) {
            report.deleted += 1;
        }
    }

    if !root_paths.is_empty() {
        let mut args: Vec<String> = vec!["rm".to_string(), "-rf".to_string(), "--".to_string()];
        args.extend(root_paths.iter().map(|p| p.display().to_string()));

        let result = if crate::exec::is_root() {
            crate::exec::run_strings("rm", &args[1..])
        } else {
            let pkexec = crate::exec::tool("pkexec");
            let mut full = vec![crate::exec::tool("rm")];
            full.extend(args[1..].iter().cloned());
            crate::exec::run_strings(&pkexec, &full)
        };

        match result {
            Ok(r) => {
                report.root_output = r.text();
                if r.ok {
                    report.deleted += root_paths.len();
                } else {
                    report
                        .failures
                        .push(format!("privileged delete failed: {}", r.text()));
                }
            }
            Err(e) => report.failures.push(format!("pkexec failed: {e}")),
        }
    }

    report
}

fn remove_path(path: &Path, failures: &mut Vec<String>) -> bool {
    let result = match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) => Err(e),
    };
    match result {
        Ok(()) => true,
        Err(e) => {
            failures.push(format!("{}: {e}", path.display()));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    fn tokens(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn confidence_rules() {
        let t = tokens(&["google-chrome", "chrome"]);
        assert_eq!(
            match_confidence("google-chrome", &t).unwrap().0,
            Confidence::High
        );
        assert_eq!(
            match_confidence(".google-chrome", &t).unwrap().0,
            Confidence::High
        );
        assert_eq!(
            match_confidence("google-chrome-beta.desktop", &t)
                .unwrap()
                .0,
            Confidence::High
        );
        assert_eq!(
            match_confidence("my-chrome-wrapper", &t).unwrap().0,
            Confidence::Low
        );
        assert!(match_confidence("unrelated", &t).is_none());
    }

    #[test]
    fn vimrc_is_high() {
        let t = tokens(&["vim"]);
        assert_eq!(match_confidence(".vimrc", &t).unwrap().0, Confidence::High);
        assert_eq!(match_confidence("vim", &t).unwrap().0, Confidence::High);
    }

    #[test]
    fn scan_finds_config_and_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        std::fs::create_dir_all(home.join(".config/foo")).unwrap();
        std::fs::write(home.join(".config/foo/settings.ini"), b"x").unwrap();
        std::fs::create_dir_all(home.join(".cache/foo")).unwrap();
        std::fs::create_dir_all(home.join(".config/unrelated")).unwrap();

        let pkg = Package {
            id: "org.example.Foo".into(),
            name: "Foo".into(),
            version: "1.0".into(),
            summary: String::new(),
            size: 0,
            source: Source::Flatpak,
            installation: "user".into(),
            gui: true,
            icon: None,
        };
        let items = scan_package_in(&pkg, home);
        let paths: Vec<String> = items.iter().map(|i| i.display_path()).collect();
        assert!(paths.iter().any(|p| p.contains(".config/foo")));
        assert!(paths.iter().any(|p| p.contains(".cache/foo")));
        assert!(!paths.iter().any(|p| p.contains("unrelated")));
    }

    #[test]
    fn deletion_guard() {
        let home = Path::new("/home/tester");
        assert!(!is_allowed_to_delete(Path::new("/"), home));
        assert!(!is_allowed_to_delete(home, home));
        assert!(!is_allowed_to_delete(Path::new("/etc"), home));
        assert!(is_allowed_to_delete(Path::new("/etc/foo"), home));
        assert!(is_allowed_to_delete(
            Path::new("/home/tester/.config/foo"),
            home
        ));
        assert!(!is_allowed_to_delete(
            Path::new("/home/tester/../etc/foo"),
            home
        ));
    }

    #[test]
    fn deletes_user_leftovers() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("foo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.conf"), b"x").unwrap();
        let file = tmp.path().join("foo.desktop");
        std::fs::write(&file, b"x").unwrap();

        let item = |path: PathBuf, is_dir: bool| Leftover {
            path,
            size: 1,
            files: 1,
            is_dir,
            confidence: Confidence::High,
            root: "/tmp",
            needs_root: false,
            token: "foo".into(),
        };
        let report = delete_items(&[item(dir.clone(), true), item(file.clone(), false)]);
        assert_eq!(report.deleted, 2);
        assert!(report.failures.is_empty());
        assert!(!dir.exists());
        assert!(!file.exists());
    }
}
