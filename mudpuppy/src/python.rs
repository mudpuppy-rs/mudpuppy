use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fs;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::sync::Arc;

use futures::stream::FuturesUnordered;
use pyo3::exceptions::PyTypeError;
use pyo3::ffi::c_str;
use pyo3::types::{
    PyAnyMethods, PyBool, PyBoolMethods, PyFunction, PyList, PyListMethods, PyModule,
    PyModuleMethods, PyStringMethods, PyTuple,
};
use pyo3::{
    pyclass, pymethods, pymodule, Bound, Py, PyAny, PyErr, PyObject, PyRef, PyResult, Python,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{watch, RwLock};
use tracing::{debug, error, info, instrument, trace, warn};

use crate::app::{State, UiState};
use crate::config::{config_dir, data_dir, GlobalConfig};
use crate::error::Error;
use crate::model::{
    Alias, AliasConfig, AliasId, InputLine, Mud, MudLine, PromptMode, PromptSignal, SessionId,
    SessionInfo, Shortcut, Timer, TimerConfig, TimerId, Tls, Trigger, TriggerConfig, TriggerId,
};
use crate::{client, net, tui, Result, CRATE_NAME, GIT_COMMIT_HASH};

/// Low level types and APIs for interacting with Mudpuppy.
///
/// For more convenient interfaces, prefer `mudpuppy` over `mudpuppy_core`.
// TODO(XXX): switch to declarative module reg.
#[allow(clippy::missing_errors_doc)]
#[pymodule]
pub fn mudpuppy_core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyApp>()?;
    m.add_class::<GlobalConfig>()?;
    m.add_class::<Shortcut>()?;
    m.add_class::<SessionInfo>()?;
    m.add_class::<SessionId>()?;
    m.add_class::<Mud>()?;
    m.add_class::<Tls>()?;
    m.add_class::<MudLine>()?;
    m.add_class::<InputLine>()?;
    m.add_class::<Event>()?;
    m.add_class::<EventType>()?;
    m.add_class::<EventHandlers>()?;
    m.add_class::<Trigger>()?;
    m.add_class::<TriggerConfig>()?;
    m.add_class::<TriggerId>()?;
    m.add_class::<Alias>()?;
    m.add_class::<AliasConfig>()?;
    m.add_class::<AliasId>()?;
    m.add_class::<TimerConfig>()?;
    m.add_class::<Timer>()?;
    m.add_class::<TimerId>()?;
    m.add_class::<PromptSignal>()?;
    m.add_class::<PromptMode>()?;
    m.add_class::<client::Status>()?;
    m.add_class::<net::stream::Info>()?;
    m.add_class::<client::output::Output>()?;
    m.add_class::<client::output::Item>()?;
    m.add_class::<client::input::Input>()?;
    m.add_class::<client::input::EchoState>()?;
    m.add_class::<tui::layout::LayoutNode>()?;
    m.add_class::<tui::layout::PyConstraint>()?;
    m.add_class::<tui::layout::PyDirection>()?;
    m.add_class::<tui::layout::BufferConfig>()?;
    m.add_class::<tui::layout::BufferDirection>()?;
    m.add_class::<tui::layout::BufferId>()?;
    m.add_class::<tui::layout::ExtraBuffer>()?;
    Ok(())
}

#[instrument(level = "trace", skip(py_app))]
pub fn init(py_app: PyApp) -> Result<(Py<EventHandlers>, Vec<PyObject>), Error> {
    // Bind a rust backend to the Python logging module.
    pyo3_pylogger::register(CRATE_NAME);

    let event_handlers = Python::with_gil(move |py| {
        // Set the Python logging level
        // TODO(XXX): use config to determine level.
        py.run(
            c_str!(
                r#"
import logging
logging.getLogger().setLevel(0)
        "#
            ),
            None,
            None,
        )?;

        debug!("adding {:?} to Python import path", config_dir());
        let syspath = py
            .import("sys")?
            .getattr("path")?
            .downcast_into::<PyList>()
            .map_err(|_| Error::Internal("getting Python syspath".to_string()))?;
        syspath.insert(0, config_dir())?;

        let module: Py<PyAny> = PyModule::import(py, "mudpuppy_core")?.into();
        module.setattr(py, "mudpuppy_core", py_app)?;
        let event_handlers = Py::new(py, EventHandlers::new())?;
        module.setattr(py, "event_handlers", event_handlers.clone())?;

        // Override print() built-in with one that will send output to the active MUD buffer.
        py.run(
            c_str!(
                r"
import builtins
import mudpuppy_core
builtins.print = mudpuppy_core.mudpuppy_core.print
        "
            ),
            None,
            None,
        )?;

        Ok::<_, Error>(event_handlers)
    })?;

    // Load built-in modules - we do this with a macro because we use read_file! to
    // source the module code.
    let builtin_modules: Vec<Py<PyAny>> = builtin_modules!(
        "mudpuppy",
        "cformat",
        "layout",
        "commands",
        "telnet_charset",
        "telnet_naws",
        "history",
        "cmd_misc",
        "cmd_py",
        "cmd_status",
        "cmd_alias",
        "cmd_trigger",
        "cmd_timer",
    );
    debug!("found {} built-in py modules", builtin_modules.len());

    let user_modules = user_modules()?;
    debug!("loaded {} user modules", user_modules.len());

    let all_modules: Vec<PyObject> = Python::with_gil(|_| {
        builtin_modules
            .into_iter()
            .chain(user_modules.clone())
            .collect()
    });
    debug!("loaded {} total modules", all_modules.len());

    Ok((event_handlers, user_modules))
}

/// # Errors
/// If reloading fails.
pub fn reload(user_modules: &[PyObject]) -> Result<()> {
    Python::with_gil(|py| {
        for module in user_modules {
            if module.getattr(py, "__reload__").is_ok() {
                module.call_method0(py, "__reload__")?;
            }
        }

        for module in user_modules {
            let importlib = PyModule::import(py, "importlib")?;
            importlib.call_method1("reload", (module,))?;
        }
        Ok(())
    })
}

pub type PyFuture = Pin<Box<dyn Future<Output = PyResult<PyObject>> + Send + 'static>>;

#[derive(Debug, Clone)]
#[pyclass(name = "MudpuppyCore")]
#[allow(clippy::module_name_repetitions)]
// TODO(XXX): calling the Rust type PyApp feels wrong. Come up with a better name.
pub struct PyApp {
    pub config: GlobalConfig,
    pub state: Arc<RwLock<State>>,
    pub waker: UnboundedSender<()>,
}

impl PyApp {
    fn toggle_trigger<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        trig_id: TriggerId,
        enabled: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        debug!("setting trigger {id} enabled: {enabled}");
        with_state!(self, py, |mut state| {
            match state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .triggers
                .get_mut(trig_id)
            {
                Some(trigger) => {
                    trigger.enabled = enabled;
                    Ok(trigger.enabled)
                }
                None => Err(Error::Trigger(trig_id.into()).into()),
            }
        })
    }

    fn toggle_alias<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        alias_id: AliasId,
        enabled: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        debug!("setting alias {id} enabled: {enabled}");
        with_state!(self, py, |mut state| {
            match state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .aliases
                .get_mut(alias_id)
            {
                Some(alias) => {
                    alias.enabled = enabled;
                    Ok(alias.enabled)
                }
                None => Err(Error::Alias(alias_id.into()).into()),
            }
        })
    }

    // Verify that a callback is an async coroutine function.
    fn require_coroutine(py: Python<'_>, name: &str, callback: &Py<PyAny>) -> PyResult<()> {
        // TODO(XXX): possible optimization - cache ref to this fn?
        let iscoroutinefunction = py
            .import("inspect")?
            .getattr("iscoroutinefunction")?
            .downcast_into::<PyFunction>()
            .map_err(|_| Error::Internal("getting inspect iscoroutinefunction".to_string()))?;

        let is_coroutine = iscoroutinefunction
            .call1((callback,))?
            .downcast::<PyBool>()?
            .is_true();

        match is_coroutine {
            true => Ok(()),
            false => Err(PyTypeError::new_err(format!(
                "{name} must be a coroutine function"
            ))),
        }
    }

    // Verify that a callback is **not** an async coroutine function, but a regular callable.
    fn require_callable(py: Python<'_>, name: &str, callback: &Py<PyAny>) -> PyResult<()> {
        if Self::require_coroutine(py, name, callback).is_ok() {
            return Err(PyTypeError::new_err(format!(
                "{name} must be a regular function, not a coroutine"
            )));
        }

        match callback.bind(py).is_callable() {
            true => Ok(()),
            false => Err(PyTypeError::new_err(format!(
                "{name} must be a callable function"
            ))),
        }
    }
}

#[pymethods]
impl PyApp {
    fn config(&self) -> GlobalConfig {
        self.config.clone()
    }

    /// Returns the path to the configuration directory.
    #[staticmethod]
    fn config_dir() -> String {
        config_dir().to_string_lossy().to_string()
    }

    #[staticmethod]
    fn data_dir() -> String {
        data_dir().to_string_lossy().to_string()
    }

    #[staticmethod]
    fn name() -> String {
        CRATE_NAME.to_string()
    }

    #[staticmethod]
    fn version() -> String {
        GIT_COMMIT_HASH.to_string()
    }

    #[pyo3(signature = (*args, sep=None, end=None))]
    fn print<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
        sep: Option<&str>,
        end: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Recreate the basics of print(), but writing to the output string.
        let sep = sep.unwrap_or(" ");
        let end = end.unwrap_or("\n");
        let mut output = String::new();
        for (i, arg) in args.try_iter()?.enumerate() {
            if i > 0 {
                output.push_str(sep);
            }
            let arg_str = arg?.str()?;
            let arg_str = arg_str.to_str()?;
            output.push_str(arg_str);
        }
        output.push_str(end);

        // Then convert each line of the output into a debug item.
        // TODO(XXX): Offer a way to pick the item type?
        let mut line_items = Vec::default();
        for line in output.lines() {
            line_items.push(client::output::Item::Debug {
                line: line.to_string(),
            });
        }

        // Finally, push the items to the active session's output.
        with_state!(self, py, |mut state| {
            let Some(cur_id) = state.active_session_id else {
                // TODO(XXX): err? log?
                return Ok(());
            };
            state
                .client_for_id_mut(cur_id)
                .unwrap()
                .output
                .extend(line_items.into_iter());
            Ok(())
        })
    }

    fn active_session_id<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| Ok(state.active_session_id))
    }

    fn sessions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| Ok(state.all_client_info()))
    }

    fn session_info<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Python::with_gil(|_| {
                Ok(state
                    .client_for_id(id)
                    .ok_or(Error::from(id))?
                    .info
                    .as_ref()
                    .clone())
            })
        })
    }

    fn status<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Ok(state.client_for_id(id).ok_or(Error::from(id))?.status())
        })
    }

    fn mud_config(&self, id: &SessionInfo) -> Option<Mud> {
        self.config.lookup_mud(&id.mud_name)
    }

    fn send_line<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        line: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .send_line(InputLine::new(line, true, true))
                .map_err(Into::into)
        })
    }

    fn send_lines<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        lines: Vec<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            for line in lines {
                state
                    .client_for_id_mut(id)
                    .ok_or(Error::from(id))?
                    .send_line(InputLine::new(line, true, true))?;
            }
            Ok(())
        })
    }

    fn connect<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .connect()
                .await
                .map_err(Into::into)
        })
    }

    fn disconnect<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .disconnect()
                .await
                .map_err(Into::into)
        })
    }

    fn request_enable_option<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        option: u8,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .request_enable_option(option)
                .map_err(Into::into)
        })
    }

    fn request_disable_option<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        option: u8,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .request_disable_option(option)
                .map_err(Into::into)
        })
    }

    fn send_subnegotiation<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        option: u8,
        data: Vec<u8>,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            state
                .client_for_id(id)
                .ok_or(Error::from(id))?
                .send_subnegotiation(option, data)
                .map_err(Into::into)
        })
    }

    fn new_trigger<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        config: Py<TriggerConfig>,
        module: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            let triggers = &mut state.client_for_id_mut(id).ok_or(Error::from(id))?.triggers;

            Python::with_gil(|pyy| {
                let new_config: PyRef<'_, TriggerConfig> = config.extract(pyy)?;
                for (_, t) in &mut *triggers {
                    let old_config: PyRef<'_, TriggerConfig> = t.config.extract(pyy)?;
                    if new_config.name == old_config.name {
                        warn!("trigger with name {} already exists", new_config.name);
                        return Ok::<_, PyErr>(None);
                    }
                }

                if let Some(callback) = &new_config.callback {
                    Self::require_coroutine(pyy, "trigger callback", callback)?;
                }

                if let Some(highlight) = &new_config.highlight {
                    Self::require_callable(pyy, "trigger highlight callback", highlight)?;
                }

                Ok(Some(triggers.construct(|id| Trigger {
                    id,
                    enabled: true,
                    module,
                    config,
                })))
            })
        })
    }

    fn get_trigger<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        trig_id: TriggerId,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Python::with_gil(|_| {
                Ok(state
                    .client_for_id(id)
                    .ok_or(Error::from(id))?
                    .triggers
                    .get(trig_id)
                    .cloned())
            })
        })
    }

    fn disable_trigger<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        trig_id: TriggerId,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.toggle_trigger(py, id, trig_id, false)
    }

    fn enable_trigger<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        trig_id: TriggerId,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.toggle_trigger(py, id, trig_id, true)
    }

    fn remove_trigger<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        trig_id: TriggerId,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .triggers
                .remove(trig_id);
            Ok(())
        })
    }

    fn remove_module_triggers<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        module: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            let triggers = &mut state.client_for_id_mut(id).ok_or(Error::from(id))?.triggers;
            let triggers_to_remove = triggers
                .iter()
                .filter_map(|(id, trigger)| {
                    if trigger.module == module {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            debug!(
                "removing {} triggers that were added by module {}",
                triggers_to_remove.len(),
                module
            );

            for trigger_id in triggers_to_remove {
                triggers.remove(trigger_id);
            }
            Ok(())
        })
    }

    fn triggers<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Python::with_gil(|_| {
                Ok(state
                    .client_for_id(id)
                    .ok_or(Error::from(id))?
                    .triggers
                    .iter()
                    .map(|(_, a)| a.clone())
                    .collect::<Vec<_>>())
            })
        })
    }

    fn new_alias<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        config: Py<AliasConfig>,
        module: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            let aliases = &mut state.client_for_id_mut(id).ok_or(Error::from(id))?.aliases;

            Python::with_gil(|pyy| {
                let new_config: PyRef<'_, AliasConfig> = config.extract(pyy)?;
                for (_, t) in &mut *aliases {
                    let old_config: PyRef<'_, AliasConfig> = t.config.extract(pyy)?;
                    if new_config.name == old_config.name {
                        warn!("alias with name {} already exists", new_config.name);
                        return Ok::<_, PyErr>(None);
                    }
                }

                if let Some(callback) = &new_config.callback {
                    Self::require_coroutine(pyy, "alias callback", callback)?;
                }

                Ok(Some(aliases.construct(|id| Alias {
                    id,
                    enabled: true,
                    config,
                    module,
                })))
            })
        })
    }

    fn get_alias<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        alias_id: AliasId,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Python::with_gil(|_| {
                Ok(state
                    .client_for_id(id)
                    .ok_or(Error::from(id))?
                    .aliases
                    .get(alias_id)
                    .cloned())
            })
        })
    }

    fn aliases<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Python::with_gil(|_| {
                Ok(state
                    .client_for_id(id)
                    .ok_or(Error::from(id))?
                    .aliases
                    .iter()
                    .map(|(_, a)| a.clone())
                    .collect::<Vec<_>>())
            })
        })
    }

    fn disable_alias<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        alias_id: AliasId,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.toggle_alias(py, id, alias_id, false)
    }

    fn remove_alias<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        alias_id: AliasId,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .aliases
                .remove(alias_id);
            Ok(())
        })
    }

    fn remove_module_aliases<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        module: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            let aliases = &mut state.client_for_id_mut(id).ok_or(Error::from(id))?.aliases;
            let aliases_to_remove = aliases
                .iter()
                .filter_map(|(id, alias)| {
                    if alias.module == module {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            debug!(
                "removing {} aliases that were added by module {}",
                aliases_to_remove.len(),
                module
            );

            for alias_id in aliases_to_remove {
                aliases.remove(alias_id);
            }
            Ok(())
        })
    }

    fn enable_alias<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        alias_id: AliasId,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.toggle_alias(py, id, alias_id, true)
    }

    fn new_timer<'py>(
        &self,
        py: Python<'py>,
        config: Py<TimerConfig>,
        module: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            let timers = &mut state.timers;

            trace!("setting up a new timer for module {module}");

            Python::with_gil(|pyy| {
                let new_config: PyRef<'_, TimerConfig> = config.extract(pyy)?;
                for (_, t) in &mut *timers {
                    let old_config: PyRef<'_, TimerConfig> = t.config.extract(pyy)?;
                    if new_config.name == old_config.name {
                        warn!("timer with name {} already exists", new_config.name);
                        return Ok::<_, PyErr>(None);
                    }
                }
                let (stop_tx, stop_rx) = watch::channel(false);

                Self::require_coroutine(pyy, "timer callback", &new_config.callback)?;

                let timer_id = timers.construct(|id| Timer {
                    id,
                    running: true,
                    stop_tx,
                    module,
                    config,
                });

                let task_locals = Python::with_gil(pyo3_async_runtimes::tokio::get_current_locals)?;
                tokio::spawn(pyo3_async_runtimes::tokio::scope(
                    task_locals,
                    run_timer(timer_id, new_config.clone(), stop_rx),
                ));

                Ok(Some(timer_id))
            })
        })
    }

    fn start_timer<'py>(&self, py: Python<'py>, timer_id: TimerId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            let timers = &mut state.timers;
            let timer = timers
                .get_mut(timer_id)
                .ok_or(Error::Timer(timer_id.into()))?;

            if timer.running {
                warn!("timer {} is already running", timer.id);
                return Ok(());
            }

            Python::with_gil(|pyy| {
                let config: PyRef<'_, TimerConfig> = timer.config.extract(pyy)?;
                let (stop_tx, stop_rx) = watch::channel(false);

                timer.stop_tx = stop_tx;
                timer.running = true;

                let task_locals = Python::with_gil(pyo3_async_runtimes::tokio::get_current_locals)?;
                tokio::spawn(pyo3_async_runtimes::tokio::scope(
                    task_locals,
                    run_timer(timer_id, config.clone(), stop_rx),
                ));

                Ok(())
            })
        })
    }

    fn stop_timer<'py>(&self, py: Python<'py>, id: TimerId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            match state.timers.get_mut(id) {
                Some(timer) => {
                    if timer.running {
                        timer.running = false;
                        timer.stop_tx.send(true).ok();
                    } else {
                        warn!("timer {} is already stopped", timer.id);
                    }

                    Ok(())
                }
                None => Err(Error::Timer(id.into()).into()),
            }
        })
    }

    fn get_timer<'py>(&self, py: Python<'py>, id: TimerId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| Python::with_gil(|_| {
            Ok(state.timers.get(id).cloned())
        }))
    }

    fn remove_timer<'py>(&self, py: Python<'py>, id: TimerId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            let timer = state.timers.get_mut(id).ok_or(Error::Timer(id.into()))?;
            if timer.running {
                timer.running = false;
                timer.stop_tx.send(true).ok();
            }
            info!("removed timer {id}");
            state.timers.remove(id);
            Ok(())
        })
    }

    fn remove_module_timers<'py>(
        &self,
        py: Python<'py>,
        module: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            let timers = &mut state.timers;
            let timers_to_remove = timers
                .iter()
                .filter_map(|(id, timer)| {
                    if timer.module == module {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            debug!(
                "removing {} timers that were added by module {}",
                timers_to_remove.len(),
                module
            );

            for timer_id in timers_to_remove {
                timers.remove(timer_id);
            }
            Ok(())
        })
    }

    fn timers<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Python::with_gil(|_| {
                Ok(state
                    .timers
                    .iter()
                    .map(|(_, a)| a.clone())
                    .collect::<Vec<_>>())
            })
        })
    }

    fn get_input<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Ok(state
                .client_for_id(id)
                .ok_or(Error::from(id))?
                .input
                .value()
                .to_string())
        })
    }

    fn set_input<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        input: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .input
                .set_value(&input);
            Ok(())
        })
    }

    fn add_output<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        output: client::output::Item,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .output
                .push(output);
            Ok(())
        })
    }

    fn add_outputs<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        output: Vec<client::output::Item>,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .output
                .extend(output.into_iter());
            Ok(())
        })
    }

    fn dimensions<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Ok(state
                .client_for_id(id)
                .ok_or(Error::from(id))?
                .buffer_dimensions)
        })
    }

    fn layout<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Python::with_gil(|_| {
                Ok(state
                    .client_for_id(id)
                    .ok_or(Error::from(id))?
                    .layout
                    .clone())
            })
        })
    }

    fn new_buffer<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        config: Py<tui::layout::BufferConfig>,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            Ok(state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .extra_buffers
                .construct(|id| tui::layout::ExtraBuffer { id, config }))
        })
    }

    fn get_buffer<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        buffer_id: tui::layout::BufferId,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Python::with_gil(|_| {
                Ok(state
                    .client_for_id(id)
                    .ok_or(Error::from(id))?
                    .extra_buffers
                    .get(buffer_id)
                    .cloned())
            })
        })
    }

    fn buffers<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            Python::with_gil(|_| {
                Ok(state
                    .client_for_id(id)
                    .ok_or(Error::from(id))?
                    .extra_buffers
                    .iter()
                    .map(|(_, a)| a.clone())
                    .collect::<Vec<_>>())
            })
        })
    }

    fn remove_buffer<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        buffer_id: tui::layout::BufferId,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .extra_buffers
                .remove(buffer_id);
            Ok(())
        })
    }

    fn gmcp_enabled<'py>(&self, py: Python<'py>, id: SessionId) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            Ok(state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .gmcp_enabled())
        })
    }

    // TODO(XXX): it would be nice to take PyObject as data arg and handle JSON
    //   serialization in Rust, but PyObject doesn't impl Serialized even with the
    //   serde feature of PyO3 active. Hmmm. Needs more investigation!
    fn gmcp_send<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        package: String,
        json: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .gmcp_send_json(&package, &json)
                .map_err(Into::into)
        })
    }

    fn gmcp_register<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        module: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .gmcp_register(&module)
                .map_err(Into::into)
        })
    }

    fn gmcp_unregister<'py>(
        &self,
        py: Python<'py>,
        id: SessionId,
        module: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            state
                .client_for_id_mut(id)
                .ok_or(Error::from(id))?
                .gmcp_unregister(&module)
                .map_err(Into::into)
        })
    }

    #[pyo3(signature = (custom_type, data, id=None))]
    fn emit_event<'py>(
        &self,
        py: Python<'py>,
        custom_type: String,
        data: PyObject,
        id: Option<SessionId>,
    ) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |state| {
            state
                .event_tx
                .send(Event::Python {
                    id,
                    custom_type,
                    data,
                })
                .map_err(|e| Error::from(e).into())
        })
    }

    fn quit<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            info!("quitting by request from Python");
            state.ui_state = UiState::Exit;
            Ok(())
        })
    }

    fn reload<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        with_state!(self, py, |mut state| {
            info!("reloading by request from Python");
            state.ui_state = UiState::ReloadPython;
            Ok(())
        })
    }

    #[allow(clippy::unused_self)]
    fn __str__(&self) -> String {
        format!("MudpuppyCore({GIT_COMMIT_HASH})")
    }
}

// NOTE(XXX): It's tempting to want to lift out all of the events that have a session id
//  into a sub-enum of a variant that holds the ID. In practice I found this more awkward
//  to use from Python.
#[derive(Debug, Clone)]
#[pyclass]
pub enum Event {
    NewSession {
        // Having a separate ID field is duplicative with 'SessionInfo.id', but makes it easier to
        // use consistently with other events.
        id: SessionId,
        info: SessionInfo,
        mud: Mud,
    },
    Connection {
        id: SessionId,
        status: client::Status,
    },
    Prompt {
        id: SessionId,
        prompt: MudLine,
    },
    Iac {
        id: SessionId,
        command: u8,
    },
    OptionEnabled {
        id: SessionId,
        option: u8,
    },
    OptionDisabled {
        id: SessionId,
        option: u8,
    },
    Subnegotiation {
        id: SessionId,
        option: u8,
        data: Vec<u8>,
    },
    BufferResized {
        id: SessionId,
        dimensions: (u16, u16),
    },
    InputLine {
        id: SessionId,
        input: InputLine,
    },
    Shortcut {
        id: SessionId,
        shortcut: Shortcut,
    },
    KeyPress {
        id: SessionId,
        // TODO(XXX): avoid JSON marshal, just pull out the data we care about...
        json: String,
    },
    GmcpEnabled {
        id: SessionId,
    },
    GmcpDisabled {
        id: SessionId,
    },
    GmcpMessage {
        id: SessionId,
        package: String,
        json: String,
    },
    Python {
        id: Option<SessionId>,
        custom_type: String,
        data: PyObject,
    },
    ConfigReloaded {},
    PythonReloaded {},
    ResumeSession {
        id: SessionId,
    },
}

#[pymethods]
impl Event {
    #[must_use]
    pub fn r#type(&self) -> EventType {
        match self {
            Self::NewSession { .. } => EventType::NewSession {},
            Self::Connection { .. } => EventType::Connection {},
            Self::Prompt { .. } => EventType::Prompt {},
            Self::ConfigReloaded {} => EventType::ConfigReloaded {},
            Self::Iac { .. } => EventType::Iac {},
            Self::OptionEnabled { .. } => EventType::OptionEnabled {},
            Self::OptionDisabled { .. } => EventType::OptionDisabled {},
            Self::Subnegotiation { .. } => EventType::Subnegotiation {},
            Self::BufferResized { .. } => EventType::BufferResized {},
            Self::InputLine { .. } => EventType::InputLine {},
            Self::Shortcut { .. } => EventType::Shortcut {},
            Self::KeyPress { .. } => EventType::KeyPress {},
            Self::Python { .. } => EventType::Python {},
            Self::GmcpEnabled { .. } => EventType::GmcpEnabled {},
            Self::GmcpDisabled { .. } => EventType::GmcpDisabled {},
            Self::GmcpMessage { .. } => EventType::GmcpMessage {},
            Self::PythonReloaded { .. } => EventType::PythonReloaded {},
            Self::ResumeSession { .. } => EventType::ResumeSession {},
        }
    }

    pub fn session_id(&self) -> Option<SessionId> {
        match self {
            Event::NewSession { id, .. }
            | Event::Connection { id, .. }
            | Event::Prompt { id, .. }
            | Event::OptionEnabled { id, .. }
            | Event::OptionDisabled { id, .. }
            | Event::Subnegotiation { id, .. }
            | Event::Iac { id, .. }
            | Event::BufferResized { id, .. }
            | Event::InputLine { id, .. }
            | Event::Shortcut { id, .. }
            | Event::KeyPress { id, .. }
            | Event::GmcpEnabled { id, .. }
            | Event::GmcpDisabled { id, .. }
            | Event::GmcpMessage { id, .. }
            | Event::ResumeSession { id, .. } => Some(*id),
            Event::Python { id, .. } => *id,
            Event::ConfigReloaded { .. } | Event::PythonReloaded { .. } => None,
        }
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::NewSession { info, .. } => {
                write!(f, "event: new session created {info}")
            }
            Event::Connection { id, status } => {
                write!(f, "event: connection ID {id} is now {status}")
            }
            Event::Prompt { id, prompt } => {
                write!(f, "event: connection ID {id} received prompt {prompt:?}")
            }
            Event::Iac { id, command } => {
                write!(f, "event: connection ID {id} received telnet IAC {command}")
            }
            Event::OptionEnabled { id, option } => {
                write!(
                    f,
                    "event: connection ID {id} enabled telnet option {option}"
                )
            }
            Event::OptionDisabled { id, option } => {
                write!(
                    f,
                    "event: connection ID {id} disabled telnet option {option}"
                )
            }
            Event::ConfigReloaded { .. } => {
                write!(f, "event: configuration was reloaded")
            }
            Event::Subnegotiation {
                id, option, data, ..
            } => {
                write!(
                    f,
                    "event: connection ID {id} got telnet subnegotiation {option} of {} bytes",
                    data.len()
                )
            }
            Event::BufferResized { id, dimensions } => {
                write!(
                    f,
                    "event: connection ID {id} buffer resized to {}x{}",
                    dimensions.0, dimensions.1
                )
            }
            Event::InputLine { id, input } => {
                write!(f, "event: connection ID {id} sent input line {input}")
            }
            Event::Shortcut { id, shortcut } => {
                write!(
                    f,
                    "event: connection ID {id} triggered shortcut {shortcut:?}"
                )
            }
            Event::KeyPress { id, json } => {
                write!(f, "event: connection ID {id} key press {json}")
            }
            Event::Python {
                id, custom_type, ..
            } => {
                write!(
                    f,
                    "event: connection ID {id:?} custom python event {custom_type}"
                )
            }
            Event::GmcpEnabled { id } => {
                write!(f, "event: connection ID {id} GMCP enabled")
            }
            Event::GmcpDisabled { id } => {
                write!(f, "event: connection ID {id} GMCP disabled")
            }
            Event::GmcpMessage { id, package, .. } => {
                write!(f, "event: connection ID {id} GMCP message {package}")
            }
            Event::ResumeSession { id, .. } => {
                write!(f, "event: connection ID {id} resumed")
            }
            Event::PythonReloaded { .. } => {
                write!(f, "event: python code reloaded")
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[pyclass(eq, eq_int)]
pub enum EventType {
    NewSession,
    Connection,
    Prompt,
    ConfigReloaded,
    PythonReloaded,
    Iac,
    OptionEnabled,
    OptionDisabled,
    Subnegotiation,
    BufferResized,
    InputLine,
    Shortcut,
    KeyPress,
    Python,
    GmcpEnabled,
    GmcpDisabled,
    GmcpMessage,
    ResumeSession,
}

#[pymethods]
impl EventType {
    // TODO(XXX): use `pyclass(hash)` once available.
    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    fn __str__(&self) -> String {
        match self {
            Self::NewSession { .. } => "event type: new session",
            Self::Connection { .. } => "event type: connection",
            Self::Prompt { .. } => "event type: prompt",
            Self::ConfigReloaded { .. } => "event type: config reloaded",
            Self::Iac { .. } => "event type: telnet IAC",
            Self::OptionEnabled { .. } => "event type: telnet option enabled",
            Self::OptionDisabled { .. } => "event type: telnet option disabled",
            Self::Subnegotiation { .. } => "event type: telnet subnegotiation",
            Self::BufferResized { .. } => "event type: buffer resized",
            Self::InputLine { .. } => "event type: input line",
            Self::Shortcut { .. } => "event type: keyboard shortcut",
            Self::KeyPress { .. } => "event type: key press",
            Self::Python { .. } => "event type: custom python event",
            Self::GmcpEnabled { .. } => "event type: GMCP enabled",
            Self::GmcpDisabled { .. } => "event type: GMCP disabled",
            Self::GmcpMessage { .. } => "event type: GMCP message",
            Self::PythonReloaded { .. } => "event type: python reloaded",
            Self::ResumeSession { .. } => "event type: session resumed",
        }
        .to_string()
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

type HandlerMap = HashMap<EventType, Py<PyList>>;

#[derive(Debug, Default, Clone)]
#[pyclass]
pub struct EventHandlers {
    handlers: HandlerMap,
}

impl EventHandlers {
    /// Dispatch an event to the appropriate handlers.
    ///
    /// If a handler is to be dispatched to, it is called to produce a future that is appended to the
    /// unordered futures set for the main event loop to await.
    ///
    /// # Errors
    /// If invoking the handler fails, or the handler's python coroutine can't be converted to
    /// a Rust future to await.
    pub fn dispatch(
        &self,
        py: Python<'_>,
        event: &Event,
        futures: &mut FuturesUnordered<PyFuture>,
    ) -> Result<(), Error> {
        if let Some(handlers) = self.get_handlers(&event.r#type()) {
            for handler_tuple in handlers.bind(py) {
                let handler_tuple: &Bound<'_, PyTuple> =
                    handler_tuple.downcast().map_err(|e| Error::Python {
                        error: PyTypeError::new_err(format!("expected tuple, got {e}")),
                        traceback: String::default(),
                    })?;
                let handler = handler_tuple.get_item(0)?;
                futures.push(Box::pin(pyo3_async_runtimes::tokio::into_future(
                    handler.call1((event.clone(),))?,
                )?));
            }
        }
        Ok(())
    }
}

#[pymethods]
impl EventHandlers {
    #[new]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an event handler that will be called for a given event type.
    ///
    /// # Errors
    /// If the handler can't be added to the list of handlers.
    pub fn add_handler(
        &mut self,
        py: Python<'_>,
        event_type: EventType,
        handler: Py<PyFunction>,
        module: &str,
    ) -> PyResult<()> {
        trace!(
            "adding handler: {:?} for module {} -> {}",
            event_type,
            module,
            handler
                .getattr(py, "__qualname__")
                .map(|x| x.to_string())
                .unwrap_or("unknown".to_string())
        );
        self.handlers
            .entry(event_type)
            .or_insert_with(|| PyList::empty(py).into())
            .bind(py)
            .append((handler, module))
    }

    #[must_use]
    pub fn get_handlers(&self, event_type: &EventType) -> Option<&Py<PyList>> {
        self.handlers.get(event_type)
    }

    fn get_handler_events(&self) -> Vec<EventType> {
        self.handlers.keys().cloned().collect()
    }
}

fn user_modules() -> Result<Vec<PyObject>, Error> {
    Python::with_gil(|py| {
        let mut modules = Vec::new();
        for entry in config_dir().read_dir()? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }

            let Some(module_name) = entry
                .file_name()
                .to_str()
                .and_then(|f| f.strip_suffix(".py").map(ToString::to_string))
            else {
                continue;
            };

            // If there's both a .py with a given name, and a directory
            // with the matching name, Python will end up loading the directory/__init__.py
            // That's not what we want here. We only want to load top level .py scripts.
            let dir_name = config_dir().join(&module_name);
            if fs::metadata(&dir_name).is_ok_and(|md| md.is_dir()) {
                warn!(
                    "skipping user module {module_name}.py because {} directory exists.",
                    dir_name.display()
                );
                continue;
            }

            info!("loading user module {module_name}.py");
            let module: Py<PyAny> = PyModule::import(py, &*module_name)?.into();
            modules.push(module);
        }
        Ok(modules)
    })
}

async fn run_timer(timer_id: TimerId, config: TimerConfig, mut stop_rx: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(config.duration);
    let mut ticks = 0;
    let mut first_tick = true;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // The interval is always immediately ready for the first tick - we prefer
                // to skip that tick and run the callback only after the duration expired.
                if first_tick {
                    first_tick = false;
                    continue;
                }
                let awaitable = Python::with_gil(|py|{
                    pyo3_async_runtimes::tokio::into_future(config.callback.bind(py).call1((timer_id, config.session_id,))?)
                });
                match awaitable {
                    // TODO(XXX): method for passing error back for ui state...
                    Err(err) => {
                        error!("Timer '{}' callback failed to produce future: {:?}", config.name, err);
                        break;
                    }
                    Ok(future) => if let Err(err) = future.await {
                        error!("Timer '{}' callback failed: {:?}", config.name, err);
                        break;
                    }
                }

                // TODO(XXX): method for passing back that the timer is expired.
                if config.max_ticks > 0 && ticks >= config.max_ticks {
                    info!("Timer '{}' reached max ticks ({}).", config.name, config.max_ticks);
                    break;
                }

                ticks += 1;

            }
            _ = stop_rx.changed() => {
                info!("Timer '{}' was stopped.", config.name);
                break;
            }
        }
    }
}

// TODO(XXX): I tried, and tried to pull out the common boilerplate in these macros to a fn
//   but, my async/rust-fu is too weak. Alas... The macros will do for now.

macro_rules! with_state {
    ($self:ident, $py:ident, |mut $state:ident| $body:expr) => {{
        let state_lock = $self.state.clone();
        let waker = $self.waker.clone();
        pyo3_async_runtimes::tokio::future_into_py($py, async move {
            let _ = waker.send(());
            let mut $state = state_lock.write().await;
            $body
        })
    }};
    ($self:ident, $py:ident, |$state:ident| $body:expr) => {{
        let state_lock = $self.state.clone();
        let waker = $self.waker.clone();
        pyo3_async_runtimes::tokio::future_into_py($py, async move {
            let _ = waker.send(());
            let $state = state_lock.read().await;
            $body
        })
    }};
}

pub(crate) use with_state;

macro_rules! builtin_modules {
     ($($module:expr),* $(,)?) => {
        {
            let mut modules = Vec::new();
            $(
                modules.push(Python::with_gil(|py| {
                    trace!(concat!("loading module ", $module, ".py"));
                    let module: PyObject = PyModule::from_code(
                        py,
                        c_str!(include_str!(concat!("../python/", $module, ".py"))),
                        c_str!(concat!($module, ".py")),
                        c_str!($module),
                    )
                    .unwrap() // Safety: builtin modules must always compile!
                    .into();
                    module
                }));
            )*
            modules
        }
    };
}

pub(crate) use builtin_modules;
