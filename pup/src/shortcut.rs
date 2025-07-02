use std::fmt::{Debug, Formatter};
use async_trait::async_trait;

use crate::app::{AppData, TabAction};
use crate::error::Error;
use crate::keyboard::KeyEvent;

#[async_trait]
pub(super) trait Shortcut: Debug + Send + Sync {
    fn name(&self) -> String; // TODO(XXX): Cow?

    async fn execute(&self, app: &mut AppData, event: KeyEvent) -> Result<Option<TabAction>, Error>;
}

pub(super) struct BuiltinShortcut {
    pub(super) name: String,
    pub(super) handler: BuiltinShortcutHandler,
}

#[async_trait]
impl Shortcut for BuiltinShortcut {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn execute(&self, app: &mut AppData, event: KeyEvent) -> Result<Option<TabAction>, Error> {
        (self.handler)(app, event)
    }
}

impl Debug for BuiltinShortcut {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuiltinShortcut")
            .field("name", &self.name)
            .field("handler", &"<function>") // Avoid printing the function pointer
            .finish()
    }
}

pub(super) type BuiltinShortcutHandler = Box<dyn Fn(&mut AppData, KeyEvent) -> Result<Option<TabAction>, Error> + Send + Sync>;
