use crate::exec::{self, Report};
use crate::model::{Package, Source};
use crate::util::{clean_summary, parse_human_size};

fn tool() -> String {
    exec::tool("flatpak")
}

/// List installed flatpak applications, user and system.
pub fn list() -> Result<Vec<Package>, String> {
    let flatpak = tool();
    let report = exec::run(
        &flatpak,
        &[
            "list",
            "--app",
            "--columns=application,name,version,size,origin,installation",
        ],
    )
    .map_err(|e| format!("cannot run {flatpak}: {e}"))?;
    if !report.ok {
        return Err(report.stderr);
    }
    let mut apps = parse_apps(&report.stdout);
    for app in &mut apps {
        app.icon = crate::icons::resolve_flatpak(&app.id);
    }
    Ok(apps)
}

fn parse_apps(stdout: &str) -> Vec<Package> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        let Some(id) = parts.first().map(|s| s.trim()) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let name = parts
            .get(1)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(id);
        let version = parts.get(2).map(|s| s.trim()).unwrap_or_default();
        let size = parts.get(3).map(|s| parse_human_size(s)).unwrap_or(0);
        let origin = parts.get(4).map(|s| s.trim()).unwrap_or_default();
        let installation = parts
            .get(5)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("system");

        let mut summary = clean_summary(&format!("{name} ({origin})"));
        if origin.is_empty() {
            summary = name.to_string();
        }

        out.push(Package {
            id: id.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            summary,
            size,
            source: Source::Flatpak,
            installation: installation.to_string(),
            gui: true,
            icon: None,
        });
    }
    out
}

/// Uninstall a flatpak app. `installation` decides --user vs --system.
pub fn uninstall(id: &str, installation: &str, delete_data: bool) -> std::io::Result<Report> {
    let flatpak = tool();
    let mut args: Vec<&str> = vec!["uninstall", "-y"];
    match installation {
        "user" => args.push("--user"),
        "system" => args.push("--system"),
        _ => {}
    }
    if delete_data {
        args.push("--delete-data");
    }
    args.push(id);
    exec::run(&flatpak, &args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flatpak_rows() {
        let text = "org.videolan.VLC\tVLC media player\t3.0.21\t412.5 MB\tflathub\tuser\n";
        let pkgs = parse_apps(text);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].id, "org.videolan.VLC");
        assert_eq!(pkgs[0].installation, "user");
        assert_eq!(pkgs[0].size, (412.5 * 1024.0 * 1024.0) as u64);
    }
}
