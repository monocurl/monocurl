use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, anyhow, bail};

const TEX_BASENAME: &str = "monocurl";

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) fn render_svg_document(document: &str) -> Result<String> {
    let temp_dir = TempDir::new()?;
    let tex_path = temp_dir.path().join(format!("{TEX_BASENAME}.tex"));
    let dvi_path = temp_dir.path().join(format!("{TEX_BASENAME}.dvi"));
    let svg_path = temp_dir.path().join(format!("{TEX_BASENAME}.svg"));

    fs::write(&tex_path, document).with_context(|| {
        format!(
            "failed to write temporary TeX file `{}`",
            tex_path.display()
        )
    })?;

    run_command(
        "latex",
        vec![
            "-interaction=nonstopmode".into(),
            "-halt-on-error".into(),
            format!("-output-directory={}", temp_dir.path().display()),
            tex_path.display().to_string(),
        ],
    )?;

    run_command(
        "dvisvgm",
        vec![
            dvi_path.display().to_string(),
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

fn run_command(command: &str, args: Vec<String>) -> Result<()> {
    let output = Command::new(command).args(args).output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            anyhow!("system LaTeX backend requires `{command}` on PATH")
        } else {
            anyhow!("failed to start `{command}`: {error}")
        }
    })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    bail!(
        "`{command}` exited with status {}{}\n{}",
        output.status,
        if stderr.is_empty() { "" } else { ":" },
        if stderr.is_empty() { stdout } else { stderr }
    );
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
