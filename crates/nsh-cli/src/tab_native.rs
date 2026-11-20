use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use nsh_core::{
    NodeId, PasteEvent,
    geometry::{Bounds, Point, Points, Size},
};
use nsh_dev::{
    DevServer,
    protocol::{SynthEvent, SynthKeyPhase, SynthPointerPhase, TestStep},
};
use nsh_testing::agent::{Key, Native, drag_path};

use crate::web_native::{ASK, WebScene, browser_keys};

const FRAME: Duration = Duration::from_millis(16);
const CLICK: Duration = Duration::from_millis(60);
const SCROLL_STEP: f32 = 20.0;
const UNSUPPORTED: &str = "is not available in a tab nsh does not control";

pub(crate) struct TabNative {
    server: Arc<DevServer>,
    scene: WebScene,
    failure: Mutex<Option<String>>,
}

impl TabNative {
    pub(crate) fn new(server: Arc<DevServer>) -> Self {
        Self {
            scene: WebScene::new(server.clone(), true),
            server,
            failure: Mutex::new(None),
        }
    }

    fn send(&self, event: SynthEvent) -> Result<(), String> {
        self.server
            .step(TestStep::Synth { event }, ASK)
            .map(|_| ())
            .map_err(|error| {
                if !self.server.has_tester() {
                    "no tab has opened the agent's page yet".into()
                } else {
                    error
                }
            })
    }

    fn record(&self, result: Result<(), String>) {
        if let Err(error) = result {
            *self.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
        }
    }

    fn pointer(
        &self,
        phase: SynthPointerPhase,
        at: Point<Points>,
        buttons: u32,
    ) -> Result<(), String> {
        self.send(SynthEvent::Pointer {
            phase,
            x: at.x.0,
            y: at.y.0,
            buttons,
        })
    }

    fn click(&self, at: Point<Points>, hold: Duration) -> Result<(), String> {
        self.pointer(SynthPointerPhase::Move, at, 0)?;
        self.pointer(SynthPointerPhase::Down, at, 1)?;
        thread::sleep(hold);
        self.pointer(SynthPointerPhase::Up, at, 0)
    }

    fn key_event(
        &self,
        phase: SynthKeyPhase,
        key: String,
        code: String,
        modifiers: nsh_core::keymap::Modifiers,
    ) -> Result<(), String> {
        self.send(SynthEvent::Key {
            phase,
            key,
            code,
            shift: modifiers.shift,
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            meta: modifiers.cmd,
        })
    }
}

impl Native for TabNative {
    fn available(&self) -> bool {
        self.server.has_tester()
    }

    fn hit_at(&self, at: Point<Points>) -> Result<Option<NodeId>, String> {
        self.scene.hit_at(at)
    }

    fn bounds(&self, id: NodeId) -> Option<Bounds<Points>> {
        self.scene.bounds(id)
    }

    fn text_bounds(&self, id: NodeId, start: u32, end: u32) -> Option<Bounds<Points>> {
        self.scene.text_bounds(id, start, end)
    }

    fn viewport(&self) -> Size<Points> {
        self.scene.viewport()
    }

    fn press(&self, at: Point<Points>) {
        self.record(self.click(at, CLICK));
    }

    fn long_press(&self, at: Point<Points>, hold: Duration) {
        self.record(self.click(at, hold));
    }

    fn scroll(&self, at: Point<Points>, delta: Point<Points>) {
        self.record((|| {
            self.pointer(SynthPointerPhase::Move, at, 0)?;
            let n = (delta.x.0.abs().max(delta.y.0.abs()) / SCROLL_STEP)
                .ceil()
                .max(1.0) as u32;
            for _ in 0..n {
                self.send(SynthEvent::Wheel {
                    x: at.x.0,
                    y: at.y.0,
                    dx: delta.x.0 / n as f32,
                    dy: delta.y.0 / n as f32,
                })?;
                thread::sleep(FRAME);
            }
            Ok(())
        })());
    }

    fn drag(&self, from: Point<Points>, to: Point<Points>) {
        let mut last = from;
        let result = (|| {
            self.pointer(SynthPointerPhase::Move, from, 0)?;
            self.pointer(SynthPointerPhase::Down, from, 1)?;
            thread::sleep(nsh_testing::agent::LANDING);
            for at in drag_path(from, to) {
                thread::sleep(FRAME);
                self.pointer(SynthPointerPhase::Move, at, 1)?;
                last = at;
            }
            self.pointer(SynthPointerPhase::Up, last, 0)
        })();
        if result.is_err() {
            let _ = self.pointer(SynthPointerPhase::Up, last, 0);
        }
        self.record(result);
    }

    fn mouse(&self) -> bool {
        true
    }

    fn hover(&self, at: Point<Points>) -> Result<(), String> {
        self.pointer(SynthPointerPhase::Move, at, 0)
    }

    fn focused(&self) -> Option<NodeId> {
        self.scene.focused()
    }

    fn paste(&self, content: &PasteEvent) -> Result<(), String> {
        if !content.html.is_empty() || !content.files.is_empty() || !content.uris.is_empty() {
            return Err("synthetic paste supports plain text only".into());
        }
        self.send(SynthEvent::InsertText {
            text: content.text.clone(),
        })
    }

    fn press_keys(&self, keys: &str) -> Result<(), String> {
        let (stroke, (key, code)) = browser_keys(keys)?;
        self.key_event(
            SynthKeyPhase::Down,
            key.clone(),
            code.clone(),
            stroke.modifiers,
        )?;
        self.key_event(SynthKeyPhase::Up, key, code, stroke.modifiers)
    }

    fn validate_keys(&self, keys: &str) -> Result<(), String> {
        browser_keys(keys).map(drop)
    }

    fn focus_node(&self, id: NodeId) -> Result<(), String> {
        self.send(SynthEvent::Focus { node: id })
    }

    fn app_focused(&self) -> bool {
        self.scene.app_focused()
    }

    fn type_text(&self, text: &str) {
        self.record(self.send(SynthEvent::InsertText {
            text: text.to_owned(),
        }));
    }

    fn key(&self, key: Key) {
        let command = if cfg!(target_os = "macos") {
            "mod"
        } else {
            "ctrl"
        };
        let result = match key {
            Key::Return => self.press_keys("enter"),
            Key::Backspace => self.press_keys("backspace"),
            Key::SelectAll => self.press_keys(&format!("{command}+a")),
        };
        self.record(result);
    }

    fn input_text(&self, id: NodeId) -> Option<String> {
        self.scene.input_text(id)
    }

    fn back(&self) -> Result<(), String> {
        self.send(SynthEvent::Back)
    }

    fn forward(&self) -> Result<(), String> {
        Err(format!("forward {UNSUPPORTED}"))
    }

    fn right_click(&self, _: Point<Points>) -> Result<(), String> {
        Err(format!("right_click {UNSUPPORTED}"))
    }

    fn answer_dialog(&self, _: Option<&str>) -> Result<(), String> {
        Err(format!("answer_dialog {UNSUPPORTED}"))
    }

    fn animating(&self) -> bool {
        self.scene.animating()
    }

    fn screenshot(&self) -> Result<Vec<u8>, String> {
        Err(format!("screenshot {UNSUPPORTED}"))
    }

    fn resize(&self, _: Points, _: Points) -> Result<(), String> {
        Err(format!("resize {UNSUPPORTED}"))
    }

    fn set_dark(&self, _: bool) -> Result<(), String> {
        Err(format!("set_dark {UNSUPPORTED}"))
    }

    fn quit(&self) {}

    fn take_failure(&self) -> Option<String> {
        self.failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}
