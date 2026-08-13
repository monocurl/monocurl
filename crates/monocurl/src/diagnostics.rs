use std::{
    fs::{self, File, OpenOptions},
    io::{self, Stderr, Write},
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use env_logger::Target;

use crate::auto_update;

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
const RETAINED_LOGS: usize = 3;

struct TeeWriter {
    file: Mutex<File>,
    stderr: Stderr,
}

impl Write for TeeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut file = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        file.write_all(bytes)?;
        let _ = self.stderr.write_all(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut file = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        file.flush()?;
        let _ = self.stderr.flush();
        Ok(())
    }
}

pub fn initialize() -> Option<PathBuf> {
    let log_path = log_path()?;
    if let Err(error) = rotate_logs(&log_path) {
        eprintln!("unable to rotate Monocurl logs: {error}");
    }

    let file = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!(
                "unable to open Monocurl log {}: {error}",
                log_path.display()
            );
            install_panic_hook(log_path.clone());
            return None;
        }
    };

    let mut builder = env_logger::Builder::from_default_env();
    builder
        .filter_level(log::LevelFilter::Info)
        .format_timestamp_millis()
        .target(Target::Pipe(Box::new(TeeWriter {
            file: Mutex::new(file),
            stderr: io::stderr(),
        })))
        .init();

    install_panic_hook(log_path.clone());
    log::info!(
        "Monocurl started; version={} os={} arch={} diagnostic log: {}",
        auto_update::CURRENT_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
        log_path.display()
    );
    Some(log_path)
}

fn log_path() -> Option<PathBuf> {
    Some(
        dirs::data_local_dir()?
            .join("Monocurl")
            .join("logs")
            .join("monocurl.log"),
    )
}

fn rotate_logs(log_path: &PathBuf) -> io::Result<()> {
    if log_path
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        < MAX_LOG_BYTES
    {
        fs::create_dir_all(log_path.parent().unwrap())?;
        return Ok(());
    }

    for index in (1..=RETAINED_LOGS).rev() {
        let source = if index == 1 {
            log_path.clone()
        } else {
            log_path.with_extension(format!("log.{previous}", previous = index - 1))
        };
        let destination = log_path.with_extension(format!("log.{index}"));
        if destination.exists() {
            let _ = fs::remove_file(&destination);
        }
        if source.exists() {
            fs::rename(source, destination)?;
        }
    }
    fs::create_dir_all(log_path.parent().unwrap())?;
    Ok(())
}

fn install_panic_hook(log_path: PathBuf) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let location = panic_info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown".to_string());
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| {
                panic_info
                    .payload()
                    .downcast_ref::<String>()
                    .map(String::as_str)
            })
            .unwrap_or("non-string panic payload");
        let crash_path = log_path.with_file_name(format!("crash-{timestamp}.log"));

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&crash_path)
        {
            let _ = writeln!(file, "Monocurl panic report");
            let _ = writeln!(file, "version: {}", auto_update::CURRENT_VERSION);
            let _ = writeln!(
                file,
                "os: {} {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            let _ = writeln!(file, "location: {location}");
            let _ = writeln!(file, "message: {payload}");
            let _ = writeln!(
                file,
                "backtrace:\n{}",
                std::backtrace::Backtrace::force_capture()
            );
            let _ = file.flush();
        }

        previous_hook(panic_info);
    }));
}
