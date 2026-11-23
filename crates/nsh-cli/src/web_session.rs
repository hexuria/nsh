use std::{
    any::Any,
    path::Path,
    sync::{Arc, OnceLock},
};

use nsh_dev::DevServer;

/// A web development session started by the command line.
#[non_exhaustive]
pub struct WebSession<'a> {
    /// The server publishing the app and serving its development page.
    pub server: Arc<DevServer>,
    /// The app's canonical entry path.
    pub entry: &'a Path,
    /// The app's display name from its package configuration.
    pub name: &'a str,
    /// The app runs a Rust root (`nsh.json` `"root"`), rather than JavaScript.
    pub rust_root: bool,
}

/// Attaches a tool to a web development session. The returned value is
/// kept until the session's watch ends; an error stops the run.
pub type WebSessionHook = fn(&WebSession<'_>) -> anyhow::Result<Option<Box<dyn Any>>>;

static WEB_SESSION_HOOK: OnceLock<WebSessionHook> = OnceLock::new();

/// Sets the hook called after a web development session starts, before
/// watching its source. A second call returns the hook without installing it.
pub fn set_web_session_hook(hook: WebSessionHook) -> Result<(), WebSessionHook> {
    WEB_SESSION_HOOK.set(hook)
}

pub(crate) fn hook() -> Option<WebSessionHook> {
    WEB_SESSION_HOOK.get().copied()
}
