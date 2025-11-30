use std::fmt::{self, Display, Formatter};

use futures::stream::FuturesUnordered;
use pyo3::types::PyAnyMethods;
use pyo3::{Py, PyAny, Python, pyclass, pymethods};
use regex::Regex;
use tracing::{Level, debug, instrument, trace};

use crate::error::{Error, ErrorKind};
use crate::python;
use crate::python::PyFuture;
use crate::session::InputLine;

// TODO(XXX): flagset instead of bools
#[derive(Debug, Clone)]
#[pyclass]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Alias {
    #[pyo3(get, set)]
    pub(crate) name: String,

    #[pyo3(get, set)]
    pub(crate) enabled: bool,

    #[pyo3(get, set)]
    pub(crate) callback: Option<Py<PyAny>>,

    #[pyo3(get, set)]
    reaction: Option<String>,

    #[pyo3(get)]
    hit_count: u64,

    regex: Regex,
}

impl Alias {
    #[instrument(level = Level::TRACE, skip(py, py_self, futures))]
    pub(super) fn evaluate(
        py: Python<'_>,
        py_self: Py<Alias>,
        futures: &FuturesUnordered<PyFuture>,
        session: &python::Session,
        line: &mut InputLine,
    ) -> Result<(), Error> {
        // Note: care is taken here to avoid runtime borrow errors.

        // First, borrow a mutable reference to the alias to perform a match test.
        // This will increase the hit_count stored in the alias if it matches. A match
        // will yield the matched groups and a possible replacement to send and drop
        // the ref.
        let (groups, replacement) = {
            let mut alias = py_self.borrow_mut(py);
            if !alias.enabled {
                trace!(name = alias.name, "ignoring disabled alias");
                return Ok(());
            }
            trace!(alias.name, "evaluating alias");

            let (matched, groups) = alias.matches(&line.sent);
            if !matched {
                return Ok(());
            }

            debug!(?alias, "matched line");
            (groups, alias.reaction.clone())
        };

        // Then, borrow an immutable reference to extract the callback, cloning
        // the Py<PyAny> refs so we don't retain any borrows of 'py_self'.
        let callback = {
            let alias = py_self.borrow(py);
            alias.callback.clone()
        };

        if let Some(callback) = callback {
            trace!("scheduling callback");
            let callback =
                callback
                    .bind(py)
                    .call1((session.clone(), py_self, line.clone(), groups))?;
            futures.push(Box::pin(pyo3_async_runtimes::tokio::into_future(callback)?));
        }

        line.original = Some(line.sent.clone());
        if !line.sent.is_empty() {
            line.sent = replacement.unwrap_or(line.sent.clone());
        }
        Ok(())
    }
}

#[pymethods]
impl Alias {
    #[new]
    #[pyo3(signature = (pattern, name, *, callback=None, reaction=None))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        pattern: &str,
        name: String,
        callback: Option<Py<PyAny>>,
        reaction: Option<String>,
    ) -> Result<Self, Error> {
        let regex = Regex::new(pattern).map_err(ErrorKind::InvalidRegex)?;
        Ok(Self {
            name,
            enabled: true,
            callback,
            reaction,
            hit_count: 0,
            regex,
        })
    }

    pub(crate) fn matches(&mut self, input: &str) -> (bool, Option<Vec<String>>) {
        match self.regex.captures(input) {
            Some(matches) => {
                let captures = matches
                    .iter()
                    .skip(1)
                    .map(|m| m.unwrap().as_str().to_owned())
                    .collect();
                self.hit_count += 1;
                (true, Some(captures))
            }
            None => (false, None),
        }
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

impl Display for Alias {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.regex)
    }
}
