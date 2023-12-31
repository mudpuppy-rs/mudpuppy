use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::OpenOptions;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock, RwLockReadGuard};
use std::{env, fs, panic, process};

use directories::ProjectDirs;
use pyo3::{pyclass, pymethods};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio_rustls::rustls::pki_types;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Value};
use tracing::{error, info, trace, warn};
use tracing_error::ErrorLayer;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

use crate::app::{restore_terminal, TabKind};
use crate::error::{ConfigError, Error};
use crate::model::{Mud, Shortcut, Tls};
use crate::{cli, CRATE_NAME};
use crate::{Result, GIT_COMMIT_HASH};

/// Set up logging to a log file in the data directory.
///
/// By default, no logging is done to STDOUT/STDERR - this would corrupt a TUI
/// application.
///
/// By default, only `INFO` level log lines and above are written to the log file,
/// and ANSI will be enabled. The log filter level can be adjusted using the
/// normal `RUST_LOG` environment variable semantics.
///
/// If the optional `console-subscriber` dependency is enabled the application
/// will be configured for `tokio-console`.
///
/// # Errors
///
/// If the data directory can't be created, or the log file can't be created,
/// or the `RUST_LOG` environment variable is invalid, this function will return
/// an error result.
pub fn init_logging(args: &cli::Args) -> Result<()> {
    let data_dir = data_dir();
    fs::create_dir_all(data_dir).map_err(|e| {
        Error::Config(ConfigError::Logging(format!(
            "creating data dir {:?}: {e}",
            data_dir.display()
        )))
    })?;
    let config_dir = config_dir();
    fs::create_dir_all(config_dir).map_err(|e| {
        Error::Config(ConfigError::Logging(format!(
            "creating config dir {:?}: {e}",
            config_dir.display()
        )))
    })?;

    let log_file = data_dir.join(format!("{CRATE_NAME}.log"));
    let log_file = fs::File::create(&log_file).map_err(|e| {
        Error::Config(ConfigError::Logging(format!(
            "creating log file {:?}: {e}",
            log_file.display()
        )))
    })?;
    let env_filter = EnvFilter::builder()
        .with_default_directive(args.log_level.into())
        .from_env()
        .map_err(|e| {
            Error::Config(ConfigError::Logging(format!(
                "invalid RUST_LOG env var filter config: {e}"
            )))
        })?;

    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(true)
        .with_filter(env_filter);

    let registry = tracing_subscriber::registry();
    #[cfg(feature = "console-subscriber")]
    let registry = registry.with(console_subscriber::spawn());

    registry
        .with(file_subscriber)
        .with(ErrorLayer::default())
        .init();

    Ok(())
}

pub fn init_panic_handler() {
    panic::set_hook(Box::new(move |panic_info| {
        if let Err(err) = restore_terminal() {
            error!("error restoring terminal: {}", err);
        }
        #[cfg(not(debug_assertions))]
        {
            use human_panic::{handle_dump, metadata, print_msg};
            let meta = metadata!();
            print_msg(handle_dump(&meta, panic_info), &meta)
                .expect("human-panic: printing error message to console failed");
        }
        #[cfg(debug_assertions)]
        {
            better_panic::Settings::auto()
                .most_recent_first(false)
                .lineno_suffix(true)
                .verbosity(better_panic::Verbosity::Full)
                .create_panic_handler()(panic_info);
        }
        error!("panic: {panic_info}");
        process::exit(1);
    }));
}

#[must_use]
pub fn data_dir() -> &'static Path {
    static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
    lazy_overridable_dir(&format!("{CRATE_NAME}_DATA"), DirType::Data, &DATA_DIR)
}

#[must_use]
#[allow(clippy::module_name_repetitions)]
pub fn config_dir() -> &'static Path {
    static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();
    lazy_overridable_dir(
        &format!("{CRATE_NAME}_CONFIG"),
        DirType::Config,
        &CONFIG_DIR,
    )
}

#[must_use]
#[allow(clippy::module_name_repetitions)]
pub fn version() -> &'static str {
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

pub fn project_directory() -> Option<&'static ProjectDirs> {
    static PROJECT_DIR: OnceLock<Option<ProjectDirs>> = OnceLock::new();
    PROJECT_DIR
        .get_or_init(|| {
            // TODO(XXX): register/use a project domain.
            ProjectDirs::from("ca.woodweb", CRATE_NAME, CRATE_NAME)
        })
        .as_ref()
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
            None => match (project_directory(), r#type) {
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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum DirType {
    Data,
    Config,
}

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
    pub fn new() -> Result<Self> {
        Ok(Self(Arc::new(RwLock::new(Config::new()?))))
    }

    /// Reload the configuration from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the new configuration is invalid.
    pub fn reload(&self) -> Result<(), Error> {
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
    pub fn must_lookup_mud(&self, mud_name: &str) -> Result<Mud> {
        self.lookup(
            |config| config.muds.iter().find(|m| m.name == mud_name).cloned(),
            None,
        )
        .ok_or(Error::Config(ConfigError::MissingMud(mud_name.to_string())))
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
    pub fn new() -> Result<Self> {
        let mut cfg = Self::default();
        cfg.load()?;
        Ok(cfg)
    }

    #[allow(clippy::missing_errors_doc, clippy::missing_panics_doc)] // TODO(XXX): doc
    pub fn load(&mut self) -> Result<(), ConfigError> {
        let default_config: Config = toml::from_str(CONFIG)?;
        let config_file = config_file();

        if !config_file.exists() {
            warn!("No configuration file found. Using defaults.");
        }

        let builder = config::Config::builder()
            // Safety: `set_default()` is documented to only panic if string conversion of the key fails.
            .set_default("_data_dir", data_dir().to_str().unwrap_or_default())
            .unwrap()
            .set_default("_config_dir", config_dir().to_str().unwrap_or_default())
            .unwrap()
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

#[derive(Debug, Clone, Default)]
#[pyclass]
pub struct KeyBindings(HashMap<String, HashMap<Vec<KeyEvent>, Shortcut>>);

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed_map = HashMap::<String, HashMap<String, Shortcut>>::deserialize(deserializer)?;

        let keybindings = parsed_map
            .into_iter()
            .map(|(input_mode, inner_map)| {
                let converted_inner_map = inner_map
                    .into_iter()
                    .map(|(key_str, cmd)| {
                        (
                            parse_key_sequence(&key_str).unwrap_or_else(|_| {
                                panic!("invalid config keyboard sequence: {key_str}")
                            }),
                            cmd,
                        )
                    })
                    .collect();
                (input_mode, converted_inner_map)
            })
            .collect();

        Ok(Self(keybindings))
    }
}

impl Serialize for KeyBindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        fn code_to_string(code: KeyCode) -> String {
            match code {
                KeyCode::Backspace => "backspace".to_string(),
                KeyCode::Enter => "enter".to_string(),
                KeyCode::Left => "left".to_string(),
                KeyCode::Right => "right".to_string(),
                KeyCode::Up => "up".to_string(),
                KeyCode::Down => "down".to_string(),
                KeyCode::Home => "home".to_string(),
                KeyCode::End => "end".to_string(),
                KeyCode::PageUp => "pageup".to_string(),
                KeyCode::PageDown => "pagedown".to_string(),
                KeyCode::Tab => "tab".to_string(),
                KeyCode::BackTab => "backtab".to_string(),
                KeyCode::Delete => "delete".to_string(),
                KeyCode::Insert => "insert".to_string(),
                KeyCode::F(code) => format!("f{code}"),
                KeyCode::Esc => "esc".to_string(),
                KeyCode::Char(' ') => "space".to_string(),
                KeyCode::Char('-') => "hyphen".to_string(),
                KeyCode::Char(c) => c.to_string(),
                _ => panic!("unknown key code: {code:?}"),
            }
        }

        fn key_to_string(event: KeyEvent) -> String {
            if event.modifiers.is_empty() {
                return code_to_string(event.code);
            }
            let mut key = code_to_string(event.code);
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                key = format!("ctrl-{key}");
            }
            if event.modifiers.contains(KeyModifiers::ALT) {
                key = format!("alt-{key}");
            }
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                key = format!("shift-{key}");
            }
            format!("<{key}>")
        }

        let mut raw_map = HashMap::<String, HashMap<String, Shortcut>>::default();

        for (tab, bindings) in &self.0 {
            let mut inner_map = HashMap::default();
            for (key, cmd) in bindings {
                inner_map.insert(
                    key.iter()
                        .map(|key| key_to_string(*key))
                        .collect::<Vec<_>>()
                        .join("><"),
                    cmd.clone(),
                );
            }
            raw_map.insert(tab.clone(), inner_map);
        }

        raw_map.serialize(serializer)
    }
}

fn parse_key_event(raw: &str) -> Result<KeyEvent, String> {
    parse_key_code_with_modifiers(extract_modifiers(&raw.to_ascii_lowercase()))
}

fn extract_modifiers(raw: &str) -> (&str, KeyModifiers) {
    let mut modifiers = KeyModifiers::empty();
    let mut current = raw;

    loop {
        if let Some(rest) = current.strip_prefix("ctrl-") {
            modifiers.insert(KeyModifiers::CONTROL);
            current = rest;
        } else if let Some(rest) = current.strip_prefix("alt-") {
            modifiers.insert(KeyModifiers::ALT);
            current = rest;
        } else if let Some(rest) = current.strip_prefix("shift-") {
            modifiers.insert(KeyModifiers::SHIFT);
            current = rest;
        } else {
            break;
        }
    }

    (current, modifiers)
}

fn parse_key_code_with_modifiers(
    (raw, mut modifiers): (&str, KeyModifiers),
) -> Result<KeyEvent, String> {
    #[allow(clippy::match_same_arms)]
    let c = match raw {
        "esc" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "backtab" => {
            modifiers.insert(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        "space" => KeyCode::Char(' '),
        "hyphen" => KeyCode::Char('-'),
        "minus" => KeyCode::Char('-'),
        "tab" => KeyCode::Tab,
        c if c.len() == 1 => {
            let mut c = c.chars().next().unwrap();
            if modifiers.contains(KeyModifiers::SHIFT) {
                c = c.to_ascii_uppercase();
            }
            KeyCode::Char(c)
        }
        _ => return Err(format!("Unable to parse {raw}")),
    };
    Ok(KeyEvent::new(c, modifiers))
}

fn parse_key_sequence(raw: &str) -> Result<Vec<KeyEvent>, String> {
    if raw.chars().filter(|c| *c == '>').count() != raw.chars().filter(|c| *c == '<').count() {
        return Err(format!("Unable to parse `{raw}`"));
    }
    let raw = if raw.contains("><") {
        raw
    } else {
        let raw = raw.strip_prefix('<').unwrap_or(raw);
        let raw = raw.strip_prefix('>').unwrap_or(raw);
        raw
    };
    let sequences = raw
        .split("><")
        .map(|seq| {
            if let Some(s) = seq.strip_prefix('<') {
                s
            } else if let Some(s) = seq.strip_suffix('>') {
                s
            } else {
                seq
            }
        })
        .collect::<Vec<_>>();

    sequences.into_iter().map(parse_key_event).collect()
}

/// Set the TOML config value under `key` to `v` for the MUD with the given `name`.
///
/// # Errors
///
/// Returns an error if config file can't be opened, read, written to, or if it contains
/// invalid TOML content.
pub fn edit_mud(name: &str, key: &str, v: impl Into<Value> + Debug) -> Result<()> {
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

const CONFIG: &str = include_str!("../../.config/config.toml");
