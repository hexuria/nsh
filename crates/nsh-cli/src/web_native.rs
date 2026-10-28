//! The web's part of the test agent (`nsh_testing::agent::Native`): where
//! the page's views are (the page answers over the dev server) and the
//! browser's trusted input there, over the DevTools protocol ([`cdp`]).
//! `nsh test --target web` acts through it ([`crate::test_web`]), and so
//! does a GPUI agent attached to `nsh run --target web`
//! ([`crate::web_agent`]), which also asks what strict mode needs.
//!
//! [`cdp`]: crate::cdp

use std::{sync::Arc, time::Duration};

use nsh_core::{
    NodeId,
    geometry::{Bounds, Point, Points, Size, point},
    keymap::shifted,
};
use nsh_dev::{
    DevServer,
    protocol::{SceneQuery, TestStep},
};
use nsh_testing::agent::{Key, Native};
use serde_json::{Value, json};

use crate::cdp::Page;

/// How long the page may take to answer a question about the app.
pub(crate) const ASK: Duration = Duration::from_secs(10);
/// A press's hold, as a quick click's.
const CLICK: Duration = Duration::from_millis(60);
/// A precise scroll's events, a display frame apart.
const FRAME: Duration = Duration::from_millis(16);
/// How far one wheel event moves at most, in CSS pixels.
const SCROLL_STEP: f32 = 20.0;

/// The viewport of `page`, `width` × `height` CSS pixels.
pub(crate) fn set_size(page: &Page, width: Points, height: Points) -> Result<(), String> {
    page.call(
        "Emulation.setDeviceMetricsOverride",
        json!({
            "width": width.0.round() as i64,
            "height": height.0.round() as i64,
            // The display's own.
            "deviceScaleFactor": 0,
            "mobile": false,
        }),
    )
    .map(drop)
}

/// The page's part of the agent: where views are (the page answers) and the
/// browser's input there. CSS pixels are points.
pub(crate) struct WebNative {
    page: Arc<Page>,
    server: Arc<DevServer>,
    /// A GPUI agent's (`nsh run`, [`crate::web_agent`]): it answers what
    /// strict mode asks (hits, focus, text bounds, the keys it can send);
    /// a test's agent keeps the trait's defaults, as before.
    agent: bool,
}

/// Modifier bits of the protocol's input events.
const ALT: u32 = 1;
const CTRL: u32 = 2;
const META: u32 = 4;
const SHIFT: u32 = 8;

/// A key's DOM `key` and `code` names, as the browser's events carry them.
fn dom_key(key: nsh_core::keymap::Key, shift: bool) -> Option<(String, String)> {
    use nsh_core::keymap::Key;
    let named = |key: &str, code: &str| Some((key.to_owned(), code.to_owned()));
    match key {
        Key::Enter => named("Enter", "Enter"),
        Key::Escape => named("Escape", "Escape"),
        Key::Tab => named("Tab", "Tab"),
        Key::Backspace => named("Backspace", "Backspace"),
        Key::Delete => named("Delete", "Delete"),
        Key::Insert => named("Insert", "Insert"),
        Key::Up => named("ArrowUp", "ArrowUp"),
        Key::Down => named("ArrowDown", "ArrowDown"),
        Key::Left => named("ArrowLeft", "ArrowLeft"),
        Key::Right => named("ArrowRight", "ArrowRight"),
        Key::Home => named("Home", "Home"),
        Key::End => named("End", "End"),
        Key::PageUp => named("PageUp", "PageUp"),
        Key::PageDown => named("PageDown", "PageDown"),
        Key::F(n) => Some((format!("F{n}"), format!("F{n}"))),
        Key::Char(' ') => named(" ", "Space"),
        Key::Char(c) => {
            let code = match c {
                'a'..='z' => format!("Key{}", c.to_ascii_uppercase()),
                '0'..='9' => format!("Digit{c}"),
                ';' => "Semicolon".into(),
                '=' => "Equal".into(),
                ',' => "Comma".into(),
                '-' => "Minus".into(),
                '.' => "Period".into(),
                '/' => "Slash".into(),
                '`' => "Backquote".into(),
                '[' => "BracketLeft".into(),
                '\\' => "Backslash".into(),
                ']' => "BracketRight".into(),
                '\'' => "Quote".into(),
                _ => return None,
            };
            let typed = if shift { shifted(c) } else { c };
            Some((typed.to_string(), code))
        }
    }
}

impl WebNative {
    /// The agent's part of `page`, whose questions `server` asks.
    pub(crate) fn new(page: Arc<Page>, server: Arc<DevServer>, agent: bool) -> WebNative {
        WebNative {
            page,
            server,
            agent,
        }
    }

    /// What the page answers of `query`.
    fn query<T: serde::de::DeserializeOwned>(&self, query: SceneQuery) -> Result<T, String> {
        let value = self.server.step(TestStep::Scene { query }, ASK)?;
        serde_json::from_value(value).map_err(|e| format!("the page answered oddly: {e}"))
    }

    fn call(&self, method: &str, params: Value) {
        if let Err(err) = self.page.call(method, params) {
            log::warn!("{err}");
        }
    }

    fn mouse(&self, kind: &str, at: Point<Points>, buttons: u32) {
        self.call(
            "Input.dispatchMouseEvent",
            json!({
                "type": kind,
                "x": at.x.0,
                "y": at.y.0,
                // A move with the button down is a drag of it, as
                // Playwright sends it.
                "button": if kind == "mouseMoved" && buttons == 0 { "none" } else { "left" },
                "buttons": buttons,
                "clickCount": 1,
            }),
        );
    }

    fn click(&self, at: Point<Points>, hold: Duration) {
        self.mouse("mouseMoved", at, 0);
        self.mouse("mousePressed", at, 1);
        std::thread::sleep(hold);
        self.mouse("mouseReleased", at, 0);
    }

    /// A key's press and release. `commands` are the editing commands the
    /// key runs on macOS, where the browser leaves them to the system's
    /// key bindings (none reach it headless), as Playwright sends them.
    fn press_key(
        &self,
        key: &str,
        code: &str,
        vk: u32,
        modifiers: u32,
        commands: &[&str],
        text: Option<(&str, &str)>,
    ) {
        let commands: &[&str] = if cfg!(target_os = "macos") {
            commands
        } else {
            &[]
        };
        let event = |kind: &str| {
            json!({
                "type": kind,
                "key": key,
                "code": code,
                "windowsVirtualKeyCode": vk,
                "modifiers": modifiers,
            })
        };
        let mut down = event("rawKeyDown");
        if key == "Enter" {
            down["type"] = "keyDown".into();
            down["text"] = "\r".into();
            down["unmodifiedText"] = "\r".into();
        }
        if let Some((typed, unmodified)) = text {
            down["type"] = "keyDown".into();
            down["text"] = typed.into();
            down["unmodifiedText"] = unmodified.into();
        }
        if !commands.is_empty() {
            down["commands"] = json!(commands);
        }
        self.call("Input.dispatchKeyEvent", down);
        self.call("Input.dispatchKeyEvent", event("keyUp"));
    }
}

/// `keys` (a keystroke as `nsh/keymap` names it, `mod` the browser's
/// platform's command key) and the DOM key it is sent as. An error for
/// what a browser's keyboard has no key for.
fn browser_keys(keys: &str) -> Result<(nsh_core::keymap::Keystroke, (String, String)), String> {
    let stroke = nsh_core::keymap::Keystroke::parse_with(keys, cfg!(target_os = "macos"))
        .map_err(|e| e.to_string())?;
    let dom = dom_key(stroke.key, stroke.modifiers.shift)
        .ok_or_else(|| format!("{keys:?} has no key on a browser's keyboard"))?;
    Ok((stroke, dom))
}

impl Native for WebNative {
    fn available(&self) -> bool {
        !self.agent
            || (!self.page.closed() && self.page.browser().alive() && self.server.has_tester())
    }

    fn hit_at(&self, at: Point<Points>) -> Result<Option<NodeId>, String> {
        if !self.agent {
            return Err(nsh_testing::agent::NATIVE_HIT_TEST_UNSUPPORTED.into());
        }
        self.query(SceneQuery::HitAt {
            x: at.x.0,
            y: at.y.0,
        })
    }

    fn text_bounds(&self, text: NodeId, start: u32, end: u32) -> Option<Bounds<Points>> {
        let [x, y, width, height] = self
            .query::<Option<[f32; 4]>>(SceneQuery::TextBounds {
                id: text,
                start,
                end,
            })
            .ok()??;
        Some(Bounds::new(
            point(Points(x), Points(y)),
            Size::new(Points(width), Points(height)),
        ))
    }

    fn validate_keys(&self, keys: &str) -> Result<(), String> {
        if !self.agent {
            return Ok(());
        }
        browser_keys(keys).map(drop)
    }

    fn focus_node(&self, id: NodeId) -> Result<(), String> {
        if !self.agent {
            return Err(nsh_testing::agent::NATIVE_FOCUS_UNSUPPORTED.into());
        }
        if self.query::<bool>(SceneQuery::Focus { id })? {
            Ok(())
        } else {
            Err(format!("node {id} cannot receive native focus"))
        }
    }

    fn app_focused(&self) -> bool {
        self.agent && self.query::<bool>(SceneQuery::AppFocused).unwrap_or(false)
    }

    fn bounds(&self, id: NodeId) -> Option<Bounds<Points>> {
        let [x, y, width, height] = self
            .query::<Option<[f32; 4]>>(SceneQuery::Bounds { id })
            .ok()??;
        Some(Bounds::new(
            point(Points(x), Points(y)),
            Size::new(Points(width), Points(height)),
        ))
    }

    fn viewport(&self) -> Size<Points> {
        let [width, height] = self
            .query::<[f32; 2]>(SceneQuery::Viewport)
            .unwrap_or([0.0, 0.0]);
        Size::new(Points(width), Points(height))
    }

    fn press(&self, at: Point<Points>) {
        self.click(at, CLICK);
    }

    fn long_press(&self, at: Point<Points>, hold: Duration) {
        self.click(at, hold);
    }

    fn scroll(&self, at: Point<Points>, delta: Point<Points>) {
        self.mouse("mouseMoved", at, 0);
        let (dx, dy) = (delta.x.0, delta.y.0);
        // A precise touchpad's events, a frame apart, adding up to it.
        let n = (dx.abs().max(dy.abs()) / SCROLL_STEP).ceil().max(1.0) as u32;
        for _ in 0..n {
            self.call(
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mouseWheel",
                    "x": at.x.0,
                    "y": at.y.0,
                    "deltaX": dx / n as f32,
                    "deltaY": dy / n as f32,
                }),
            );
            std::thread::sleep(FRAME);
        }
    }

    fn drag(&self, from: Point<Points>, to: Point<Points>) {
        self.mouse("mouseMoved", from, 0);
        self.mouse("mousePressed", from, 1);
        std::thread::sleep(nsh_testing::agent::LANDING);
        let mut last = from;
        for at in nsh_testing::agent::drag_path(from, to) {
            std::thread::sleep(FRAME);
            self.mouse("mouseMoved", at, 1);
            last = at;
        }
        self.mouse("mouseReleased", last, 0);
    }

    fn mouse(&self) -> bool {
        true
    }

    fn right_click(&self, at: Point<Points>) -> Result<(), String> {
        self.mouse("mouseMoved", at, 0);
        for kind in ["mousePressed", "mouseReleased"] {
            self.call(
                "Input.dispatchMouseEvent",
                json!({
                    "type": kind,
                    "x": at.x.0,
                    "y": at.y.0,
                    "button": "right",
                    "buttons": if kind == "mousePressed" { 2 } else { 0 },
                    "clickCount": 1,
                }),
            );
        }
        Ok(())
    }

    fn hover(&self, at: Point<Points>) -> Result<(), String> {
        // From next to it, as a mouse arrives: the browser reports no move
        // to where the mouse already is (scrolling into view can leave it
        // there, the page moving under it).
        let near = Point::new(at.x - Points(1.0), at.y - Points(1.0));
        self.mouse("mouseMoved", near, 0);
        self.mouse("mouseMoved", at, 0);
        Ok(())
    }

    fn press_keys(&self, keys: &str) -> Result<(), String> {
        let (stroke, (key, code)) = browser_keys(keys)?;
        let m = stroke.modifiers;
        let vk = u32::from(stroke.key.virtual_key().unwrap_or(0));
        let mut modifiers = 0;
        for (on, bit) in [
            (m.alt, ALT),
            (m.ctrl, CTRL),
            (m.cmd, META),
            (m.shift, SHIFT),
        ] {
            if on {
                modifiers |= bit;
            }
        }
        // A character's key types it, with its key's code and modifiers,
        // so a shortcut can take it before a focused input does.
        let unmodified = match stroke.key {
            nsh_core::keymap::Key::Char(c) if !(m.ctrl || m.alt || m.cmd) => Some(c.to_string()),
            _ => None,
        };
        self.press_key(
            &key,
            &code,
            vk,
            modifiers,
            &[],
            unmodified.as_deref().map(|c| (key.as_str(), c)),
        );
        Ok(())
    }

    fn type_text(&self, text: &str) {
        for ch in text.chars() {
            let ch = ch.to_string();
            self.call(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyDown", "key": ch, "text": ch, "unmodifiedText": ch }),
            );
            self.call(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyUp", "key": ch }),
            );
        }
    }

    fn key(&self, key: Key) {
        match key {
            Key::Return => self.press_key("Enter", "Enter", 13, 0, &[], None),
            Key::Backspace => {
                self.press_key("Backspace", "Backspace", 8, 0, &["deleteBackward"], None)
            }
            Key::SelectAll => {
                // The platform's command key: Command on macOS, else Ctrl.
                let (name, code, vk, bit) = if cfg!(target_os = "macos") {
                    ("Meta", "MetaLeft", 91, META)
                } else {
                    ("Control", "ControlLeft", 17, CTRL)
                };
                let modifier = |kind: &str| {
                    json!({
                        "type": kind,
                        "key": name,
                        "code": code,
                        "windowsVirtualKeyCode": vk,
                        "modifiers": if kind == "keyUp" { 0 } else { bit },
                    })
                };
                self.call("Input.dispatchKeyEvent", modifier("rawKeyDown"));
                self.press_key("a", "KeyA", 65, bit, &["selectAll"], None);
                self.call("Input.dispatchKeyEvent", modifier("keyUp"));
            }
        }
    }

    fn input_text(&self, id: NodeId) -> Option<String> {
        self.query::<Option<String>>(SceneQuery::InputText { id })
            .ok()
            .flatten()
    }

    fn back(&self) -> Result<(), String> {
        // The browser's back button: the page's history, which the app's
        // navigation keeps.
        self.page
            .call(
                "Runtime.evaluate",
                json!({ "expression": "history.back()" }),
            )
            .map(drop)
    }

    fn answer_dialog(&self, button: Option<&str>) -> Result<(), String> {
        let [x, y] = self.query::<[f32; 2]>(SceneQuery::DialogPoint {
            button: button.map(str::to_owned),
        })?;
        self.click(point(Points(x), Points(y)), CLICK);
        Ok(())
    }

    fn animating(&self) -> bool {
        self.query::<bool>(SceneQuery::Animating).unwrap_or(false)
    }

    fn screenshot(&self) -> Result<Vec<u8>, String> {
        use base64::Engine as _;
        let shot = self
            .page
            .call("Page.captureScreenshot", json!({ "format": "png" }))?;
        let data = shot["data"]
            .as_str()
            .ok_or("the browser took no screenshot")?;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("the browser's screenshot: {e}"))
    }

    fn resize(&self, width: Points, height: Points) -> Result<(), String> {
        set_size(&self.page, width, height)
    }

    fn set_dark(&self, dark: bool) -> Result<(), String> {
        self.page
            .call(
                "Emulation.setEmulatedMedia",
                json!({
                    "features": [{
                        "name": "prefers-color-scheme",
                        "value": if dark { "dark" } else { "light" },
                    }],
                }),
            )
            .map(drop)
    }

    fn quit(&self) {
        self.page.close();
    }
}

#[cfg(test)]
mod tests {
    use super::browser_keys;

    #[test]
    fn keys_a_browser_has_are_sent_as_its_keys() {
        let (_, dom) = browser_keys("mod+b").unwrap();
        assert_eq!(dom, ("b".into(), "KeyB".into()));
        let (_, dom) = browser_keys("shift+enter").unwrap();
        assert_eq!(dom, ("Enter".into(), "Enter".into()));
        // A character is pressed on its key, shifted as the US layout does.
        assert_eq!(browser_keys("x").unwrap().1, ("x".into(), "KeyX".into()));
        assert_eq!(
            browser_keys("shift+8").unwrap().1,
            ("*".into(), "Digit8".into())
        );
        // No key of a browser's keyboard makes it.
        assert!(browser_keys("ctrl+é").is_err());
        assert!(browser_keys("nonsense+++").is_err());
    }
}
