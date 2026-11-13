pub mod dnf;
pub mod flatpak;

use crate::exec::Report;
use crate::model::Package;

/// List every application known to all backends.
/// Returns the packages plus non-fatal error messages.
pub fn list_all() -> (Vec<Package>, Vec<String>) {
    let dnf_handle = std::thread::spawn(dnf::list);
    let flatpak_handle = std::thread::spawn(flatpak::list);

    let mut packages = Vec::new();
    let mut errors = Vec::new();

    match dnf_handle.join() {
        Ok(Ok(mut p)) => packages.append(&mut p),
        Ok(Err(e)) => errors.push(format!("DNF: {e}")),
        Err(_) => errors.push("DNF: backend crashed".to_string()),
    }
    match flatpak_handle.join() {
        Ok(Ok(mut p)) => packages.append(&mut p),
        Ok(Err(e)) => errors.push(format!("Flatpak: {e}")),
        Err(_) => errors.push("Flatpak: backend crashed".to_string()),
    }

    packages.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    (packages, errors)
}

/// Remove a package with its backend. `delete_data` asks flatpak to purge
/// application data (~/.var/app) in the same step. For DNF it is ignored.
pub fn uninstall(pkg: &Package, delete_data: bool) -> std::io::Result<Report> {
    match pkg.source {
        crate::model::Source::Dnf => dnf::remove(&pkg.id),
        crate::model::Source::Flatpak => {
            flatpak::uninstall(&pkg.id, &pkg.installation, delete_data)
        }
    }
}

/// Paths that the package manager itself owns and will delete on uninstall.
/// The leftover scanner skips these.
pub fn owned_paths(pkg: &Package) -> Vec<std::path::PathBuf> {
    match pkg.source {
        crate::model::Source::Dnf => dnf::files(&pkg.id),
        crate::model::Source::Flatpak => Vec::new(),
    }
}
