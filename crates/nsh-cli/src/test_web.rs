//! `nsh test --target web`: the apps tests open run on the web's
//! development host, a page of a headless Chromium browser ([`cdp`]). Each
//! `open` builds the entry and serves it with the page in a dev server's
//! test session of its own; the page connects as a host that takes test
//! steps, and its runtime keeps the model of the app
//! (`nsh_testing::agent::Model`). The agent runs here: it asks the page
//! about the app ([`RemoteScene`]) and acts with the browser's trusted
//! input, which pages cannot tell from a user's ([`WebNative`]).

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nsh_core::geometry::{Points, Size};
use nsh_dev::{
    DevServer,
    protocol::{StepOutcome, TestStep},
};
use nsh_testing::{
    Host, HostOptions, StartHost,
    agent::{Agent, RemoteScene},
};
use serde_json::{Value, json};

use crate::{
    cdp::{self, Browser, Page},
    web_native::{ASK, WebNative, set_size},
};

/// How long the page may take to load and connect.
const CONNECT: Duration = Duration::from_secs(60);

/// Starts the apps tests open in pages of `browser` (`--browser`), else of
/// the Chrome, Edge or Chromium found on this machine.
pub(crate) fn starter(browser: Option<&Path>) -> anyhow::Result<StartHost> {
    let exe = cdp::find(browser)?;
    // Built (in a checkout) or found once, before the first test.
    let runtime = crate::runtime::test_web()?;
    let host = Arc::new(crate::web::dev_host(&runtime, false)?);
    // One browser for the run, started with the first app.
    let running = Arc::new(Mutex::new(None::<Arc<Browser>>));
    Ok(Arc::new(move |entry: &Path, options: &HostOptions| {
        start(&exe, &running, &host, entry, options).map(|h| Box::new(h) as Box<dyn Host>)
    }))
}

/// An app on a page, and the agent that takes its test's steps.
struct WebHost {
    agent: Agent,
    page: Arc<Page>,
    _server: Arc<DevServer>,
}

impl Host for WebHost {
    fn step(&self, step: TestStep, _: Duration) -> Result<Value, String> {
        // The agent's own waits end each step, as a host's do.
        match self.agent.take(step) {
            StepOutcome::Done { value } => Ok(value),
            StepOutcome::Failed { message } => Err(message),
        }
    }
}

impl Drop for WebHost {
    fn drop(&mut self) {
        self.page.close();
    }
}

/// Builds `entry`, serves it with the web's host page and opens that in the
/// browser (started first if it is not running).
fn start(
    exe: &Path,
    running: &Mutex<Option<Arc<Browser>>>,
    host: &(String, Vec<(String, Vec<u8>)>),
    entry: &Path,
    options: &HostOptions,
) -> Result<WebHost, String> {
    let server = Arc::new(crate::test_host::serve(entry, options)?);
    let (page, files) =
        crate::web::with_page_host(host.clone(), entry).map_err(|e| format!("{e:#}"))?;
    server.serve_web(page, files);
    let browser = {
        let mut running = running.lock().unwrap_or_else(|e| e.into_inner());
        match &*running {
            Some(browser) if browser.alive() => browser.clone(),
            _ => {
                let browser = Arc::new(Browser::launch(exe)?);
                *running = Some(browser.clone());
                browser
            }
        }
    };
    let page = Arc::new(browser.page()?);
    // Focused as the window in front is, whichever page is.
    page.call(
        "Emulation.setFocusEmulationEnabled",
        json!({ "enabled": true }),
    )?;
    let size = options
        .size
        .unwrap_or(Size::new(Points(390.0), Points(844.0)));
    set_size(&page, size.width, size.height)?;
    page.call(
        "Page.navigate",
        json!({ "url": format!("http://{}/", server.local_addr()) }),
    )?;
    let deadline = Instant::now() + CONNECT;
    while !server.has_tester() {
        if Instant::now() >= deadline {
            return Err(format!(
                "the page did not connect in {} s{}",
                CONNECT.as_secs(),
                page.browser().output()
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let asked = server.clone();
    let scene = Arc::new(
        RemoteScene::new(move |step| asked.step(step, ASK))
            .with_manual_clock(options.environment.clock.is_some()),
    );
    let native = WebNative::new(page.clone(), server.clone(), false);
    Ok(WebHost {
        agent: Agent::new(scene, Box::new(native)),
        page,
        _server: server,
    })
}
