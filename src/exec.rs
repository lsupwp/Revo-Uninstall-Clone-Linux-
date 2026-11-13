use std::process::{Command, Stdio};

/// Result of running an external command.
#[derive(Clone, Debug)]
pub struct Report {
    pub ok: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Report {
    fn from_output(output: std::process::Output) -> Self {
        Report {
            ok: output.status.success(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }
    }

    /// Combined output suitable for the log panel.
    pub fn text(&self) -> String {
        let mut out = String::new();
        if !self.stdout.is_empty() {
            out.push_str(&self.stdout);
        }
        if !self.stderr.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&self.stderr);
        }
        if out.is_empty() {
            out.push_str(&format!("exit code {:?}", self.code));
        }
        out
    }
}

/// Run a program and capture stdout/stderr with a stable C locale.
pub fn run(program: &str, args: &[&str]) -> std::io::Result<Report> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .output()?;
    Ok(Report::from_output(output))
}

pub fn run_strings(program: &str, args: &[String]) -> std::io::Result<Report> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(program, &refs)
}

/// True when the current user is root.
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// Absolute path of a tool, falling back to the plain name.
pub fn tool(name: &str) -> String {
    for dir in ["/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
        let p = format!("{dir}/{name}");
        if std::path::Path::new(&p).exists() {
            return p;
        }
    }
    name.to_string()
}
