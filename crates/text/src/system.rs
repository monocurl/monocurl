use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, anyhow, bail};

use crate::{SystemBackendConfig, SystemBackendStatus, SystemToolPaths};

const TEX_BASENAME: &str = "monocurl";

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) fn discover_backend() -> SystemToolPaths {
    SystemToolPaths {
        latex: find_command("xelatex").or_else(|| find_command("latex")),
        dvisvgm: find_command("dvisvgm"),
    }
}

pub(crate) fn backend_status(config: &SystemBackendConfig) -> SystemBackendStatus {
    SystemBackendStatus {
        latex: command_available(&config.latex),
        dvisvgm: command_available(&config.dvisvgm),
    }
}

pub(crate) fn render_svg_document(document: &str, config: &SystemBackendConfig) -> Result<String> {
    let engine = LatexEngine::for_command(&config.latex);
    let temp_dir = TempDir::new()?;
    let tex_path = temp_dir.path().join(format!("{TEX_BASENAME}.tex"));
    let output_path = temp_dir
        .path()
        .join(format!("{TEX_BASENAME}.{}", engine.output_extension()));
    let svg_path = temp_dir.path().join(format!("{TEX_BASENAME}.svg"));

    fs::write(&tex_path, document).with_context(|| {
        format!(
            "failed to write temporary TeX file `{}`",
            tex_path.display()
        )
    })?;

    run_command(&config.latex, engine.args(temp_dir.path(), &tex_path))?;

    run_command(
        &config.dvisvgm,
        vec![
            output_path.display().to_string(),
            "-v".into(),
            "0".into(),
            "-n".into(),
            "-o".into(),
            svg_path.display().to_string(),
        ],
    )?;

    fs::read_to_string(&svg_path)
        .with_context(|| format!("failed to read generated SVG `{}`", svg_path.display()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LatexEngine {
    Latex,
    XeLatex,
}

impl LatexEngine {
    fn for_command(command: &Path) -> Self {
        let name = command
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if name.contains("xelatex") {
            Self::XeLatex
        } else {
            Self::Latex
        }
    }

    fn args(self, output_dir: &Path, tex_path: &Path) -> Vec<String> {
        let mut args = vec![
            "-interaction=nonstopmode".into(),
            "-halt-on-error".into(),
            format!("-output-directory={}", output_dir.display()),
        ];
        if self == Self::XeLatex {
            args.push("-no-pdf".into());
        }
        args.push(tex_path.display().to_string());
        args
    }

    fn output_extension(self) -> &'static str {
        match self {
            Self::Latex => "dvi",
            Self::XeLatex => "xdv",
        }
    }
}

fn command_available(command: &Path) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn run_command(command: &Path, args: Vec<String>) -> Result<()> {
    let output = Command::new(command).args(args).output().map_err(|error| {
        let command = command.display();
        if error.kind() == std::io::ErrorKind::NotFound {
            anyhow!("system LaTeX backend requires `{command}`")
        } else {
            anyhow!("failed to start `{command}`: {error}")
        }
    })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let command = command.display();
    bail!(
        "`{command}` exited with status {}{}\n{}",
        output.status,
        if stderr.is_empty() { "" } else { ":" },
        if stderr.is_empty() { stdout } else { stderr }
    );
}

fn find_command(name: &str) -> Option<PathBuf> {
    let names = candidate_names(name);
    for dir in env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .chain(common_command_dirs())
    {
        for name in &names {
            let candidate = dir.join(name);
            if command_available(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn candidate_names(name: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let mut out = vec![name.to_owned()];
        if !name.ends_with(".exe") {
            out.push(format!("{name}.exe"));
        }
        out
    }

    #[cfg(not(target_os = "windows"))]
    {
        vec![name.to_owned()]
    }
}

fn common_command_dirs() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        [
            "/Library/TeX/texbin",
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/usr/texbin",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect()
    }

    #[cfg(target_os = "windows")]
    {
        [
            r"C:\texlive\2026\bin\windows",
            r"C:\texlive\2025\bin\windows",
            r"C:\texlive\2024\bin\windows",
            r"C:\Program Files\MiKTeX\miktex\bin\x64",
            r"C:\Program Files\MiKTeX 2.9\miktex\bin\x64",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect()
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        ["/usr/local/bin", "/usr/bin", "/bin"]
            .into_iter()
            .map(PathBuf::from)
            .collect()
    }
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "monocurl-latex-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).with_context(|| {
            format!("failed to create temporary directory `{}`", path.display())
        })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
