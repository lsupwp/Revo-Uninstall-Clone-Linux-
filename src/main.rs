mod app;
mod backends;
mod cli;
mod exec;
mod icons;
mod model;
mod scan;
mod search;
mod util;

fn main() -> eframe::Result<()> {
    // Behave like a normal Unix CLI when stdout is piped into head/less.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::dispatch(&args) {
        std::process::exit(code);
    }

    use_software_gl_on_vmware();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("RevoLinux")
            .with_icon(std::sync::Arc::new(app_icon())),
        ..Default::default()
    };

    eframe::run_native(
        "RevoLinux",
        options,
        Box::new(|cc| Ok(Box::new(app::RevoApp::new(cc)))),
    )
}

/// VMware's virtual GPU (vmwgfx) often has no usable EGL device inside a
/// guest. Mesa then probes Zink and prints warnings before falling back to
/// llvmpipe. Select llvmpipe up front so the console stays clean.
fn use_software_gl_on_vmware() {
    if std::env::var_os("LIBGL_ALWAYS_SOFTWARE").is_some() {
        return;
    }
    let driver = std::fs::read_link("/sys/class/drm/card0/device/driver").ok();
    let is_vmware = driver
        .as_deref()
        .and_then(|p| p.file_name())
        .map(|n| n == "vmwgfx")
        .unwrap_or(false);
    if is_vmware {
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    }
}

/// Simple procedural window icon: blue rounded square with a white trash can.
fn app_icon() -> egui::IconData {
    const N: u32 = 64;
    let mut rgba = vec![0u8; (N * N * 4) as usize];

    let mix = |a: [u8; 3], b: [u8; 3], t: f32| -> [u8; 3] {
        let t = t.clamp(0.0, 1.0);
        [
            (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
            (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
            (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
        ]
    };

    for y in 0..N {
        for x in 0..N {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let half = 30.0;
            let radius = 14.0;
            let hx = (px - 32.0).abs();
            let hy = (py - 32.0).abs();
            let inside = if hx <= half - radius || hy <= half - radius {
                hx <= half && hy <= half
            } else {
                (hx - (half - radius)).powi(2) + (hy - (half - radius)).powi(2) <= radius * radius
            };
            if !inside {
                continue;
            }

            let base = mix([0x4f, 0xa3, 0xff], [0x2b, 0x6f, 0xc4], (px + py) / 128.0);
            let mut color = [base[0], base[1], base[2], 255];

            // Trash can body (trapezoid).
            if (27.0..51.0).contains(&py) {
                let half_w = 10.0 - 2.0 * (py - 27.0) / 24.0;
                if (px - 32.0).abs() <= half_w {
                    color = [255, 255, 255, 255];
                    // Two slats.
                    if (33.0..47.0).contains(&py)
                        && ((px - 28.5).abs() < 1.4 || (px - 35.5).abs() < 1.4)
                    {
                        color = [0x2b, 0x6f, 0xc4, 255];
                    }
                }
            }
            // Lid.
            if (19.0..45.0).contains(&px) && (22.0..26.0).contains(&py) {
                color = [255, 255, 255, 255];
            }
            // Handle.
            if (27.0..37.0).contains(&px) && (17.0..23.0).contains(&py) {
                color = [255, 255, 255, 255];
            }
            // Sparkle.
            let dx = (px - 47.0).abs();
            let dy = (py - 15.0).abs();
            if dx + dy <= 5.5 {
                color = [0xff, 0xd1, 0x66, 255];
            }

            let i = ((y * N + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&color);
        }
    }

    egui::IconData {
        rgba,
        width: N,
        height: N,
    }
}
