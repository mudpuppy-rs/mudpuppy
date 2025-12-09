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

use std::fs;
use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio_rustls::rustls::pki_types::ServerName;

use crate::error::ConfigError;
use crate::session::Tls;

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
        #[serde(default)]
        pub struct Settings {
            $(
                $(#[$field_meta])*
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
        pub struct SettingsOverlay {
            $(
                $(#[$field_meta])*
                #[serde(skip_serializing_if = "Option::is_none")]
                pub $field: Option<$ty>,
            )*
        }

        impl SettingsOverlay {
            /// Merge this overlay onto base settings, returning resolved values.
            pub fn merge(&self, mut base: Settings) -> Settings {
                $(
                    if let Some(ref value) = self.$field {
                        base.$field = value.clone();
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

        /// Command separator for sending multiple commands in one line.
        command_separator: String = ";;".to_string(),

        /// Number of lines to scroll when using scroll shortcuts.
        scroll_lines: u16 = 5,

        /// Whether to show input echo in the output buffer.
        show_input_echo: bool = true,

        /// Percentage of screen to use for scrollback overlay.
        scrollback_percentage: u16 = 70,
    }
}

/// Top level app configuration with character list and global settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Whether mouse support is enabled in the TUI.
    #[serde(default = "default::mouse_enabled")]
    pub mouse_enabled: bool,

    /// Named MUD definitions that can be referenced by characters.
    #[serde(default)]
    pub muds: HashMap<String, Mud>,

    /// Character definitions.
    #[serde(default)]
    pub characters: HashMap<String, Character>,

    /// Python modules to load at startup.
    ///
    /// The `async def setup(): ...` function in each module will be invoked
    /// as soon as the application starts.
    #[serde(default)]
    pub modules: Vec<String>,

    /// Global default settings.
    #[serde(default)]
    settings: Settings,
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::InvalidMud(format!("Failed to read config file: {e}")))?;
        let config: Self = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Save configuration to a TOML file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        self.validate()?;
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::InvalidMud(format!("Failed to serialize config: {e}")))?;
        fs::write(path.as_ref(), content)
            .map_err(|e| ConfigError::InvalidMud(format!("Failed to write config file: {e}")))?;
        Ok(())
    }

    /// Get a MUD definition by name.
    pub fn mud(&self, name: &str) -> Option<&Mud> {
        self.muds.get(name)
    }

    /// Get a character by name.
    pub fn character(&self, name: &str) -> Option<&Character> {
        self.characters.get(name)
    }

    /// Resolve all settings for the character name provided.
    ///
    /// This returns a `Settings` instance with the override hierarchy applied:
    /// Character settings > MUD settings > Global settings.
    pub fn resolve_settings(&self, char_name: &str) -> Result<Settings, ConfigError> {
        let char_def = self.character(char_name).ok_or_else(|| {
            ConfigError::InvalidCharacter(format!("unknown character name {char_name:?}"))
        })?;

        let mud_def = self.mud(&char_def.mud).ok_or_else(|| {
            ConfigError::InvalidCharacter(format!(
                "character {char_name:?} references unknown MUD {mud_name:?}",
                mud_name = &char_def.mud
            ))
        })?;

        // Build settings by applying overlays in order: global <- MUD <- character
        let settings = self.settings.clone();
        let settings = mud_def.settings.merge(settings);
        let settings = char_def.settings.merge(settings);

        Ok(settings)
    }

    /// Validate the configuration for consistency.
    fn validate(&self) -> Result<(), ConfigError> {
        for (mud_name, mud) in &self.muds {
            if mud_name.trim().is_empty() {
                return Err(ConfigError::InvalidMud(
                    "MUD name cannot be empty".to_string(),
                ));
            }

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

        for (char_name, character) in &self.characters {
            if char_name.trim().is_empty() {
                return Err(ConfigError::InvalidCharacter(
                    "character name cannot be empty".to_string(),
                ));
            }

            if character.mud.is_empty() {
                return Err(ConfigError::InvalidCharacter(
                    "MUD name cannot be empty".to_string(),
                ));
            }

            if !self.muds.contains_key(&character.mud) {
                return Err(ConfigError::InvalidCharacter(format!(
                    "character {char_name:?} references unknown MUD {mud_name:?}",
                    mud_name = character.mud
                )));
            }
        }

        Ok(())
    }
}

/// MUD server configuration with connection details and optional setting overrides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mud {
    /// Hostname to connect to.
    ///
    /// The host can be specified as an IP address, or a domain name.
    pub host: String,

    /// Port to connect to.
    pub port: u16,

    /// Whether to use transport layer security (TLS).
    #[serde(default)]
    pub tls: Tls,

    /// Whether to disable TCP keep alive.
    ///
    /// Since Telnet offers no protocol keepalive mechanism with wide deployment
    /// it's advantageous to use a transport layer keepalive. This can be disabled
    /// if necessary, but without regular bidirectional traffic or a keepalive MUD
    /// connections may be closed unexpectedly.
    #[serde(default)]
    pub no_tcp_keepalive: bool,

    /// MUD-specific setting overrides.
    #[serde(default, skip_serializing_if = "SettingsOverlay::is_empty")]
    settings: SettingsOverlay,
}

/// Character definition with MUD reference and optional setting overrides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Character {
    /// Reference to a MUD definition by name.
    pub mud: String,

    /// Python module to load at session creation time.
    ///
    /// The `setup` function in your module will be called with the `Session`
    /// that was created. This is a great place to initialize/invoke your own
    /// code to add triggers, override settings, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,

    /// Character-specific setting overrides.
    #[serde(default, skip_serializing_if = "SettingsOverlay::is_empty")]
    settings: SettingsOverlay,
}

// 🤷 https://github.com/serde-rs/serde/issues/368
mod default {
    pub(super) fn mouse_enabled() -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_settings_defaults() {
        let settings = Settings::default();
        assert!(settings.word_wrap);
        assert_eq!(settings.command_separator, ";;");
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
        assert_eq!(merged.command_separator, ";;"); // From base
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
    fn test_config_load_save_roundtrip() {
        let config = test_config();

        let tmpfile = NamedTempFile::new().unwrap();
        config.save(tmpfile.path()).unwrap();

        let loaded = Config::load(tmpfile.path()).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn test_config_validation_unknown_mud() {
        let mut config = test_config();
        config.characters.get_mut(TEST_CHAR_NAME).unwrap().mud = "ImaginaryMUD".to_string();

        let err = config.validate().unwrap_err();
        assert!(matches!(err, ConfigError::InvalidCharacter(_)));
        assert!(err.to_string().contains("unknown MUD"))
    }

    #[test]
    fn test_character_resolve_settings_hierarchy() {
        let mut config = test_config();

        // Global settings (lowest priority)
        config.settings.show_input_echo = true;
        config.settings.scroll_lines = 5;
        config.settings.word_wrap = true;

        // MUD settings (overriding global settings)
        let mud = config.muds.get_mut(TEST_MUD_NAME).unwrap();
        mud.settings.show_input_echo = Some(false);
        mud.settings.scroll_lines = Some(10);

        // Character setting (overriding a global setting)
        let char = config.characters.get_mut(TEST_CHAR_NAME).unwrap();
        char.settings.word_wrap = Some(false);
        char.settings.scroll_lines = Some(11);

        let resolved = config.resolve_settings(TEST_CHAR_NAME).unwrap();
        assert_eq!(resolved.show_input_echo, false); // From MUD override
        assert_eq!(resolved.scroll_lines, 11); // From character override.
        assert!(!resolved.word_wrap); // From character override
        assert_eq!(resolved.command_separator, ";;"); // From global default
    }

    #[test]
    fn test_config_helper_methods() {
        let config = test_config();

        // Test get_mud
        assert!(config.mud(TEST_MUD_NAME).is_some());
        assert!(config.mud("NonExistent").is_none());

        // Test get_character
        assert!(config.character(TEST_CHAR_NAME).is_some());
        assert!(config.character("NonExistent").is_none());
        assert_eq!(config.character(TEST_CHAR_NAME).unwrap().mud, TEST_MUD_NAME);
    }

    #[test]
    fn test_toml_parsing_with_overrides() {
        let toml = r#"
            [settings]
            word_wrap = false
            scroll_lines = 8

            [muds.DuneMUD]
            host = "dunemud.net"
            port = 6789

            [muds.DuneMUD.settings]
            command_separator = ";"
            scroll_lines = 10

            [characters.Warrior]
            mud = "DuneMUD"

            [characters.Mage]
            mud = "DuneMUD"

            [characters.Mage.settings]
            scroll_lines = 15
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        config.validate().unwrap();

        assert!(!config.settings.word_wrap);
        assert_eq!(config.settings.scroll_lines, 8);

        let mud = config.mud("DuneMUD").unwrap();
        assert_eq!(mud.host, "dunemud.net");
        assert_eq!(mud.port, 6789);

        let warrior = config.character("Warrior").unwrap();
        assert_eq!(warrior.mud, "DuneMUD");
        let warrior_settings = config.resolve_settings("Warrior").unwrap();
        assert_eq!(warrior_settings.scroll_lines, 10); // From MUD
        assert_eq!(warrior_settings.command_separator, ";"); // From MUD
        assert!(!warrior_settings.word_wrap); // From global

        let mage = config.character("Mage").unwrap();
        assert_eq!(mage.mud, "DuneMUD");
        let mage_settings = config.resolve_settings("Mage").unwrap();
        assert_eq!(mage_settings.scroll_lines, 15); // From character override
        assert_eq!(mage_settings.command_separator, ";"); // From MUD
    }

    fn test_config() -> Config {
        Config {
            mouse_enabled: true,
            muds: [(TEST_MUD_NAME.to_string(), test_mud("test.mud.com", 4000))]
                .into_iter()
                .collect(),
            characters: [(TEST_CHAR_NAME.to_string(), test_character(TEST_MUD_NAME))]
                .into_iter()
                .collect(),
            modules: vec![],
            settings: Settings::default(),
        }
    }

    fn test_mud(host: &str, port: u16) -> Mud {
        Mud {
            host: host.to_string(),
            port,
            tls: Tls::Disabled,
            no_tcp_keepalive: false,
            settings: SettingsOverlay::default(),
        }
    }

    fn test_character(mud: &str) -> Character {
        Character {
            mud: mud.to_string(),
            module: None,
            settings: SettingsOverlay::default(),
        }
    }

    static TEST_MUD_NAME: &str = "TestMUD";
    static TEST_CHAR_NAME: &str = "TestChar";
}
