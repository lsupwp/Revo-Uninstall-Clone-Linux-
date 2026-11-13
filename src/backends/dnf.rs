use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::exec::{self, Report};
use crate::model::{Package, Source};
use crate::util::{clean_summary, strip_epoch};

const QUERY_FORMAT: &str = "%{name}\t%{evr}\t%{summary}\t%{installsize}\n";

/// List installed user-facing DNF packages.
///
/// Primary source is `dnf5 repoquery --userinstalled`, which hides the
/// hundreds of dependency packages. Falls back to plain RPM if dnf5 is
/// unavailable or the query fails.
pub fn list() -> Result<Vec<Package>, String> {
    let dnf = exec::tool("dnf5");
    let report = exec::run(
        &dnf,
        &["repoquery", "--userinstalled", "--qf", QUERY_FORMAT],
    )
    .map_err(|e| format!("cannot run {dnf}: {e}"))?;

    if report.ok && !report.stdout.trim().is_empty() {
        return Ok(mark_gui(parse_packages(&report.stdout)));
    }

    // Fallback: every installed RPM.
    let rpm = exec::tool("rpm");
    let report = exec::run(
        &rpm,
        &[
            "-qa",
            "--qf",
            "%{NAME}\t%{VERSION}-%{RELEASE}\t%{SUMMARY}\t%{SIZE}\n",
        ],
    )
    .map_err(|e| format!("cannot run {rpm}: {e}"))?;
    if !report.ok {
        return Err(report.stderr);
    }
    Ok(mark_gui(parse_packages(&report.stdout)))
}

/// Flag packages that install a .desktop file (visible in the app menu) and
/// resolve their icons.
fn mark_gui(mut packages: Vec<Package>) -> Vec<Package> {
    let entries = desktop_entries();
    for p in &mut packages {
        if let Some(desktop) = entries.get(&p.id) {
            p.gui = true;
            if let Some(name) = crate::icons::desktop_display_name(desktop) {
                p.name = name;
            }
            p.icon = crate::icons::resolve_from_desktop(desktop, &p.id);
        }
    }
    packages
}

fn desktop_entries() -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    let rpm = exec::tool("rpm");
    let Ok(report) = exec::run(&rpm, &["-qa", "--filesbypkg"]) else {
        return out;
    };
    if !report.ok {
        return out;
    }
    for line in report.stdout.lines() {
        let Some((name, path)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let path = path.trim();
        // Menu entries live in applications/; ignore servicemenus, autostart,
        // krunner plugins and other .desktop lookalikes.
        if !path.starts_with('/') || !path.contains("/share/applications/") {
            continue;
        }
        if !path.ends_with(".desktop") || !desktop_entry_visible(path) {
            continue;
        }
        // Keep the first visible entry per package.
        out.entry(name.to_string())
            .or_insert_with(|| PathBuf::from(path));
    }
    out
}

/// Skip hidden helpers that are installed but do not appear in the menu.
fn desktop_entry_visible(path: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return true;
    };
    !text.lines().any(|line| {
        let line = line.trim();
        line == "NoDisplay=true" || line == "Hidden=true"
    })
}

fn parse_packages(stdout: &str) -> Vec<Package> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.splitn(4, '\t');
        let Some(name) = parts.next() else { continue };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        // Skip gpg-pubkey pseudo packages.
        if name.starts_with("gpg-pubkey") {
            continue;
        }
        let version = parts.next().unwrap_or_default().trim();
        let summary = parts.next().unwrap_or_default();
        let size = parts
            .next()
            .unwrap_or("0")
            .trim()
            .parse::<u64>()
            .unwrap_or(0);
        out.push(Package {
            id: name.to_string(),
            name: name.to_string(),
            version: strip_epoch(version).to_string(),
            summary: clean_summary(summary),
            size,
            source: Source::Dnf,
            installation: "system".to_string(),
            gui: false,
            icon: None,
        });
    }
    out
}

/// Remove an RPM with DNF, asking for privileges through polkit.
pub fn remove(id: &str) -> std::io::Result<Report> {
    let dnf = exec::tool("dnf5");
    let args = ["remove", "-y", "--", id];

    if exec::is_root() {
        return exec::run(&dnf, &args);
    }

    let pkexec = exec::tool("pkexec");
    if std::path::Path::new(&pkexec).exists() {
        let mut full = vec![dnf.as_str()];
        full.extend_from_slice(&args);
        return exec::run(&pkexec, &full);
    }

    // Last resort: run unprivileged and let DNF report the failure.
    exec::run(&dnf, &args)
}

/// Files owned by an RPM package. Used to avoid reporting package files as
/// "leftovers" of the uninstall.
pub fn files(id: &str) -> Vec<PathBuf> {
    let rpm = exec::tool("rpm");
    match exec::run(&rpm, &["-ql", id]) {
        Ok(report) if report.ok => report
            .stdout
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with('/'))
            .map(PathBuf::from)
            .collect(),
        _ => Vec::new(),
    }
}

/// All files owned by any installed RPM. Used to avoid flagging files that
/// still belong to another installed package.
pub fn all_owned_files() -> &'static HashSet<PathBuf> {
    static OWNED: OnceLock<HashSet<PathBuf>> = OnceLock::new();
    OWNED.get_or_init(|| {
        let rpm = exec::tool("rpm");
        let report = exec::run(&rpm, &["-qa", "--filesbypkg"]);
        match report {
            Ok(r) if r.ok => r
                .stdout
                .lines()
                .filter_map(|line| {
                    line.split_once(char::is_whitespace)
                        .map(|(_, path)| PathBuf::from(path.trim()))
                })
                .filter(|p| p.is_absolute())
                .collect(),
            _ => HashSet::new(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_query_output() {
        let text = "vlc\t1:3.0.21-1.fc45\tVLC media player\t123456\n";
        let pkgs = parse_packages(text);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].id, "vlc");
        assert_eq!(pkgs[0].version, "3.0.21-1.fc45");
        assert_eq!(pkgs[0].size, 123456);
        assert_eq!(pkgs[0].source, Source::Dnf);
    }
}
