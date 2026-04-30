use std::io::{self, Write};

use exporter::{ExportOutcome, ExportProgress, SceneInspectionOutcome};

use crate::command::CommandKind;

const PROGRESS_BAR_WIDTH: usize = 28;

pub(crate) struct TerminalProgress {
    title: &'static str,
    last_line_len: usize,
}

impl TerminalProgress {
    pub(crate) fn new(kind: CommandKind) -> Self {
        eprintln!("{}", kind.progress_title());
        Self {
            title: kind.progress_title(),
            last_line_len: 0,
        }
    }

    pub(crate) fn update(&mut self, progress: &ExportProgress) -> io::Result<()> {
        let ratio = if progress.total == 0 {
            0.35
        } else {
            progress.ratio()
        };
        let filled = (ratio * PROGRESS_BAR_WIDTH as f32).round() as usize;
        let filled = filled.min(PROGRESS_BAR_WIDTH);
        let bar = format!(
            "[{}{}]",
            "#".repeat(filled),
            "-".repeat(PROGRESS_BAR_WIDTH - filled)
        );

        let mut line = format!("{bar} {}", progress.message);
        if progress.total > 0 {
            line.push_str(&format!(" ({}/{})", progress.completed, progress.total));
        }

        let padding = self.last_line_len.saturating_sub(line.len());
        let mut stderr = io::stderr().lock();
        write!(stderr, "\r{line}{:padding$}", "", padding = padding)?;
        stderr.flush()?;
        self.last_line_len = line.len();
        Ok(())
    }

    pub(crate) fn finish_export(
        &mut self,
        kind: CommandKind,
        outcome: &ExportOutcome,
    ) -> io::Result<()> {
        self.clear_line()?;
        eprintln!("{}", kind.success_title());
        eprintln!("Saved to {}", outcome.output_path.display());
        if matches!(kind, CommandKind::Video) {
            eprintln!("Frames written: {}", outcome.frames_written);
        }
        if !outcome.transcript.is_empty() {
            eprintln!();
            eprintln!("Print output:");
            for entry in &outcome.transcript {
                eprintln!("  {}", entry.text());
            }
        }
        Ok(())
    }

    pub(crate) fn finish_transcript(&mut self, outcome: &SceneInspectionOutcome) -> io::Result<()> {
        self.clear_line()?;
        eprintln!("{}", CommandKind::Transcript.success_title());
        eprintln!(
            "Reached slide {} at {}",
            outcome.timestamp.slide.saturating_sub(1),
            format_time(outcome.timestamp.time)
        );
        Ok(())
    }

    pub(crate) fn fail(&mut self, kind: CommandKind, cancelled: bool) -> io::Result<()> {
        self.clear_line()?;
        if cancelled {
            eprintln!("{}", kind.canceled_title());
        } else {
            eprintln!("{}", kind.failure_title());
        }
        Ok(())
    }

    fn clear_line(&mut self) -> io::Result<()> {
        if self.last_line_len == 0 {
            return Ok(());
        }

        let mut stderr = io::stderr().lock();
        write!(
            stderr,
            "\r{:width$}\r",
            "",
            width = self.last_line_len.max(self.title.len())
        )?;
        stderr.flush()?;
        self.last_line_len = 0;
        Ok(())
    }
}

fn format_time(time: f64) -> String {
    if time.is_infinite() {
        "end".into()
    } else {
        format!("{time:.3}s")
    }
}
