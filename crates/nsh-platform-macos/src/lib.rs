//! macOS backend for nsh: AppKit views for nodes, CoreText for text.
//!
//! - Each node is an [`view::NshView`] (a flipped, layer-backed `NSView`
//!   subclass) at the frame the core laid out; Core Animation draws its
//!   background, borders, shadows and images, and its text and canvas
//!   drawings go into a content layer of its own (`drawLayer:inContext:`);
//!   text fields, web views and videos are AppKit views hosted in their
//!   node's view.
//! - The engine runs on its own thread ([`nsh_engine::Engine`]); each
//!   commit is one hop to the main queue ([`surface`]).
//! - Text is typeset by CoreText on the engine thread; the main thread
//!   only draws the lines it produced ([`text`]).
//! - Native animations run on the root view's display link
//!   ([`animation`]).
//! - Desktop input as the Linux backend has it: the root view takes the
//!   mouse, wheel, trackpad and keyboard and picks views itself
//!   ([`input`]); scroll views keep offsets of their own ([`scroll`]).
//!
//! [`AppView`] is one surface as a view (for embedding); [`run_app`]
//! opens one in a window. Off macOS this crate is empty.

#![cfg(target_os = "macos")]

mod access;
#[cfg(feature = "dev")]
mod agent;
mod animation;
mod app;
mod blur;
mod canvas;
mod chrome;
mod decor;
#[cfg(feature = "dev")]
pub mod dev;
mod file_drop;
mod file_requests;
mod fps;
mod global_shortcut;
mod gpu;
// Without the server only `AppView::ui_tree` reads it, and nothing acts yet.
#[cfg(feature = "gpui-agent")]
#[cfg_attr(not(feature = "gpui-agent-server"), allow(dead_code, unused_imports))]
mod gpui_agent;
mod image;
mod input;
mod keymap;
mod mount;
mod native;
mod navigation;
mod net;
mod picker;
mod provenance;
mod screen_color;
mod scroll;
mod selectable_view;
mod shader;
mod sight;
// Shared with iOS.
use nsh_platform_apple::{audio, notifications, speech};
mod surface;
// The test agent and the gpui-agent host are synth's only callers; the
// helpers it uses in input are gated the same way.
#[cfg(any(feature = "dev", feature = "gpui-agent"))]
mod synth;
mod system;
mod text;
mod text_input;
mod transition;
mod tray;
mod view;
mod visual;
mod window;
mod window_state;

#[cfg(feature = "gpui-agent")]
pub use ::gpui_agent::tree as ui_tree;
pub use block2;
pub use keymap::keystroke as keystroke_of;
pub use nsh_core::platform::{Appearance, Locale};
pub use nsh_engine::{
    App, Bundle, ByteSize, CacheLocation, CacheOptions, CachePolicy, Limits, Root, perf,
};
pub use objc2;
pub use objc2_app_kit;
pub use objc2_foundation;

#[cfg(feature = "dev")]
pub use crate::app::set_root;
pub use crate::{
    app::{
        ActivationPolicy, AppOptions, AppSource, AppView, WindowDisplay, quit_app, run_app,
        set_components, set_ui_sets,
    },
    image::AssetBase,
    keymap::KeyPrecedence,
    native::{ComponentContext, ComponentFactory, Components, NativeComponent},
    provenance::{Input, on_input},
    text::CoreTextEngine,
};
