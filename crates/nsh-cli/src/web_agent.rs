//! A GPUI agent attached to `nsh run --target web` (`GPUI_AGENT=1`): the
//! `gpui-agent` CLI drives the app through `nsh-gpui-agent`, in its strict,
//! natively hit-tested mode, as on Linux and Android.
//!
//! A page cannot listen on a socket, and a tab the user opened takes no
//! trusted input, so the agent runs here: `nsh` opens the development page
//! in a browser of its own ([`cdp`], headless unless
//! `GPUI_AGENT_WEB_HEADED=1`), which the dev server knows by a key
//! (`DevServer::agent_session`) handed to the page before its scripts run,
//! and acts on it as `nsh test --target web` does ([`WebNative`]). The
//! agent's server listens on `GPUI_AGENT_ADDR` (loopback only) and checks
//! `GPUI_AGENT_TOKEN`, both from this process's environment.
//!
//! [`cdp`]: crate::cdp

use std::{
    net::SocketAddr,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use nsh_core::geometry::Points;
use nsh_dev::DevServer;
use nsh_gpui_agent::{NshAgentHost, PlatformKind, WorkerHandle};
use nsh_testing::agent::{Agent, RemoteScene};
use serde_json::json;

use crate::{
    cdp::{self, Browser, Page},
    ui,
    web_native::{ASK, WebNative, set_size},
};

/// How long the page may take to load and connect.
const CONNECT: Duration = Duration::from_secs(60);
/// The page's size: a laptop's window, the desktop idiom.
const VIEWPORT: (f32, f32) = (1280.0, 800.0);

/// The agent's address and token, when `GPUI_AGENT=1` asks for one (read
/// before anything starts, so that a wrong configuration stops the run).
pub(crate) fn config() -> anyhow::Result<Option<(SocketAddr, String)>> {
    nsh_gpui_agent::worker_config_from_env().map_err(anyhow::Error::msg)
}

/// The agent while it runs: dropping it stops its server, closes the page
/// and the browser. Ctrl-C meanwhile asks the run to stop
/// ([`ui::Stoppable`]), so the browser never outlives it.
pub(crate) struct WebAgent {
    _worker: WorkerHandle,
    page: Arc<Page>,
    browser: Arc<Browser>,
    _stoppable: ui::Stoppable,
}

impl Drop for WebAgent {
    fn drop(&mut self) {
        // The agent's server thread still holds the page: the browser goes
        // now, not when the last of them does.
        self.page.close();
        self.browser.close();
    }
}

/// Opens the page `server` serves in a browser of its own and serves the
/// agent on `addr`, `token` required.
pub(crate) fn start(
    server: Arc<DevServer>,
    entry: &Path,
    addr: SocketAddr,
    token: String,
) -> anyhow::Result<WebAgent> {
    // From the browser's start on: a stop closes it on the way out.
    let stoppable = ui::Stoppable::new();
    let exe = cdp::find(None)?;
    let headed = matches!(
        std::env::var("GPUI_AGENT_WEB_HEADED").as_deref(),
        Ok("1" | "true")
    );
    let key = key()?;
    server.agent_session(key.clone());
    let browser = Arc::new(Browser::launch_with(&exe, headed).map_err(anyhow::Error::msg)?);
    let page = Arc::new(browser.page().map_err(anyhow::Error::msg)?);
    open(&page, &server, key).map_err(anyhow::Error::msg)?;
    let scene = Arc::new(RemoteScene::new({
        let server = server.clone();
        move |step| server.step(step, ASK)
    }));
    let native = WebNative::new(page.clone(), server, true);
    let agent = Arc::new(Agent::requiring_native_hit_test(scene, Box::new(native)));
    let config = crate::package::read_config(entry)?;
    let (name, _) = crate::package::name_and_id(entry, &config);
    let host = NshAgentHost::new(name, PlatformKind::Web, agent).map_err(anyhow::Error::msg)?;
    let worker = nsh_gpui_agent::spawn_worker(addr, token, host).map_err(anyhow::Error::msg)?;
    ui::note(format!(
        "gpui-agent: {} ({}, a browser of its own)",
        worker.local_addr(),
        if headed { "in a window" } else { "headless" }
    ));
    Ok(WebAgent {
        _worker: worker,
        page,
        browser,
        _stoppable: stoppable,
    })
}

/// Hands the page its key before its scripts run (on every load: the page
/// loads again when the runtime changes), opens it from this machine and
/// waits for it to connect.
fn open(page: &Page, server: &DevServer, key: String) -> Result<(), String> {
    page.call(
        "Emulation.setFocusEmulationEnabled",
        json!({ "enabled": true }),
    )?;
    set_size(page, Points(VIEWPORT.0), Points(VIEWPORT.1))?;
    // Read once by `dev.html`, which deletes it; never in the page's URL.
    // (The page's scripts to come run only once its domain is enabled.)
    page.call("Page.enable", json!({}))?;
    page.call(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({
            "source": format!(
                "Object.defineProperty(globalThis, '__nshAgentKey', \
                 {{ value: {}, configurable: true }});",
                serde_json::Value::from(key)
            ),
        }),
    )?;
    let url = format!("http://127.0.0.1:{}/", server.local_addr().port());
    page.call("Page.navigate", json!({ "url": url }))?;
    let deadline = Instant::now() + CONNECT;
    while !server.has_tester() {
        if ui::stop_requested() {
            return Err("stopped".into());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the agent's page did not connect in {} s{}",
                CONNECT.as_secs(),
                page.browser().output()
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// A fresh key for the agent's page: 128 random bits, in hex.
fn key() -> anyhow::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn each_agent_page_gets_a_key_of_its_own() {
        let (a, b) = (super::key().unwrap(), super::key().unwrap());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
