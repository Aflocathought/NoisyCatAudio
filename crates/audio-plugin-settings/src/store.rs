use serde_json::{Map, Value, json};
use std::{
    fmt,
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_BYTES: u64 = 1024 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum Error {
    Busy,
    Invalid(String),
    Io(io::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("Shared settings are busy; retrying"),
            Self::Invalid(message) => f.write_str(message),
            Self::Io(error) => write!(f, "Cannot access shared settings: {error}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// An edit changes exactly one key, never an old copy of the whole document.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub plugin: Option<String>,
    pub key: String,
    pub value: Value,
}
impl Edit {
    pub fn global(key: &str, value: Value) -> Self {
        Self {
            plugin: None,
            key: key.into(),
            value,
        }
    }
    pub fn plugin(plugin: &str, key: &str, value: Value) -> Self {
        Self {
            plugin: Some(plugin.into()),
            key: key.into(),
            value,
        }
    }
    pub(crate) fn same_key(&self, other: &Self) -> bool {
        self.plugin == other.plugin && self.key == other.key
    }
}

#[derive(Clone, Debug)]
pub struct Document(pub Value);
impl Default for Document {
    fn default() -> Self {
        Self(json!({
            "schema_version": 1,
            "global": {"maximum_ui_fps": 60, "language": "system"},
            "plugins": {}
        }))
    }
}
impl Document {
    fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|e| Error::Invalid(format!("Invalid shared settings JSON: {e}")))?;
        if value["schema_version"].as_u64() != Some(1) {
            return Err(Error::Invalid(
                "Unsupported shared settings schema; file left unchanged".into(),
            ));
        }
        if !value["global"].is_object()
            || !value["plugins"].is_object()
            || value["plugins"]
                .as_object()
                .unwrap()
                .values()
                .any(|v| !v.is_object())
        {
            return Err(Error::Invalid(
                "Shared settings sections must be objects; file left unchanged".into(),
            ));
        }
        Ok(Self(value))
    }
    pub fn maximum_ui_fps(&self) -> u32 {
        match self.0["global"]["maximum_ui_fps"].as_u64() {
            Some(value @ (30 | 60 | 90 | 120)) => value as u32,
            _ => 60,
        }
    }
    pub fn language(&self) -> &str {
        self.0["global"]["language"].as_str().unwrap_or("system")
    }
    pub(crate) fn apply(&mut self, edits: &[Edit]) {
        for edit in edits {
            let section = if let Some(plugin) = &edit.plugin {
                self.0["plugins"]
                    .as_object_mut()
                    .unwrap()
                    .entry(plugin)
                    .or_insert_with(|| Value::Object(Map::new()))
            } else {
                &mut self.0["global"]
            };
            section
                .as_object_mut()
                .unwrap()
                .insert(edit.key.clone(), edit.value.clone());
        }
    }
}

/// Windows: %APPDATA%; macOS: Application Support; Linux: XDG_CONFIG_HOME.
/// A reverse-domain namespace groups products without relying on a Rust rule.
pub fn settings_path(namespace: &str) -> Result<PathBuf, Error> {
    if namespace.is_empty()
        || !namespace
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-_".contains(&c))
        || matches!(namespace, "." | "..")
    {
        return Err(Error::Invalid("Invalid shared settings namespace".into()));
    }
    // Explicit override supports portable installs and isolated test hosts.
    if let Some(root) = std::env::var_os("AUDIO_PLUGIN_SETTINGS_ROOT") {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(Error::Invalid("Settings root must be absolute".into()));
        }
        return Ok(root.join(namespace).join("settings.json"));
    }
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root =
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")));
    let root = root.filter(|p| p.is_absolute()).ok_or_else(|| {
        Error::Invalid("No absolute per-user settings directory available".into())
    })?;
    Ok(root.join(namespace).join("settings.json"))
}

pub struct Store {
    path: PathBuf,
}
impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn read(&self) -> Result<Document, Error> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Document::default()),
            Err(e) => return Err(e.into()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Invalid("Shared settings file is too large".into()));
        }
        Document::parse(&bytes)
    }
    pub fn update(&self, edits: &[Edit]) -> Result<Document, Error> {
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| Error::Invalid("Settings path needs a parent directory".into()))?;
        fs::create_dir_all(parent)?;
        // Lock a stable sidecar inode/handle, NOT the JSON file being replaced.
        // The OS releases this lock on crash. Never delete the lock file.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.path.with_extension("lock"))?;
        match lock.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock) => return Err(Error::Busy),
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
        // Reread under lock so stale instances cannot erase another plugin's
        // settings. Unknown fields survive edits by older compatible clients.
        let mut document = self.read()?;
        document.apply(edits);
        let mut bytes = serde_json::to_vec_pretty(&document.0).unwrap();
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Invalid(
                "Shared settings would exceed the size limit".into(),
            ));
        }
        let mut temp = None;
        for _ in 0..32 {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".settings-{}-{id}.tmp", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    temp = Some((path, file));
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        let (path, mut file) = temp
            .ok_or_else(|| Error::Invalid("Cannot create unique settings staging file".into()))?;
        let result = (|| -> Result<(), Error> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            // Same-directory rename replaces atomically on supported local
            // filesystems, so readers see either a complete old or new JSON.
            fs::rename(&path, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&path);
        }
        result?;
        Ok(document)
    }
}
