mod config_file;
mod keybindings;
mod logging;

pub use config_file::*;
pub use keybindings::*;
pub use logging::*;

use std::env;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use directories::ProjectDirs;

use crate::CRATE_NAME;
use crate::GIT_COMMIT_HASH;

#[must_use]
pub fn data_dir() -> &'static Path {
    static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
    lazy_overridable_dir(
        &format!("{}_DATA", CRATE_NAME.to_uppercase()),
        DirType::Data,
        &DATA_DIR,
    )
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
