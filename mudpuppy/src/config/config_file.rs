use std::fmt::Debug;
use std::fs::OpenOptions;
use std::io::{Read, Seek, Write};
use std::sync::{Arc, RwLock, RwLockReadGuard};

use crossterm::event::KeyEvent;
use pyo3::{pyclass, pymethods};
use serde::{Deserialize, Serialize};
use tokio_rustls::rustls::pki_types;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Value};
use tracing::{debug, info, trace, warn};

use super::keybindings::KeyBindings;
use crate::app::TabKind;
use crate::config::{config_dir, config_file, data_dir};
use crate::error::{ConfigError, Error};
use crate::model::{Mud, Shortcut, Tls};

/// A [`Config`] that is shared globally for the entire application.
#[derive(Debug, Clone)]
#[allow(clippy::module_name_repetitions)]
#[pyclass(name = "Config")]
pub struct GlobalConfig(Arc<RwLock<Config>>);

impl GlobalConfig {
    /// Construct a new global config instance that is safe for concurrent access.
    ///
    /// # Errors
    ///
    /// Returns an error if loading config content from disk fails, for example
    /// because the [`config_file()`] is invalid.
    pub fn new() -> crate::Result<Self> {
        Ok(Self(Arc::new(RwLock::new(Config::new()?))))
    }

    /// Reload the configuration from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the new configuration is invalid.
    pub fn reload(&self) -> crate::Result<(), Error> {
        if let Ok(mut config) = self.0.write() {
            return config.load().map_err(Into::into);
        }
        Ok(())
    }

    pub fn lookup<T: Clone>(
        &self,
        f: impl FnOnce(RwLockReadGuard<'_, Config>) -> T,
        default: T,
    ) -> T {
        let Ok(config) = self.0.read() else {
            return default;
        };
        f(config)
    }

    #[must_use]
    pub fn key_binding(&self, tab_kind: &TabKind, event: &KeyEvent) -> Option<Shortcut> {
        self.lookup(
            |config| {
                // TODO(XXX): Consider multi-key shortcuts. Requires buffering KeyEvents somewhere.
                let tab_shortcuts = config.keybindings.0.get(tab_kind.config_key())?;
                tab_shortcuts.get(&vec![*event]).cloned()
            },
            None,
        )
    }
}

#[pymethods]
impl GlobalConfig {
    #[must_use]
    pub fn lookup_mud(&self, mud_name: &str) -> Option<Mud> {
        self.lookup(
            |config| config.muds.iter().find(|m| m.name == mud_name).cloned(),
            None,
        )
    }

    /// # Errors
    ///
    /// Returns an error if the MUD can't be found in the configuration by the given name.
    pub fn must_lookup_mud(&self, mud_name: &str) -> crate::Result<Mud> {
        self.lookup_mud(mud_name)
            .ok_or(ConfigError::MissingMud(mud_name.to_string()).into())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[allow(clippy::unsafe_derive_deserialize)] // No constructor invariants to uphold.
pub struct Config {
    #[serde(default)]
    pub muds: Vec<Mud>,

    #[serde(default)]
    pub keybindings: KeyBindings,
}

impl Config {
    /// Construct and load configuration.
    ///
    /// If [`config_file()`] exists, it will be loaded and used to populate the config.
    /// If it does not, then the default `CONFIG` will be used instead.
    ///
    /// # Errors
    /// If the config content is not valid TOML an error will be returned.
    pub fn new() -> crate::Result<Self> {
        let mut cfg = Self::default();
        cfg.load()?;
        Ok(cfg)
    }

    #[allow(clippy::missing_errors_doc)] // TODO(XXX): doc
    pub fn load(&mut self) -> crate::Result<(), ConfigError> {
        let default_config: Config = toml::from_str(CONFIG)?;
        let config_file = config_file();

        if !config_file.exists() {
            warn!("No configuration file found. Using defaults.");
        }

        let builder = config::Config::builder()
            // Safety: `set_default()` is documented to only panic if string conversion of the key fails.
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

        if cfg.keybindings.0.is_empty() {
            trace!(
                "No keybindings found in config. Using defaults ({} mode bindings)",
                default_config.keybindings.0.len()
            );
            cfg.keybindings = default_config.keybindings;
        } else {
            if let Some(unknown_mode) = cfg
                .keybindings
                .0
                .keys()
                .find(|k| *k != "mudlist" && *k != "mud")
            {
                warn!("unknown keybinding mode {unknown_mode:?} found in config");
            }
            debug!("merging keybindings from config with defaults");
            for (mode, bindings) in default_config.keybindings.0 {
                let user_bindings = cfg.keybindings.0.entry(mode.to_lowercase()).or_default();
                for (key_seq, shortcut) in bindings {
                    user_bindings.entry(key_seq).or_insert(shortcut);
                }
            }
        }

        for (mode, bindings) in &cfg.keybindings.0 {
            trace!("key bindings for mode: {mode}");
            for (key_seq, shortcut) in bindings {
                trace!("{}: {key_seq:?} -> {shortcut:?}", mode.to_lowercase());
            }
        }

        cfg.validate()?;

        *self = cfg;
        Ok(())
    }

    fn validate(&self) -> crate::Result<(), ConfigError> {
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

/// Set the TOML config value under `key` to `v` for the MUD with the given `name`.
///
/// # Errors
///
/// Returns an error if config file can't be opened, read, written to, or if it contains
/// invalid TOML content.
pub fn edit_mud(name: &str, key: &str, v: impl Into<Value> + Debug) -> crate::Result<()> {
    let mut config_file = OpenOptions::new()
        .read(true)
        .write(true)
        .append(false)
        .open(config_file())?;

    let mut config_data = String::new();
    config_file.read_to_string(&mut config_data)?;
    let mut config_doc = config_data
        .parse::<DocumentMut>()
        .map_err(|err| Error::Config(ConfigError::TomlEdit(err)))?;

    let Some(muds) = config_doc
        .entry("muds")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::default()))
        .as_array_of_tables_mut()
    else {
        warn!("invalid 'muds' config data type - config update not persisted");
        return Ok(());
    };

    let Some(mud) = muds.iter_mut().find(|mud| {
        if let Some(Item::Value(Value::String(mud_name))) = mud.get("name") {
            mud_name.value() == name
        } else {
            false
        }
    }) else {
        warn!("no mud named {name} - config update not persisted");
        return Ok(());
    };

    info!("updated {name} {key} to {v:?}");
    mud[key] = Item::Value(v.into());

    config_file.rewind()?;
    config_file.write_all(config_doc.to_string().as_bytes())?;

    Ok(())
}

const CONFIG: &str = include_str!("../../../.config/config.toml");
