use crate::backends;
use crate::model::{Confidence, Package};
use crate::scan;
use crate::util::fmt_size;

const HELP: &str = "\
RevoLinux - clean uninstaller for Fedora (dnf + flatpak)

Usage:
  revolinux                     Launch the GUI
  revolinux list [--json]       List installed applications
  revolinux scan <id> [--json]  Scan leftover files for an application
  revolinux purge <id> [--dry-run] [--yes]
                                Preview leftovers (read-only) by default.
                                With --yes: uninstall, then delete high and
                                medium confidence leftovers.

<id> is a dnf package name, a flatpak application ID, or an exact app name.

Options:
  -h, --help      Show this help
  -V, --version   Show the version
";

/// Returns Some(exit_code) for CLI commands, None when the GUI should start.
pub fn dispatch(args: &[String]) -> Option<i32> {
    let cmd = args.first().map(String::as_str)?;
    let result = match cmd {
        "-h" | "--help" | "help" => {
            print!("{HELP}");
            Ok(())
        }
        "-V" | "--version" | "version" => {
            println!("revolinux {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "list" | "ls" => list(args.contains(&"--json".to_string())),
        "scan" => {
            let id = args.get(1);
            match id {
                Some(id) => scan_one(id, args.contains(&"--json".to_string())),
                None => Err("usage: revolinux scan <id>".to_string()),
            }
        }
        "purge" | "uninstall" => {
            let id = args.get(1);
            match id {
                Some(id) => purge(
                    id,
                    args.contains(&"--yes".to_string()),
                    args.contains(&"--dry-run".to_string()),
                ),
                None => Err("usage: revolinux purge <id> [--dry-run] [--yes]".to_string()),
            }
        }
        other => {
            eprintln!("unknown command: {other}\n\n{HELP}");
            return Some(2);
        }
    };
    match result {
        Ok(()) => Some(0),
        Err(e) => {
            eprintln!("error: {e}");
            Some(1)
        }
    }
}

fn find_package(id: &str) -> Result<Package, String> {
    let (packages, errors) = backends::list_all();
    if packages.is_empty() {
        return Err(format!(
            "no packages found{}",
            if errors.is_empty() {
                String::new()
            } else {
                format!(" ({})", errors.join("; "))
            }
        ));
    }
    packages
        .into_iter()
        .find(|p| {
            p.id == id
                || p.key() == id
                || p.name.eq_ignore_ascii_case(id)
                || p.id.to_lowercase() == id.to_lowercase()
        })
        .ok_or_else(|| format!("application not found: {id}"))
}

fn list(json: bool) -> Result<(), String> {
    let (packages, errors) = backends::list_all();
    for e in &errors {
        eprintln!("warning: {e}");
    }
    if json {
        let value = serde_json::json!(packages
            .iter()
            .map(|p| serde_json::json!({
                "id": p.id,
                "name": p.name,
                "version": p.version,
                "summary": p.summary,
                "size": p.size,
                "source": p.source.label().to_lowercase(),
                "installation": p.installation,
                "gui": p.gui,
                "icon": p.icon.as_ref().map(|i| i.display().to_string()),
            }))
            .collect::<Vec<_>>());
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return Ok(());
    }
    let total: u64 = packages.iter().map(|p| p.size).sum();
    for p in &packages {
        println!(
            "{:<8} {:<40} {:<22} {:>10}  {}",
            p.source.label(),
            truncate(&p.name, 40),
            truncate(&p.version, 22),
            fmt_size(p.size),
            p.id
        );
    }
    println!(
        "\n{} applications, {} installed",
        packages.len(),
        fmt_size(total)
    );
    Ok(())
}

fn scan_one(id: &str, json: bool) -> Result<(), String> {
    let pkg = match find_package(id) {
        Ok(pkg) => pkg,
        Err(_) => {
            eprintln!("not installed; scanning leftovers by name");
            Package::synthetic(id)
        }
    };
    println!(
        "Scanning leftovers for {} ({})...\n",
        pkg.name,
        pkg.source.label()
    );
    let items = scan::scan_package(&pkg);
    if json {
        let value = serde_json::json!(items
            .iter()
            .map(|i| serde_json::json!({
                "path": i.path,
                "size": i.size,
                "files": i.files,
                "is_dir": i.is_dir,
                "confidence": i.confidence.label(),
                "root": i.root,
                "needs_root": i.needs_root,
                "token": i.token,
            }))
            .collect::<Vec<_>>());
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return Ok(());
    }
    print_items(&items);
    Ok(())
}

fn print_items(items: &[scan::Leftover]) {
    if items.is_empty() {
        println!("No leftover files found.");
        return;
    }
    let total: u64 = items.iter().map(|i| i.size).sum();
    for i in items {
        println!(
            "{:<5} {:>10}  {}{}",
            i.confidence.label(),
            fmt_size(i.size),
            i.display_path(),
            if i.needs_root { "  [root]" } else { "" }
        );
    }
    println!("\n{} items, {}", items.len(), fmt_size(total));
}

fn purge(id: &str, yes: bool, dry_run: bool) -> Result<(), String> {
    let pkg = find_package(id)?;

    // Without --yes this is a read-only preview: scan and print the plan.
    if dry_run || !yes {
        println!(
            "Preview for {} ({}, {}). Nothing will be removed.",
            pkg.name,
            pkg.source.label(),
            pkg.id
        );
        println!("Uninstalling first is required to find true leftovers.\n");
        let items = scan::scan_package(&pkg);
        print_items(&items);
        let candidates: Vec<scan::Leftover> = items
            .iter()
            .filter(|i| i.confidence >= Confidence::Medium)
            .cloned()
            .collect();
        print_plan(&candidates);
        println!("\nRe-run with --yes to uninstall and delete the planned paths.");
        return Ok(());
    }

    println!(
        "Uninstalling {} ({}, {})...",
        pkg.name,
        pkg.source.label(),
        pkg.id
    );

    let report = backends::uninstall(&pkg, false)
        .map_err(|e| format!("failed to run package manager: {e}"))?;
    if !report.ok {
        return Err(format!("uninstall failed:\n{}", report.text()));
    }
    if !report.text().is_empty() {
        println!("{}", report.text());
    }

    println!("\nScanning for leftovers...");
    let items = scan::scan_package(&pkg);
    let candidates: Vec<scan::Leftover> = items
        .iter()
        .filter(|i| i.confidence >= Confidence::Medium)
        .cloned()
        .collect();

    print_items(&items);
    print_plan(&candidates);
    if candidates.is_empty() {
        return Ok(());
    }

    let report = scan::delete_items(&candidates);
    println!("\nDeleted {} items.", report.deleted);
    if !report.root_output.is_empty() {
        println!("{}", report.root_output);
    }
    if !report.failures.is_empty() {
        println!("Some paths could not be removed:");
        for f in &report.failures {
            println!("  {f}");
        }
    }
    Ok(())
}

fn print_plan(candidates: &[scan::Leftover]) {
    println!("\nCleanup plan (high/medium confidence):");
    if candidates.is_empty() {
        println!("  nothing to delete");
        return;
    }
    let total: u64 = candidates.iter().map(|i| i.size).sum();
    for i in candidates {
        println!(
            "  DELETE {}  {}{}",
            i.display_path(),
            if i.needs_root { "[root] " } else { "" },
            i.confidence.label()
        );
    }
    println!(
        "\n{} items, {} reclaimable",
        candidates.len(),
        fmt_size(total)
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
