//! File-backed configuration system with hierarchical settings and runtime overlays.
//!
//! Important goals:
//! 1. A config file on disk that's easy for humans to edit. Easy means that there should be a
//!    hierarchy of defaults, minimizing the per-character, per-mud config required.
//! 2. Ability to change settings at runtime, through Rust code, TUI shortcuts, or Python
//!    user script programmatic action. Runtime changes are overlaid on top of config state.
//! 4. Changing the config file should automatically reload settings.
//! 5. Reloading the config file clears out runtime changes, making the config file the source
//!    of truth.
//! 6. Python scripts can listen to config reload events to re-apply runtime changes if
//!    applicable.
//! 7. Make it easy to add new settings w/o re-defining the field in many structs.

use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{env, fs};

use directories::ProjectDirs;
use futures::SinkExt;
use futures::channel::mpsc::{Receiver, channel as futures_channel};
use notify::{
    Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use pyo3::conversion::FromPyObjectOwned;
use pyo3::impl_::pyclass::ExtractPyClassWithClone;
use pyo3::{
    Py, PyClass, PyClassInitializer, PyResult, Python, pyclass, pymethods,
    types::{PyAnyMethods, PyDict, PyDictMethods},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio_rustls::rustls::pki_types::ServerName;
use tracing::info;

use crate::error::{ConfigError, Error, ErrorKind};

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

macro_rules! settings {
    (
        $(#[$settings_meta:meta])*
        struct Settings {
            $(
                $(#[$field_meta:meta])*
                $field:ident: $ty:ty = $default:expr
            ),* $(,)?
        }
    ) => {
        $(#[$settings_meta])*
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[pyclass]
        #[serde(default)]
        #[allow(clippy::unsafe_derive_deserialize)]
        pub struct Settings {
            $(
                $(#[$field_meta])*
                #[pyo3(get, set)]
                pub $field: $ty,
            )*
        }

        impl Default for Settings {
            fn default() -> Self {
                Self {
                    $($field: $default),*
                }
            }
        }

        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        #[pyclass]
        #[allow(clippy::unsafe_derive_deserialize)]
        pub struct SettingsOverlay {
            $(
                $(#[$field_meta])*
                #[serde(skip_serializing_if = "Option::is_none")]
                #[pyo3(get, set)]
                pub $field: Option<$ty>,
            )*
        }

        impl SettingsOverlay {
            /// Merge this overlay onto base settings, returning resolved values.
            pub fn merge(&self, mut base: Settings) -> Settings {
                $(
                    if let Some(ref value) = self.$field {
                        // Use MergeField trait - HashMap merges keys, others replace
                        base.$field.merge_from(value);
                    }
                )*
                base
            }

            /// Check if this overlay is empty (all fields are None).
            pub fn is_empty(&self) -> bool {
                true $(&& self.$field.is_none())*
            }
        }
    };
}

settings! {
    /// Global settings with defaults that can be overridden per-MUD or per-character.
    struct Settings {
        /// Whether output should wrap at the buffer edge.
        word_wrap: bool = true,

        /// Separator for sending multiple commands in one line.
        send_separator: String = ";;".to_string(),

        /// Number of lines to scroll when using scroll shortcuts.
        scroll_lines: u16 = 5,

        /// Whether to show input echo in the output buffer.
        show_input_echo: bool = true,

        /// Percentage of screen to use for scrollback overlay.
        scrollback_percentage: u16 = 70,

        /// Whether to echo raw received GMCP messages as debug output
        gmcp_echo: bool = false,

        /// Free-form custom settings for use by Python scripts.
        /// Allows arbitrary string key-value pairs without requiring Rust struct changes.
        #[serde(default)]
        extras: HashMap<String, String> = HashMap::new(),
    }
}

/// Top level app configuration with character list and global settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[pyclass]
#[allow(clippy::unsafe_derive_deserialize)]
pub struct Config {
    /// Whether mouse support is enabled in the TUI.
    #[serde(default = "default::mouse_enabled")]
    #[pyo3(get, set)]
    pub mouse_enabled: bool,

    /// Named MUD definitions that can be referenced by characters.
    #[serde(default)]
    pub muds: PyMap<Mud>,

    /// Character definitions.
    #[serde(default)]
    pub characters: PyMap<Character>,

    /// Python modules to load at startup.
    ///
    /// The `async def setup(): ...` function in each module will be invoked
    /// as soon as the application starts.
    #[serde(default)]
    #[pyo3(get)]
    pub modules: Vec<String>,

    /// Global default settings.
    #[serde(
        default = "default::settings",
        serialize_with = "ser_py_settings",
        deserialize_with = "der_py_settings"
    )]
    #[pyo3(get)]
    settings: Py<Settings>,
}

impl Config {
    pub fn new() -> Result<Self, ConfigError> {
        Self::load(config_file())
    }

    /// Load configuration from a TOML file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::InvalidMud(format!("Failed to read config file: {e}")))?;
        let config: Self = toml::from_str(&content)?;
        Python::attach(|py| config.validate(py))?;
        Ok(config)
    }

    /// Replace the configuration with the other Config.
    pub fn replace_with(&mut self, config: Config) {
        *self = config;
    }

    /// Save configuration to a TOML file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        Python::attach(|py| self.validate(py))?;
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::InvalidMud(format!("Failed to serialize config: {e}")))?;
        fs::write(path.as_ref(), content)
            .map_err(|e| ConfigError::InvalidMud(format!("Failed to write config file: {e}")))?;
        Ok(())
    }

    /// Validate the configuration for consistency.
    fn validate(&self, py: Python<'_>) -> Result<(), ConfigError> {
        for (mud_name, mud) in self.muds.iter(py) {
            if mud_name.trim().is_empty() {
                return Err(ConfigError::InvalidMud(
                    "MUD name cannot be empty".to_string(),
                ));
            }

            let mud = mud.borrow(py);
            if mud.host.is_empty() {
                return Err(ConfigError::InvalidMud(format!(
                    "MUD {mud_name:?} has empty host"
                )));
            }

            if matches!(mud.tls, Tls::Enabled) {
                ServerName::try_from(mud.host.as_str()).map_err(|e| {
                    ConfigError::InvalidMud(format!(
                        "MUD {mud_name:?} hostname {hostname:?} invalid for TLS: {e}",
                        hostname = mud.host
                    ))
                })?;
            }
        }

        for (char_name, character) in self.characters.iter(py) {
            if char_name.trim().is_empty() {
                return Err(ConfigError::InvalidCharacter(
                    "character name cannot be empty".to_string(),
                ));
            }

            let character = character.borrow(py);
            if character.mud.is_empty() {
                return Err(ConfigError::InvalidCharacter(
                    "MUD name cannot be empty".to_string(),
                ));
            }

            if !self.muds.contains_key(py, &character.mud) {
                return Err(ConfigError::InvalidCharacter(format!(
                    "character {char_name:?} references unknown MUD {mud_name:?}",
                    mud_name = character.mud
                )));
            }
        }

        Ok(())
    }
}

#[pymethods]
impl Config {
    #[getter(muds)]
    fn get_muds(&self) -> &Py<PyDict> {
        &self.muds.dict
    }

    #[getter(characters)]
    fn get_characters(&self) -> &Py<PyDict> {
        &self.characters.dict
    }

    /// Get a MUD definition by name.
    pub fn mud(&self, py: Python<'_>, name: &str) -> Option<Py<Mud>> {
        self.muds.get(py, name)
    }

    /// Get a character by name.
    pub fn character(&self, py: Python<'_>, name: &str) -> Option<Py<Character>> {
        self.characters.get(py, name)
    }

    /// Resolve all settings for the character name provided.
    ///
    /// This returns a `Settings` instance with the override hierarchy applied:
    /// Character settings > MUD settings > Global settings.
    pub fn resolve_settings(&self, py: Python<'_>, char_name: &str) -> Result<Settings, Error> {
        let char_def = self.character(py, char_name).ok_or_else(|| {
            ErrorKind::from(ConfigError::InvalidCharacter(format!(
                "unknown character name {char_name:?}"
            )))
        })?;
        let char_def = char_def.borrow(py);

        let mud_def = self.mud(py, &char_def.mud).ok_or_else(|| {
            ErrorKind::from(ConfigError::InvalidCharacter(format!(
                "character {char_name:?} references unknown MUD {mud_name:?}",
                mud_name = &char_def.mud
            )))
        })?;
        let mud_def = mud_def.borrow(py);

        // Build settings by applying overlays in order: global <- MUD <- character
        let settings = self.settings.borrow(py).clone();
        let settings = mud_def.settings.borrow(py).merge(settings);
        let settings = char_def.settings.borrow(py).merge(settings);

        Ok(settings)
    }

    /// Resolve a single custom setting by key for the character name provided.
    ///
    /// This applies the same override hierarchy as `resolve_settings`:
    /// Character settings > MUD settings > Global settings.
    ///
    /// If the setting key is not found in any level, returns the provided default.
    #[pyo3(signature = (char_name, key, default = None))]
    pub fn resolve_setting(
        &self,
        py: Python<'_>,
        char_name: &str,
        key: &str,
        default: Option<String>,
    ) -> Result<Option<String>, Error> {
        Ok(self
            .resolve_settings(py, char_name)?
            .extras
            .get(key)
            .cloned()
            .or(default))
    }
}

impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            mouse_enabled,
            muds,
            characters,
            modules,
            settings,
        } = self;

        let Ok(muds) = Python::attach(|py| muds.to_hashmap(py)) else {
            return false;
        };

        let Ok(other_muds) = Python::attach(|py| other.muds.to_hashmap(py)) else {
            return false;
        };

        let Ok(characters) = Python::attach(|py| characters.to_hashmap(py)) else {
            return false;
        };

        let Ok(other_characters) = Python::attach(|py| other.characters.to_hashmap(py)) else {
            return false;
        };

        let settings_equal =
            Python::attach(|py| *settings.borrow(py) == *other.settings.borrow(py));

        *mouse_enabled == other.mouse_enabled
            && muds == other_muds
            && characters == other_characters
            && modules == &other.modules
            && settings_equal
    }
}

/// MUD server configuration with connection details and optional setting overrides.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[pyclass]
#[allow(clippy::unsafe_derive_deserialize)]
pub struct Mud {
    /// Hostname to connect to.
    ///
    /// The host can be specified as an IP address, or a domain name.
    #[pyo3(get, set)]
    pub host: String,

    /// Port to connect to.
    #[pyo3(get, set)]
    pub port: u16,

    /// Whether to use transport layer security (TLS).
    #[serde(default)]
    #[pyo3(get, set)]
    pub tls: Tls,

    /// Whether to disable TCP keep alive.
    ///
    /// Since Telnet offers no protocol keepalive mechanism with wide deployment
    /// it's advantageous to use a transport layer keepalive. This can be disabled
    /// if necessary, but without regular bidirectional traffic or a keepalive MUD
    /// connections may be closed unexpectedly.
    #[serde(default)]
    #[pyo3(get, set)]
    pub no_tcp_keepalive: bool,

    /// MUD-specific setting overrides.
    #[serde(
        default = "default::settings_overlay",
        skip_serializing_if = "is_settings_overlay_empty",
        serialize_with = "ser_py_settings_overlay",
        deserialize_with = "der_py_settings_overlay"
    )]
    #[pyo3(get)]
    settings: Py<SettingsOverlay>,
}

#[pymethods]
impl Mud {
    #[pyo3(signature = (host, port, tls = None))]
    #[new]
    fn new(host: String, port: u16, tls: Option<Tls>) -> PyResult<Self> {
        Python::attach(|py| {
            Ok(Self {
                host,
                port,
                tls: tls.unwrap_or_default(),
                no_tcp_keepalive: false,
                settings: Py::new(py, SettingsOverlay::default())?,
            })
        })
    }
}

impl PartialEq for Mud {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            host,
            port,
            tls,
            no_tcp_keepalive,
            settings,
        } = self;

        let settings_equal =
            Python::attach(|py| *settings.borrow(py) == *other.settings.borrow(py));

        host == &other.host
            && port == &other.port
            && tls == &other.tls
            && no_tcp_keepalive == &other.no_tcp_keepalive
            && settings_equal
    }
}

/// Possible TLS states for a `MUD`.
#[derive(
    Debug, Clone, Copy, Default, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize,
)]
#[pyclass(frozen, eq, eq_int, hash)]
#[allow(clippy::unsafe_derive_deserialize)] // No constructor invariants to uphold.
pub enum Tls {
    #[default]
    Disabled,
    Enabled,
    InsecureSkipVerify,
}

/// Character definition with MUD reference and optional setting overrides.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[pyclass]
#[allow(clippy::unsafe_derive_deserialize)]
pub struct Character {
    /// Reference to a MUD definition by name.
    #[pyo3(get, set)]
    pub mud: String,

    /// Python module to load at session creation time.
    ///
    /// The `setup` function in your module will be called with the `Session`
    /// that was created. This is a great place to initialize/invoke your own
    /// code to add triggers, override settings, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[pyo3(get, set)]
    pub module: Option<String>,

    /// Character-specific setting overrides.
    #[serde(
        default = "default::settings_overlay",
        skip_serializing_if = "is_settings_overlay_empty",
        serialize_with = "ser_py_settings_overlay",
        deserialize_with = "der_py_settings_overlay"
    )]
    #[pyo3(get)]
    pub settings: Py<SettingsOverlay>,
}

#[pymethods]
impl Character {
    #[pyo3(signature = (mud, module = None))]
    #[new]
    fn new(mud: String, module: Option<String>) -> PyResult<Self> {
        Python::attach(|py| {
            Ok(Self {
                mud,
                module,
                settings: Py::new(py, SettingsOverlay::default())?,
            })
        })
    }
}

impl PartialEq for Character {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            mud,
            module,
            settings,
        } = self;

        let settings_equal =
            Python::attach(|py| *settings.borrow(py) == *other.settings.borrow(py));

        mud == &other.mud && module == &other.module && settings_equal
    }
}

#[derive(Clone)]
pub struct PyMap<T> {
    dict: Py<PyDict>,
    _phantom: PhantomData<T>,
}

impl<'py, T> PyMap<T>
where
    Py<T>: FromPyObjectOwned<'py>,
    T: Clone + PyClass + Into<PyClassInitializer<T>> + ExtractPyClassWithClone,
{
    fn new() -> Self {
        Python::attach(|py| Self {
            dict: PyDict::new(py).into(),
            _phantom: PhantomData,
        })
    }

    pub fn get(&self, py: Python<'py>, key: &str) -> Option<Py<T>> {
        self.dict.bind(py).get_item(key).ok()??.extract().ok()
    }

    #[cfg(test)]
    fn insert(&self, py: Python<'_>, key: &str, value: T) -> PyResult<()> {
        self.dict.bind(py).set_item(key, Py::new(py, value)?)
    }

    pub fn contains_key(&self, py: Python<'_>, key: &str) -> bool {
        self.dict.bind(py).contains(key).unwrap_or(false)
    }

    pub fn iter(&self, py: Python<'py>) -> impl Iterator<Item = (String, Py<T>)> + 'py {
        self.dict
            .bind(py)
            .iter()
            .filter_map(|(k, v)| Some((k.extract().ok()?, v.extract().ok()?)))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn to_hashmap(&self, py: Python<'_>) -> PyResult<HashMap<String, T>> {
        let mut map = HashMap::new();

        for (key, value) in self.dict.bind(py).iter() {
            map.insert(key.extract()?, value.extract()?);
        }

        Ok(map)
    }
}

impl<'py, T> Default for PyMap<T>
where
    Py<T>: FromPyObjectOwned<'py>,
    T: Clone + PyClass + Into<PyClassInitializer<T>> + ExtractPyClassWithClone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'py, T> Serialize for PyMap<T>
where
    Py<T>: FromPyObjectOwned<'py>,
    T: Clone + Serialize + PyClass + Into<PyClassInitializer<T>> + ExtractPyClassWithClone,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Python::attach(|py| {
            self.to_hashmap(py)
                .map_err(|e| serde::ser::Error::custom(e.to_string()))?
                .serialize(serializer)
        })
    }
}

impl<'py, 'de, T> Deserialize<'de> for PyMap<T>
where
    Py<T>: FromPyObjectOwned<'py>,
    T: Clone + Deserialize<'de> + PyClass + Into<PyClassInitializer<T>> + ExtractPyClassWithClone,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map: HashMap<String, T> = HashMap::deserialize(deserializer)?;

        Python::attach(|py| {
            let dict = PyDict::new(py);

            for (key, value) in map {
                let py_obj = Py::new(py, value).map_err(|e| {
                    serde::de::Error::custom(format!("failed to create object: {e}"))
                })?;
                dict.set_item(key, py_obj).map_err(|e| {
                    serde::de::Error::custom(format!("failed to insert object into dict: {e}"))
                })?;
            }

            Ok(Self {
                dict: dict.into(),
                _phantom: PhantomData,
            })
        })
    }
}

impl<'py, T> Debug for PyMap<T>
where
    Py<T>: FromPyObjectOwned<'py>,
    T: Clone + Debug + PyClass + Into<PyClassInitializer<T>> + ExtractPyClassWithClone,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Python::attach(|py| write!(f, "PyMudMap({:?})", self.dict.bind(py)))
    }
}

// Trait for merging overlay values onto base values.
trait MergeField {
    fn merge_from(&mut self, overlay: &Self);
}

// Implementations for primitive/simple types: replace base with overlay
impl MergeField for bool {
    fn merge_from(&mut self, overlay: &Self) {
        *self = *overlay;
    }
}

impl MergeField for String {
    fn merge_from(&mut self, overlay: &Self) {
        self.clone_from(overlay);
    }
}

impl MergeField for u16 {
    fn merge_from(&mut self, overlay: &Self) {
        *self = *overlay;
    }
}

// Specialized implementation for HashMap: merge keys with overlay taking precedence
impl MergeField for HashMap<String, String> {
    fn merge_from(&mut self, overlay: &Self) {
        for (key, value) in overlay {
            self.insert(key.clone(), value.clone());
        }
    }
}

fn ser_py_settings<S>(settings: &Py<Settings>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    Python::attach(|py| (*settings.borrow(py)).serialize(serializer))
}

fn der_py_settings<'de, D>(deserializer: D) -> Result<Py<Settings>, D::Error>
where
    D: Deserializer<'de>,
{
    let settings = Settings::deserialize(deserializer)?;
    Python::attach(|py| Py::new(py, settings).map_err(|e| serde::de::Error::custom(e.to_string())))
}

fn ser_py_settings_overlay<S>(
    settings: &Py<SettingsOverlay>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    Python::attach(|py| (*settings.borrow(py)).serialize(serializer))
}

fn der_py_settings_overlay<'de, D>(deserializer: D) -> Result<Py<SettingsOverlay>, D::Error>
where
    D: Deserializer<'de>,
{
    let settings = SettingsOverlay::deserialize(deserializer)?;
    Python::attach(|py| Py::new(py, settings).map_err(|e| serde::de::Error::custom(e.to_string())))
}

fn is_settings_overlay_empty(settings: &Py<SettingsOverlay>) -> bool {
    Python::attach(|py| settings.borrow(py).is_empty())
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

pub static GIT_COMMIT_HASH: &str = env!("MUDPUPPY_GIT_INFO");

// 🤷 https://github.com/serde-rs/serde/issues/368
mod default {
    use pyo3::{Py, Python};

    use super::{Settings, SettingsOverlay};

    pub(super) fn mouse_enabled() -> bool {
        true
    }

    pub(super) fn settings() -> Py<Settings> {
        Python::attach(|py| Py::new(py, Settings::default()).unwrap())
    }

    pub(super) fn settings_overlay() -> Py<SettingsOverlay> {
        Python::attach(|py| Py::new(py, SettingsOverlay::default()).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use pyo3::types::PyModule;
    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_settings_defaults() {
        let settings = Settings::default();
        assert!(settings.word_wrap);
        assert_eq!(settings.send_separator, ";;");
        assert_eq!(settings.scroll_lines, 5);
        assert!(settings.show_input_echo);
        assert_eq!(settings.scrollback_percentage, 70);
    }

    #[test]
    fn test_settings_overlay_merge() {
        let overlay = SettingsOverlay {
            word_wrap: Some(false),
            scroll_lines: Some(10),
            ..Default::default()
        };

        let merged = overlay.merge(Settings::default());
        assert!(!merged.word_wrap); // From overlay
        assert_eq!(merged.scroll_lines, 10); // From overlay
        assert_eq!(merged.send_separator, ";;"); // From base
    }

    #[test]
    fn test_settings_overlay_is_empty() {
        let empty = SettingsOverlay::default();
        assert!(empty.is_empty());

        let not_empty = SettingsOverlay {
            word_wrap: Some(false),
            ..Default::default()
        };
        assert!(!not_empty.is_empty());
    }

    #[test]
    fn test_settings_extras_default() {
        assert!(Settings::default().extras.is_empty());
    }

    #[test]
    fn test_settings_extras_merge() {
        let mut base = Settings::default();
        base.extras
            .insert("key1".to_string(), "base_value".to_string());
        base.extras
            .insert("key2".to_string(), "base_value2".to_string());

        let mut overlay_extras = HashMap::new();
        overlay_extras.insert("key2".to_string(), "overlay_value2".to_string());
        overlay_extras.insert("key3".to_string(), "overlay_value3".to_string());

        let overlay = SettingsOverlay {
            extras: Some(overlay_extras),
            ..Default::default()
        };

        let merged = overlay.merge(base);

        // key1 from base should remain
        assert_eq!(merged.extras.get("key1"), Some(&"base_value".to_string()));
        // key2 from overlay should override base
        assert_eq!(
            merged.extras.get("key2"),
            Some(&"overlay_value2".to_string())
        );
        // key3 from overlay should be added
        assert_eq!(
            merged.extras.get("key3"),
            Some(&"overlay_value3".to_string())
        );
    }

    #[test]
    fn test_config_load_save_roundtrip() {
        Python::initialize();
        let config = test_config();

        let tmpfile = NamedTempFile::new().unwrap();
        config.save(tmpfile.path()).unwrap();

        let loaded = Config::load(tmpfile.path()).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn test_config_validation_unknown_mud() {
        Python::initialize();
        let config = test_config();
        Python::attach(|py| {
            let char = config.characters.get(py, TEST_CHAR_NAME).unwrap();
            let mut char = char.borrow_mut(py);
            char.mud = "ImaginaryMUD".to_string();
        });

        Python::attach(|py| {
            let err = config.validate(py).unwrap_err();
            assert!(matches!(err, ConfigError::InvalidCharacter(_)));
            assert!(err.to_string().contains("unknown MUD"));
        });
    }

    #[test]
    fn test_character_resolve_settings_hierarchy() {
        Python::initialize();
        let config = test_config();

        // Global settings (lowest priority)
        Python::attach(|py| {
            let mut settings = config.settings.borrow_mut(py);
            settings.show_input_echo = true;
            settings.scroll_lines = 5;
            settings.word_wrap = true;
        });

        // MUD settings (overriding global settings)
        Python::attach(|py| {
            let mud = config.muds.get(py, TEST_MUD_NAME).unwrap();
            let mud = mud.borrow(py);
            let mut settings = mud.settings.borrow_mut(py);
            settings.show_input_echo = Some(false);
            settings.scroll_lines = Some(10);
        });

        // Character setting (overriding a global setting)
        Python::attach(|py| {
            let char = config.characters.get(py, TEST_CHAR_NAME).unwrap();
            let char = char.borrow(py);
            let mut settings = char.settings.borrow_mut(py);
            settings.word_wrap = Some(false);
            settings.scroll_lines = Some(11);
        });

        Python::attach(|py| {
            let resolved = config.resolve_settings(py, TEST_CHAR_NAME).unwrap();
            assert!(!resolved.show_input_echo); // From MUD override
            assert_eq!(resolved.scroll_lines, 11); // From character override.
            assert!(!resolved.word_wrap); // From character override
            assert_eq!(resolved.send_separator, ";;"); // From global default
        });
    }

    #[test]
    fn test_resolve_setting_hierarchy() {
        Python::initialize();
        let config = test_config();

        // Set up extras at different levels
        Python::attach(|py| {
            // Global setting
            let mut settings = config.settings.borrow_mut(py);
            settings
                .extras
                .insert("global_key".to_string(), "global_value".to_string());
            settings
                .extras
                .insert("overridden_key".to_string(), "global_value".to_string());
        });

        // MUD-level setting (overrides global)
        Python::attach(|py| {
            let mud = config.muds.get(py, TEST_MUD_NAME).unwrap();
            let mud = mud.borrow(py);
            let mut settings = mud.settings.borrow_mut(py);
            let mut extras = HashMap::new();
            extras.insert("mud_key".to_string(), "mud_value".to_string());
            extras.insert("overridden_key".to_string(), "mud_value".to_string());
            settings.extras = Some(extras);
        });

        // Character-level setting (overrides MUD and global)
        Python::attach(|py| {
            let char = config.characters.get(py, TEST_CHAR_NAME).unwrap();
            let char = char.borrow(py);
            let mut settings = char.settings.borrow_mut(py);
            let mut extras = HashMap::new();
            extras.insert("char_key".to_string(), "char_value".to_string());
            extras.insert("overridden_key".to_string(), "char_value".to_string());
            settings.extras = Some(extras);
        });

        Python::attach(|py| {
            // Test global setting
            assert_eq!(
                config
                    .resolve_setting(py, TEST_CHAR_NAME, "global_key", None)
                    .unwrap(),
                Some("global_value".to_string())
            );

            // Test MUD setting
            assert_eq!(
                config
                    .resolve_setting(py, TEST_CHAR_NAME, "mud_key", None)
                    .unwrap(),
                Some("mud_value".to_string())
            );

            // Test character setting
            assert_eq!(
                config
                    .resolve_setting(py, TEST_CHAR_NAME, "char_key", None)
                    .unwrap(),
                Some("char_value".to_string())
            );

            // Test override hierarchy (character > MUD > global)
            assert_eq!(
                config
                    .resolve_setting(py, TEST_CHAR_NAME, "overridden_key", None)
                    .unwrap(),
                Some("char_value".to_string())
            );

            // Test non-existent key with default
            assert_eq!(
                config
                    .resolve_setting(
                        py,
                        TEST_CHAR_NAME,
                        "nonexistent",
                        Some("default_value".to_string())
                    )
                    .unwrap(),
                Some("default_value".to_string())
            );

            // Test non-existent key without default
            assert_eq!(
                config
                    .resolve_setting(py, TEST_CHAR_NAME, "nonexistent", None)
                    .unwrap(),
                None
            );
        });
    }

    #[test]
    fn test_config_helper_methods() {
        Python::initialize();
        let config = test_config();

        Python::attach(|py| {
            // Test get_mud
            assert!(config.mud(py, TEST_MUD_NAME).is_some());
            assert!(config.mud(py, "NonExistent").is_none());

            // Test get_character
            assert!(config.character(py, TEST_CHAR_NAME).is_some());
            assert!(config.character(py, "NonExistent").is_none());
            let char = config.character(py, TEST_CHAR_NAME).unwrap();
            assert_eq!(char.borrow(py).mud, TEST_MUD_NAME);
        });
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_python_user_experience() {
        Python::initialize();
        let config = test_config();

        Python::attach(|py| {
            // Set some initial values in Rust
            {
                let mut global_settings = config.settings.borrow_mut(py);
                global_settings.word_wrap = true;
                global_settings.scroll_lines = 5;
                global_settings.send_separator = ";;".to_string();
            }

            let code = c"
def test_settings(config):
    # Read global settings
    assert config.settings.word_wrap == True
    assert config.settings.scroll_lines == 5
    assert config.settings.send_separator == ';;'

    # Modify global settings
    config.settings.word_wrap = False
    config.settings.scroll_lines = 10

    # Access MUD and modify its settings
    mud = config.muds['TestMUD']
    assert mud.host == 'test.mud.com'
    assert mud.port == 4000

    # Modify MUD-specific settings
    mud.settings.show_input_echo = False
    mud.settings.scroll_lines = 15

    # Access character and modify its settings
    char = config.characters['TestChar']
    assert char.mud == 'TestMUD'

    # Modify character-specific settings
    char.settings.word_wrap = True
    char.settings.scrollback_percentage = 80

    # Test hierarchy resolution from Python
    resolved = config.resolve_settings('TestChar')

    # word_wrap should be True (from character override)
    assert resolved.word_wrap == True

    # scroll_lines should be 15 (from MUD override)
    assert resolved.scroll_lines == 15

    # show_input_echo should be False (from MUD override)
    assert resolved.show_input_echo == False

    # scrollback_percentage should be 80 (from character override)
    assert resolved.scrollback_percentage == 80

    # send_separator should be ';;' (from global default, no overrides)
    assert resolved.send_separator == ';;'

    return 'success'
";

            let module = PyModule::from_code(py, code, c"", c"").unwrap();
            let result: String = module
                .getattr("test_settings")
                .unwrap()
                .call1((config.clone(),))
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(result, "success");

            // Verify Rust sees the Python modifications to global settings
            {
                let global_settings = config.settings.borrow(py);
                assert!(
                    !global_settings.word_wrap,
                    "Global word_wrap should be False"
                );
                assert_eq!(
                    global_settings.scroll_lines, 10,
                    "Global scroll_lines should be 10"
                );
            }

            // Verify Rust sees the Python modifications to MUD settings
            {
                let mud = config.muds.get(py, TEST_MUD_NAME).unwrap();
                let mud = mud.borrow(py);
                let mud_settings = mud.settings.borrow(py);
                assert_eq!(
                    mud_settings.show_input_echo,
                    Some(false),
                    "MUD show_input_echo should be Some(false)"
                );
                assert_eq!(
                    mud_settings.scroll_lines,
                    Some(15),
                    "MUD scroll_lines should be Some(15)"
                );
            }

            // Verify Rust sees the Python modifications to Character settings
            {
                let char = config.characters.get(py, TEST_CHAR_NAME).unwrap();
                let char = char.borrow(py);
                let char_settings = char.settings.borrow(py);
                assert_eq!(
                    char_settings.word_wrap,
                    Some(true),
                    "Character word_wrap should be Some(true)"
                );
                assert_eq!(
                    char_settings.scrollback_percentage,
                    Some(80),
                    "Character scrollback_percentage should be Some(80)"
                );
            }
            // Test hierarchy resolution
            let resolved = config.resolve_settings(py, TEST_CHAR_NAME).unwrap();
            assert!(
                resolved.word_wrap,
                "Resolved word_wrap should be True (from character override)"
            );
            assert_eq!(
                resolved.scroll_lines, 15,
                "Resolved scroll_lines should be 15 (from MUD override)"
            );
            assert!(
                !resolved.show_input_echo,
                "Resolved show_input_echo should be False (from MUD override)"
            );
            assert_eq!(
                resolved.scrollback_percentage, 80,
                "Resolved scrollback_percentage should be 80 (from character override)"
            );
            assert_eq!(
                resolved.send_separator, ";;",
                "Resolved send_separator should be ';;' (from global default)"
            );
        });
    }

    #[test]
    fn test_toml_parsing_with_overrides() {
        Python::initialize();
        let toml = r#"
            [settings]
            word_wrap = false
            scroll_lines = 8

            [muds.DuneMUD]
            host = "dunemud.net"
            port = 6789

            [muds.DuneMUD.settings]
            send_separator = ";"
            scroll_lines = 10

            [characters.Warrior]
            mud = "DuneMUD"

            [characters.Mage]
            mud = "DuneMUD"

            [characters.Mage.settings]
            scroll_lines = 15
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        Python::attach(|py| config.validate(py).unwrap());

        Python::attach(|py| {
            let settings = config.settings.borrow(py);
            assert!(!settings.word_wrap);
            assert_eq!(settings.scroll_lines, 8);
        });

        Python::attach(|py| {
            let mud = config.mud(py, "DuneMUD").unwrap();
            let mud = mud.borrow(py);
            assert_eq!(mud.host, "dunemud.net");
            assert_eq!(mud.port, 6789);

            let warrior = config.character(py, "Warrior").unwrap();
            assert_eq!(warrior.borrow(py).mud, "DuneMUD");
            let warrior_settings = config.resolve_settings(py, "Warrior").unwrap();
            assert_eq!(warrior_settings.scroll_lines, 10); // From MUD
            assert_eq!(warrior_settings.send_separator, ";"); // From MUD
            assert!(!warrior_settings.word_wrap); // From global

            let mage = config.character(py, "Mage").unwrap();
            assert_eq!(mage.borrow(py).mud, "DuneMUD");
            let mage_settings = config.resolve_settings(py, "Mage").unwrap();
            assert_eq!(mage_settings.scroll_lines, 15); // From character override
            assert_eq!(mage_settings.send_separator, ";"); // From MUD
        });
    }

    #[test]
    fn test_toml_parsing_with_extras() {
        Python::initialize();
        let toml = r#"
            [settings]
            word_wrap = false

            [settings.extras]
            history_next = "down"
            history_prev = "up"

            [muds.TestMUD]
            host = "test.mud.com"
            port = 4000

            [muds.TestMUD.settings.extras]
            gmcp_window_resize_up = "Alt-j"
            gmcp_window_resize_down = "Alt-k"

            [characters.TestChar]
            mud = "TestMUD"

            [characters.TestChar.settings.extras]
            history_next = "Ctrl-n"
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        Python::attach(|py| config.validate(py).unwrap());

        Python::attach(|py| {
            // Test global extras
            let settings = config.settings.borrow(py);
            assert_eq!(
                settings.extras.get("history_next"),
                Some(&"down".to_string())
            );
            assert_eq!(settings.extras.get("history_prev"), Some(&"up".to_string()));

            // Test resolve_setting with hierarchy
            // history_prev should come from global (not overridden)
            assert_eq!(
                config
                    .resolve_setting(py, "TestChar", "history_prev", None)
                    .unwrap(),
                Some("up".to_string())
            );

            // history_next should come from character override
            assert_eq!(
                config
                    .resolve_setting(py, "TestChar", "history_next", None)
                    .unwrap(),
                Some("Ctrl-n".to_string())
            );

            // GMCP settings should come from MUD
            assert_eq!(
                config
                    .resolve_setting(py, "TestChar", "gmcp_window_resize_up", None)
                    .unwrap(),
                Some("Alt-j".to_string())
            );
            assert_eq!(
                config
                    .resolve_setting(py, "TestChar", "gmcp_window_resize_down", None)
                    .unwrap(),
                Some("Alt-k".to_string())
            );

            // Non-existent setting with default
            assert_eq!(
                config
                    .resolve_setting(py, "TestChar", "nonexistent", Some("default".to_string()))
                    .unwrap(),
                Some("default".to_string())
            );
        });
    }

    fn test_config() -> Config {
        let muds = PyMap::new();
        let characters = PyMap::new();
        Python::attach(|py| {
            muds.insert(py, TEST_MUD_NAME, test_mud("test.mud.com", 4000))
                .unwrap();
            characters
                .insert(py, TEST_CHAR_NAME, test_character(TEST_MUD_NAME))
                .unwrap();
        });

        Python::attach(|py| Config {
            mouse_enabled: true,
            muds,
            characters,
            modules: vec![],
            settings: Py::new(py, Settings::default()).unwrap(),
        })
    }

    fn test_mud(host: &str, port: u16) -> Mud {
        Python::attach(|py| Mud {
            host: host.to_string(),
            port,
            tls: Tls::Disabled,
            no_tcp_keepalive: false,
            settings: Py::new(py, SettingsOverlay::default()).unwrap(),
        })
    }

    fn test_character(mud: &str) -> Character {
        Python::attach(|py| Character {
            mud: mud.to_string(),
            module: None,
            settings: Py::new(py, SettingsOverlay::default()).unwrap(),
        })
    }

    static TEST_MUD_NAME: &str = "TestMUD";
    static TEST_CHAR_NAME: &str = "TestChar";
}
