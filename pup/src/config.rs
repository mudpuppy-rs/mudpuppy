use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use config as config_crate;
use directories::ProjectDirs;
use futures::channel::mpsc::{channel as futures_channel, Receiver};
use futures::SinkExt;
use notify::{
    Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use pyo3::pyclass;
use serde::{Deserialize, Serialize};
use tokio_rustls::rustls::pki_types;
use tracing::{info, warn};

use crate::error::ConfigError;
use crate::session::{Mud, Tls};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[pyclass]
#[allow(clippy::unsafe_derive_deserialize)] // No constructor invariants to uphold.
pub(crate) struct Config {
    #[serde(default)]
    #[pyo3(get, set)]
    pub(crate) muds: Vec<Mud>,
}

impl Config {
    pub(crate) fn new() -> Result<Self, ConfigError> {
        let mut cfg = Self { muds: Vec::new() };
        cfg.load()?;
        Ok(cfg)
    }

    pub(crate) fn load(&mut self) -> Result<(), ConfigError> {
        let default_config: Config = toml::from_str(include_str!("../../.config/config.toml"))?;
        let config_file = config_file();

        if !config_file.exists() {
            warn!("No configuration file found. Using defaults.");
        }

        let builder = config_crate::Config::builder()
            .set_default("_data_dir", data_dir().to_str().unwrap_or_default())?
            .set_default("_config_dir", config_dir().to_str().unwrap_or_default())?
            .add_source(
                config::File::from(config_file)
                    .format(config::FileFormat::Toml)
                    .required(false),
            );

        let mut cfg: Self = builder.build()?.try_deserialize()?;
        if cfg.muds.is_empty() {
            cfg.muds = default_config.muds;
        }
        cfg.validate()?;

        *self = cfg;
        Ok(())
    }

    fn validate(&self) -> Result<(), ConfigError> {
        for mud in &self.muds {
            if mud.name.is_empty() {
                return Err(ConfigError::InvalidMud("name is empty".to_string()));
            }

            if mud.host.is_empty() {
                return Err(ConfigError::InvalidMud(format!(
                    "MUD {:?} host is empty",
                    mud.name
                )));
            }

            if matches!(mud.tls, Tls::Enabled) {
                pki_types::ServerName::try_from(mud.host.as_str()).map_err(|e| {
                    ConfigError::InvalidMud(format!(
                        "MUD {:?} hostname {:?} invalid for TLS: {e}",
                        mud.name, mud.host
                    ))
                })?;
            }
        }

        Ok(())
    }
}

pub(super) fn reload_watcher(
) -> NotifyResult<(RecommendedWatcher, Receiver<NotifyResult<NotifyEvent>>)> {
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
    info!("registering watch for {}", config_dir_path.display());
    watcher.watch(config_dir_path, RecursiveMode::NonRecursive)?;

    Ok((watcher, config_event_rx))
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

pub static CRATE_NAME: &str = env!("CARGO_CRATE_NAME");
