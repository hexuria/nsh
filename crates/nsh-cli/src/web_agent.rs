//! A GPUI agent attached to `nsh run --target web` (`GPUI_AGENT=1`): the
//! `gpui-agent` CLI drives the app through `nsh-gpui-agent`, in its strict,
//! natively hit-tested mode, as on Linux and Android.
//!
//! By default `nsh` opens the development page in a browser of its own
//! ([`cdp`], headless unless `GPUI_AGENT_WEB_HEADED=1`). With
//! `GPUI_AGENT_WEB_TAB=1`, a person opens the printed URL in their browser
//! instead, and input is dispatched synthetically by [`TabNative`]. The dev
//! server pairs the page by a key; only loopback peers are accepted, and
//! off-machine pairing is out of scope. The agent's server listens on
//! `GPUI_AGENT_ADDR` and checks `GPUI_AGENT_TOKEN`, both from this process's
//! environment.
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
use nsh_testing::agent::{Agent, Native, RemoteScene};
use serde_json::json;

use crate::{
    cdp::{self, Browser, Page},
    tab_native::TabNative,
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

pub(crate) fn tab_mode() -> bool {
    matches!(
        std::env::var("GPUI_AGENT_WEB_TAB").as_deref(),
        Ok("1" | "true")
    )
}

/// The agent while it runs: dropping it stops its server and closes an
/// owned page and browser. Ctrl-C meanwhile asks the run to stop
/// ([`ui::Stoppable`]).
pub(crate) struct WebAgent {
    _worker: WorkerHandle,
    page: Option<Arc<Page>>,
    browser: Option<Arc<Browser>>,
    _stoppable: ui::Stoppable,
}

impl Drop for WebAgent {
    fn drop(&mut self) {
        // The agent's server thread still holds the page: the browser goes
        // now, not when the last of them does.
        if let Some(page) = &self.page {
            page.close();
        }
        if let Some(browser) = &self.browser {
            browser.close();
        }
    }
}

/// Serves the agent for `server` on `addr`, requiring `token`. In tab mode,
/// a user-opened page is paired instead of launching a browser.
pub(crate) fn start(
    server: Arc<DevServer>,
    entry: &Path,
    addr: SocketAddr,
    token: String,
    tab_mode: bool,
) -> anyhow::Result<WebAgent> {
    let stoppable = ui::Stoppable::new();
    let key = key()?;
    server.agent_session(key.clone());
    let (page, browser, native): (Option<Arc<Page>>, Option<Arc<Browser>>, Box<dyn Native>) =
        if tab_mode {
            (None, None, Box::new(TabNative::new(server.clone())))
        } else {
            let exe = cdp::find(None)?;
            let headed = matches!(
                std::env::var("GPUI_AGENT_WEB_HEADED").as_deref(),
                Ok("1" | "true")
            );
            let browser = Arc::new(Browser::launch_with(&exe, headed).map_err(anyhow::Error::msg)?);
            let page = Arc::new(browser.page().map_err(anyhow::Error::msg)?);
            open(&page, &server, key.clone()).map_err(anyhow::Error::msg)?;
            let native = Box::new(WebNative::new(page.clone(), server.clone(), true));
            (Some(page), Some(browser), native)
        };
    let scene = Arc::new(RemoteScene::new({
        let server = server.clone();
        move |step| server.step(step, ASK)
    }));
    let agent = Arc::new(Agent::requiring_native_hit_test(scene, native));
    let config = crate::package::read_config(entry)?;
    let (name, _) = crate::package::name_and_id(entry, &config);
    let host = NshAgentHost::new(name, PlatformKind::Web, agent).map_err(anyhow::Error::msg)?;
    let host = if tab_mode { host.with_tab_mode() } else { host };
    let worker = nsh_gpui_agent::spawn_worker(addr, token, host).map_err(anyhow::Error::msg)?;
    if tab_mode {
        ui::note(format!(
            "gpui-agent: {} (a tab you open: http://127.0.0.1:{}/#gpui-agent={key})",
            worker.local_addr(),
            server.local_addr().port()
        ));
    } else {
        let headed = matches!(
            std::env::var("GPUI_AGENT_WEB_HEADED").as_deref(),
            Ok("1" | "true")
        );
        ui::note(format!(
            "gpui-agent: {} ({}, a browser of its own)",
            worker.local_addr(),
            if headed { "in a window" } else { "headless" }
        ));
    }
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
