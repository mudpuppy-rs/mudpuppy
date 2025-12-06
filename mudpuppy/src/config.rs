use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use config as config_crate;
use directories::ProjectDirs;
use futures::SinkExt;
use futures::channel::mpsc::{Receiver, channel as futures_channel};
use notify::{
    Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use pyo3::{Python, pyclass, pymethods};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, Instant as TokioInstant};
use tokio_rustls::rustls::pki_types;
use tracing::{info, warn};

use crate::error::{ConfigError, Error, ErrorKind};
use crate::python::{APP, Command};
use crate::session::{Character, Tls};

#[derive(Debug, Default)]
pub(crate) struct ConfigState {
    pub(super) pending_save: Option<TokioInstant>,
    last_save_hash: Option<u64>,
}

impl ConfigState {
    pub(crate) fn queue_save(&mut self) {
        self.pending_save = Some(TokioInstant::now() + CONFIG_SAVE_DEBOUNCE);
    }

    pub(crate) fn should_save_now(&self) -> bool {
        self.pending_save
            .is_some_and(|deadline| TokioInstant::now() >= deadline)
    }

    pub(crate) fn saved(&mut self, new_hash: u64) {
        self.last_save_hash = Some(new_hash);
        self.pending_save = None;
    }

    pub(crate) fn up_to_date(&self, current_hash: u64) -> bool {
        self.last_save_hash == Some(current_hash)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[pyclass]
#[allow(clippy::unsafe_derive_deserialize)]
pub(crate) struct Config {
    #[serde(default, rename = "character")]
    characters: Vec<Character>,

    #[serde(default)]
    modules: Vec<String>,

    #[serde(default = "default::mouse_enabled")]
    mouse_enabled: bool,
}

impl Config {
    pub(crate) fn new() -> Result<Self, ConfigError> {
        let mut cfg = Self::default();
        cfg.load()?;
        Ok(cfg)
    }

    pub(crate) fn load(&mut self) -> Result<(), ConfigError> {
        // TODO(XXX): default config stuff...
        // let default_config = toml::from_str::<Config>(include_str!("../../.config/config.toml"))?;
        let config_file = config_file();
        if !config_file.exists() {
            warn!("No configuration file found. Using defaults.");
        }

        let builder = config_crate::Config::builder()
            .set_default("_data_dir", data_dir().to_str().unwrap_or_default())?
            .set_default("_config_dir", config_dir().to_str().unwrap_or_default())?
            .add_source(
                config_crate::File::from(config_file)
                    .format(config_crate::FileFormat::Toml)
                    .required(false),
            );

        let cfg: Self = builder.build()?.try_deserialize()?;
        cfg.validate()?;

        *self = cfg;

        Ok(())
    }

    pub(crate) fn characters(&self) -> &[Character] {
        &self.characters
    }

    pub(crate) fn modules(&self) -> &[String] {
        &self.modules
    }

    pub(crate) fn mouse_enabled(&self) -> bool {
        self.mouse_enabled
    }

    pub(crate) fn save(&self) -> Result<u64, ConfigError> {
        self.validate()?;

        let toml = toml::to_string(self)?;
        let hash = {
            let mut hasher = DefaultHasher::new();
            toml.hash(&mut hasher);
            hasher.finish()
        };

        let config_path = config_file();
        let tmp_path = config_path.with_extension("toml.tmp");

        fs::write(&tmp_path, &toml)?;
        fs::rename(tmp_path, config_path)?;

        info!("configuration saved");
        Ok(hash)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let mut seen = HashSet::new();

        for character in &self.characters {
            if seen.contains(&character.name) {
                return Err(ConfigError::InvalidCharacter(format!(
                    "multiple characters named {}",
                    character.name
                )));
            }
            seen.insert(&character.name);

            if character.name.is_empty() {
                return Err(ConfigError::InvalidMud("name is empty".to_string()));
            }

            if character.mud.host.is_empty() {
                return Err(ConfigError::InvalidMud(format!(
                    "MUD {:?} host is empty",
                    character.mud.name
                )));
            }

            if matches!(character.mud.tls, Tls::Enabled) {
                pki_types::ServerName::try_from(character.mud.host.as_str()).map_err(|e| {
                    ConfigError::InvalidMud(format!(
                        "MUD {:?} hostname {:?} invalid for TLS: {e}",
                        character.mud.name, character.mud.host
                    ))
                })?;
            }
        }

        Ok(())
    }
}

#[pymethods]
impl Config {
    #[getter]
    fn get_characters(&self) -> Vec<Character> {
        self.characters.clone()
    }

    #[setter]
    fn set_characters(&mut self, py: Python<'_>, value: Vec<Character>) -> pyo3::PyResult<()> {
        self.characters = value;
        dispatch_save_config(py)?;
        Ok(())
    }

    #[getter]
    fn get_mouse_enabled(&self) -> bool {
        self.mouse_enabled
    }

    #[setter]
    fn set_mouse_enabled(&mut self, py: Python<'_>, value: bool) -> pyo3::PyResult<()> {
        self.mouse_enabled = value;
        dispatch_save_config(py)?;
        Ok(())
    }
}

pub(crate) fn compute_file_hash() -> Result<u64, ConfigError> {
    let config_path = config_file();
    if !config_path.exists() {
        return Ok(0);
    }

    let contents = fs::read_to_string(config_path)?;
    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    Ok(hasher.finish())
}

pub(super) fn reload_watcher()
-> NotifyResult<(RecommendedWatcher, Receiver<NotifyResult<NotifyEvent>>)> {
    let (mut config_event_tx, config_event_rx) = futures_channel(1);
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            futures::executor::block_on(async {
                config_event_tx.send(res).await.unwrap();
            });
        },
        notify::Config::default(),
    )?;

    let config_dir_path = config_dir();
    info!(
        config_dir_path = config_dir_path.display().to_string(),
        "registering watch"
    );
    watcher.watch(config_dir_path, RecursiveMode::NonRecursive)?;

    Ok((watcher, config_event_rx))
}

#[must_use]
pub(crate) fn version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            let core_commit_hash = GIT_COMMIT_HASH;
            let config_dir_path = config_dir().display().to_string();
            let data_dir_path = data_dir().display().to_string();

            format!(
                "\
{core_commit_hash}

Config directory: {config_dir_path}
Data directory: {data_dir_path}"
            )
        })
        .as_ref()
}

#[must_use]
#[allow(clippy::module_name_repetitions)]
pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

#[must_use]
#[allow(clippy::module_name_repetitions)]
pub fn config_dir() -> &'static Path {
    static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();
    lazy_overridable_dir(
        &format!("{}_CONFIG", CRATE_NAME.to_uppercase()),
        DirType::Config,
        &CONFIG_DIR,
    )
}

#[must_use]
pub fn data_dir() -> &'static Path {
    static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
    lazy_overridable_dir(
        &format!("{}_DATA", CRATE_NAME.to_uppercase()),
        DirType::Data,
        &DATA_DIR,
    )
}

pub fn project_dir() -> Option<&'static ProjectDirs> {
    static PROJECT_DIR: OnceLock<Option<ProjectDirs>> = OnceLock::new();
    PROJECT_DIR
        .get_or_init(|| {
            // TODO(XXX): register/use a project domain.
            ProjectDirs::from("ca.woodweb", CRATE_NAME, CRATE_NAME)
        })
        .as_ref()
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum DirType {
    Data,
    Config,
}

fn lazy_overridable_dir(
    env_var: &str,
    r#type: DirType,
    lock: &'static OnceLock<PathBuf>,
) -> &'static Path {
    lock.get_or_init(|| {
        match env::var(env_var).ok() {
            // User env var specified path is the first priority.
            Some(custom_path) => PathBuf::from(custom_path),
            None => match (project_dir(), r#type) {
                // Otherwise fall back to ProjectDirs.
                (Some(proj_dirs), DirType::Data) => proj_dirs.data_local_dir().into(),
                (Some(proj_dirs), DirType::Config) => proj_dirs.config_local_dir().into(),
                // And as a last resort, pwd and a subdir.
                (None, DirType::Data) => PathBuf::from(".").join(".data"),
                (None, DirType::Config) => PathBuf::from(".").join(".config"),
            },
        }
    })
}

fn dispatch_save_config(py: Python<'_>) -> pyo3::PyResult<()> {
    APP.get(py)
        .ok_or_else(|| Error::from(ErrorKind::Internal("app not yet initialized".to_owned())))?
        .send(Command::SaveConfig)
        .map_err(|e| Error::from(ErrorKind::from(e)))?;
    Ok(())
}

pub static CRATE_NAME: &str = env!("CARGO_CRATE_NAME");

pub static GIT_COMMIT_HASH: &str = env!("MUDPUPPY_GIT_INFO");

const CONFIG_SAVE_DEBOUNCE: Duration = Duration::from_millis(200);

// 🤷 https://github.com/serde-rs/serde/issues/368
mod default {
    pub(super) fn mouse_enabled() -> bool {
        true
    }
}
