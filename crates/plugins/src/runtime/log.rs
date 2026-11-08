use std::io::Write as _;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use kg_domain::{OpError, PluginId};

use crate::Result;

pub(crate) const LIMIT: u64 = 10 * 1024 * 1024;
static WRITER: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::str::FromStr for Level {
    type Err = OpError;

    fn from_str(level: &str) -> Result<Self> {
        match level {
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(OpError::Invalid(format!("unknown log level {level:?}"))),
        }
    }
}

pub(crate) fn log(data_dir: &Path, id: &PluginId, level: Level, message: &str) -> Result<()> {
    let directory = kg_core::plugin_dir(data_dir, id);
    std::fs::create_dir_all(&directory)?;
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| OpError::Failed(error.to_string()))?
        .as_millis();
    let mut line =
        serde_json::to_vec(&serde_json::json!({ "at": at, "level": level, "message": message }))?;
    line.push(b'\n');
    let size = u64::try_from(line.len()).map_err(|error| OpError::Failed(error.to_string()))?;
    if size > LIMIT {
        return Err(OpError::Invalid("log record exceeds 10 MiB".into()));
    }
    let _writer = WRITER
        .lock()
        .map_err(|error| OpError::Failed(error.to_string()))?;
    let path = kg_core::plugin_log_file(data_dir, id);
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.len() + size > LIMIT => {
            let previous = directory.join("logs.1.jsonl");
            match std::fs::remove_file(&previous) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            std::fs::rename(&path, previous)?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(&line)?;
    Ok(())
}
