use std::fmt::{self, Display, Formatter};

use futures::stream::FuturesUnordered;
use pyo3::types::PyAnyMethods;
use pyo3::{Py, PyAny, Python, pyclass, pymethods};
use regex::Regex;
use tracing::{Level, debug, instrument, trace};

use crate::error::{Error, ErrorKind};
use crate::python;
use crate::python::PyFuture;
use crate::session::MudLine;

// TODO(XXX): flagset instead of bools
#[derive(Debug, Clone)]
#[pyclass]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Trigger {
    #[pyo3(get, set)]
    pub(crate) name: String,

    #[pyo3(get, set)]
    pub(crate) enabled: bool,

    #[pyo3(get, set)]
    strip_ansi: bool,

    #[pyo3(get, set)]
    prompt: bool,

    #[pyo3(get, set)]
    pub(crate) gag: bool,

    #[pyo3(get, set)]
    pub(crate) callback: Option<Py<PyAny>>,

    #[pyo3(get, set)]
    pub(crate) highlight: Option<Py<PyAny>>,

    #[pyo3(get, set)]
    reaction: Option<String>,

    #[pyo3(get)]
    hit_count: u64,

    regex: Regex,
}

impl Trigger {
    #[instrument(level = Level::TRACE, skip(py, py_self, futures))]
    pub(super) fn evaluate(
        py: Python<'_>,
        py_self: Py<Trigger>,
        futures: &FuturesUnordered<PyFuture>,
        session: &python::Session,
        line: &mut MudLine,
    ) -> Result<(), Error> {
        // Note: care is taken here to avoid runtime borrow errors.

        // First, borrow a mutable reference to the trigger to perform a match test.
        // This will increase the hit_count stored in the alias if it matches. A match
        // will yield the matched groups and drop the ref.
        let groups = {
            let mut trigger = py_self.borrow_mut(py);
            if !trigger.enabled {
                trace!(name = trigger.name, "ignoring disabled trigger");
                return Ok(());
            }
            trace!(name = trigger.name, "evaluating trigger");

            let (matched, groups) = trigger.matches(line);
            if !matched {
                return Ok(());
            }
            debug!(?trigger, "matched line");
            groups
        };

        // Then, borrow an immutable reference to extract the callback/highlight/gag
        // status, cloning the Py<PyAny> ref's so we don't retain any borrows of 'py_self'.
        let (callback, highlight, gag) = {
            let trigger = py_self.borrow(py);
            (
                trigger.callback.clone(),
                trigger.highlight.clone(),
                trigger.gag,
            )
        };

        if gag {
            trace!("line was gagged by trigger default");
            line.gag = true;
        }

        if let Some(highlight) = highlight {
            trace!("calling highlight");
            let new_line = highlight.call1(
                py,
                (
                    session.clone(),
                    py_self.clone(),
                    line.clone(),
                    groups.clone(),
                ),
            )?;
            let new_line: MudLine = new_line.extract(py).map_err(ErrorKind::from)?;
            trace!(new_line=?new_line, "line was replaced by trigger");
            *line = new_line;
        }

        if let Some(callback) = callback {
            trace!("scheduling callback");
            let callback =
                callback
                    .bind(py)
                    .call1((session.clone(), py_self, line.clone(), groups))?;
            futures.push(Box::pin(pyo3_async_runtimes::tokio::into_future(callback)?));
        }

        Ok(())
    }
}

#[pymethods]
impl Trigger {
    #[new]
    #[pyo3(signature = (pattern, name, *, strip_ansi=false, prompt=false, gag=false, callback=None, highlight=None, reaction=None))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        pattern: &str,
        name: String,
        strip_ansi: bool,
        prompt: bool,
        gag: bool,
        callback: Option<Py<PyAny>>,
        highlight: Option<Py<PyAny>>,
        reaction: Option<String>,
    ) -> Result<Self, Error> {
        let regex = Regex::new(pattern).map_err(ErrorKind::InvalidRegex)?;
        Ok(Self {
            name,
            enabled: true,
            strip_ansi,
            prompt,
            gag,
            callback,
            highlight,
            reaction,
            hit_count: 0,
            regex,
        })
    }

    pub(crate) fn matches(&mut self, line: &MudLine) -> (bool, Option<Vec<String>>) {
        if !line.prompt && self.prompt {
            return (false, None);
        }
        let stripped_haystack;
        let line = if self.strip_ansi {
            stripped_haystack = line.stripped();
            stripped_haystack.as_str()
        } else {
            &line.to_str()
        };
        self.regex.captures(line).map_or((false, None), |matches| {
            let captures = matches
                .iter()
                .skip(1)
                .map(|m| m.unwrap().as_str().to_owned())
                .collect();
            self.hit_count += 1;
            (true, Some(captures))
        })
    }

    #[must_use]
    fn pattern(&self) -> &str {
        self.regex.as_str()
    }

    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for Trigger {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.regex)
    }
}
