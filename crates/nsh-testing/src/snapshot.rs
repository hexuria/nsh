//! Owned wire data for a mounted screen, without runtime objects or menu images.

use nsh_core::{
    AccessibilityCurrent, Checked, EventKind, NodeId, ViewKind,
    desktop::{MenuItem, Shortcut},
    element::PropEnum,
    keymap::{KeyBinding, When},
};
use serde::{Deserialize, Serialize};

use crate::Screen;

/// The mounted views and application commands at one instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct ScreenSnapshot {
    /// The surface's root id.
    pub root: NodeId,
    /// Mounted views, including the root, in id order; closed views are omitted.
    pub views: Vec<ViewSnapshot>,
    /// The installed keymap, oldest binding first; `None` when not installed.
    pub keymap: Option<Vec<KeyBindingSnapshot>>,
    /// The installed application menu, without icons or symbols.
    pub app_menu: Option<Vec<MenuItemSnapshot>>,
}

/// A mounted view's properties and its ordered hierarchy links.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct ViewSnapshot {
    /// The view id.
    pub id: NodeId,
    /// Its parent's id, if inserted.
    pub parent: Option<NodeId>,
    /// Children in mounting order, bottom to top.
    pub children: Vec<NodeId>,
    /// Stack screens, bottom to top, excluding screens popped natively.
    pub stack_screens: Vec<NodeId>,
    /// The kind's wire name: view, text, image, scroll, input, stack, screen,
    /// modal, header, accessory, canvas, web, native or video; `None` for the root.
    pub kind: Option<String>,
    /// Listener names as returned by `EventKind::prop_name`.
    pub events: Vec<String>,
    /// Its `testID`.
    pub test_id: Option<String>,
    /// Its accessibility label.
    pub label: Option<String>,
    /// The text a text view shows.
    pub text: Option<String>,
    /// Its parent-relative frame `[x, y, width, height]` in points.
    pub frame: [f32; 4],
    /// The text an input holds, unmodified, including secure inputs.
    pub input_text: Option<String>,
    /// Whether the input's text is masked by the platform.
    pub secure: bool,
    /// Whether it scrolls its content.
    pub scrolls: bool,
    /// Whether it clips its content.
    pub clips: bool,
    /// Its hidden property.
    pub hidden: bool,
    /// Its disabled property.
    pub disabled: bool,
    /// Its selected property.
    pub selected: bool,
    /// The role's JS property name.
    pub role: String,
    /// Its accessibility state, without inherited state.
    pub accessibility_state: AccessibilityStateSnapshot,
    /// Its accessibility hint.
    pub accessibility_hint: Option<String>,
    /// Its range or textual accessibility value.
    pub accessibility_value: Option<AccessibilityValueSnapshot>,
    /// Its explicit accessible property.
    pub accessible: Option<bool>,
    /// Whether it hides its subtree from assistive technologies.
    pub accessibility_hidden: bool,
    /// Whether this is a canvas node rather than a platform view.
    pub canvas_node: bool,
    /// The text span this node represents, if any.
    pub span: Option<SpanSnapshot>,
}

/// A text span and its offsets in its containing text view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct SpanSnapshot {
    /// The containing text view.
    pub view: NodeId,
    /// The span's characters.
    pub text: String,
    /// The starting UTF-16 offset.
    pub start: u32,
    /// The exclusive ending UTF-16 offset.
    pub end: u32,
}

/// A view's explicitly mounted accessibility state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct AccessibilityStateSnapshot {
    /// Disabled.
    pub disabled: bool,
    /// Selected.
    pub selected: bool,
    /// Checked state: `false`, `true` or `mixed`.
    pub checked: Option<String>,
    /// Busy.
    pub busy: bool,
    /// Expanded or collapsed.
    pub expanded: Option<bool>,
    /// Toggle state: `false`, `true` or `mixed`.
    pub pressed: Option<String>,
    /// Current item: `true`, `page`, `step`, `location`, `date` or `time`.
    pub current: Option<String>,
    /// Opens a popup.
    pub has_popup: bool,
    /// Invalid content.
    pub invalid: bool,
}

/// A mounted range or textual accessibility value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct AccessibilityValueSnapshot {
    /// The minimum value.
    pub min: Option<f32>,
    /// The maximum value.
    pub max: Option<f32>,
    /// The current value.
    pub now: Option<f32>,
    /// Its textual description.
    pub text: Option<String>,
}

/// One installed keyboard binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct KeyBindingSnapshot {
    /// Its command id.
    pub id: u32,
    /// Canonical `Keystroke` text, with literal modifiers, not `mod`.
    pub keystroke: String,
    /// Fires while a text input has focus.
    pub in_text: bool,
    /// Fires on auto-repeat.
    pub repeat: bool,
    /// Lets the keystroke pass on after firing.
    pub pass_through: bool,
    /// The window's root id, or any window.
    pub window: Option<NodeId>,
    /// Belongs to the focused view.
    pub for_focus: bool,
}

/// The non-visual data of an application menu item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum MenuItemSnapshot {
    /// A separator between items.
    Separator,
    /// An action the user can choose.
    Action {
        /// Its command id.
        id: u32,
        /// Its explicit label.
        label: String,
        /// The shortcut, without a platform-specific label.
        shortcut: Option<ShortcutSnapshot>,
        /// Whether it can be chosen.
        enabled: bool,
        /// Whether it is checkable and checked.
        checked: Option<bool>,
        /// The menu role's JS property name.
        role: Option<String>,
        /// Shortcut policy: `auto`, `always` or `not_editing`.
        when: String,
        /// Whether it is a destructive action.
        destructive: bool,
    },
    /// A submenu or inline section.
    Submenu {
        /// Its label.
        label: String,
        /// Whether it can be opened.
        enabled: bool,
        /// Its ordered items.
        items: Vec<MenuItemSnapshot>,
        /// Whether it is an inline section.
        inline: bool,
    },
}

/// A menu shortcut's key and literal modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct ShortcutSnapshot {
    /// The key's name from `desktop::Key::label`, such as `A`, `Enter` or `F5`.
    pub key: String,
    /// Control.
    pub ctrl: bool,
    /// Alt / Option.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Command / Super.
    pub meta: bool,
}

impl ViewSnapshot {
    pub(crate) fn read(screen: &Screen, id: NodeId) -> Option<Self> {
        let v = screen.mounted(id)?;
        let frame = v.frame();
        let state = v.accessibility_state();
        let checked = |c| {
            match c {
                Checked::False => "false",
                Checked::True => "true",
                Checked::Mixed => "mixed",
            }
            .to_owned()
        };
        Some(Self {
            id,
            parent: v.parent(),
            children: screen.children(id).to_vec(),
            stack_screens: if v.kind() == Some(ViewKind::Stack) {
                screen.stack_screens(id)
            } else {
                Vec::new()
            },
            kind: v.kind().map(|k| {
                match k {
                    ViewKind::View => "view",
                    ViewKind::Text => "text",
                    ViewKind::Image => "image",
                    ViewKind::Scroll => "scroll",
                    ViewKind::Input => "input",
                    ViewKind::Stack => "stack",
                    ViewKind::Screen => "screen",
                    ViewKind::Modal => "modal",
                    ViewKind::HeaderItem => "header",
                    ViewKind::InputAccessory => "accessory",
                    ViewKind::Canvas => "canvas",
                    ViewKind::WebView => "web",
                    ViewKind::Native => "native",
                    ViewKind::Video => "video",
                }
                .to_owned()
            }),
            events: EventKind::ALL
                .iter()
                .filter(|&&e| v.events().contains(e))
                .map(|e| e.prop_name().to_owned())
                .collect(),
            test_id: v.test_id().map(str::to_owned),
            label: v.label().map(str::to_owned),
            text: v.text().map(str::to_owned),
            frame: [
                frame.origin.x.0,
                frame.origin.y.0,
                frame.size.width.0,
                frame.size.height.0,
            ],
            input_text: v.input_text().map(str::to_owned),
            secure: v.secure(),
            scrolls: v.scrolls(),
            clips: v.clips(),
            hidden: v.hidden(),
            disabled: v.disabled(),
            selected: v.selected(),
            role: v.role().js_name().to_owned(),
            accessibility_state: AccessibilityStateSnapshot {
                disabled: state.disabled,
                selected: state.selected,
                checked: state.checked.map(checked),
                busy: state.busy,
                expanded: state.expanded,
                pressed: state.pressed.map(checked),
                current: state.current.map(|c| {
                    match c {
                        AccessibilityCurrent::True => "true",
                        AccessibilityCurrent::Page => "page",
                        AccessibilityCurrent::Step => "step",
                        AccessibilityCurrent::Location => "location",
                        AccessibilityCurrent::Date => "date",
                        AccessibilityCurrent::Time => "time",
                    }
                    .to_owned()
                }),
                has_popup: state.has_popup,
                invalid: state.invalid,
            },
            accessibility_hint: v.accessibility_hint().map(str::to_owned),
            accessibility_value: v.accessibility_value().map(|a| AccessibilityValueSnapshot {
                min: a.min,
                max: a.max,
                now: a.now,
                text: a.text.as_deref().map(str::to_owned),
            }),
            accessible: v.accessible(),
            accessibility_hidden: v.accessibility_hidden(),
            canvas_node: v.canvas_node(),
            span: v.span().map(|s| SpanSnapshot {
                view: s.view,
                text: s.text.to_owned(),
                start: s.start,
                end: s.end,
            }),
        })
    }
}

impl From<&KeyBinding> for KeyBindingSnapshot {
    fn from(k: &KeyBinding) -> Self {
        Self {
            id: k.id,
            keystroke: k.keystroke.to_string(),
            in_text: k.in_text,
            repeat: k.repeat,
            pass_through: k.pass_through,
            window: k.window,
            for_focus: k.for_focus,
        }
    }
}

impl From<&Shortcut> for ShortcutSnapshot {
    fn from(s: &Shortcut) -> Self {
        Self {
            key: s.key.label(),
            ctrl: s.ctrl,
            alt: s.alt,
            shift: s.shift,
            meta: s.meta,
        }
    }
}

impl From<&MenuItem> for MenuItemSnapshot {
    fn from(item: &MenuItem) -> Self {
        match item {
            MenuItem::Separator => Self::Separator,
            MenuItem::Action(a) => Self::Action {
                id: a.id,
                label: a.label.clone(),
                shortcut: a.shortcut.as_ref().map(ShortcutSnapshot::from),
                enabled: a.enabled,
                checked: a.checked,
                role: a.role.map(|r| r.js_name().to_owned()),
                when: match a.when {
                    When::Auto => "auto",
                    When::Always => "always",
                    When::NotEditing => "not_editing",
                }
                .to_owned(),
                destructive: a.destructive,
            },
            MenuItem::Submenu {
                label,
                enabled,
                items,
                inline,
                ..
            } => Self::Submenu {
                label: label.clone(),
                enabled: *enabled,
                items: items.iter().map(Self::from).collect(),
                inline: *inline,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nsh_core::{
        AccessibilityRole, AccessibilityState, AccessibilityValue, AttributedText, Bounds, Command,
        EventMask, InputConfig, MountBatch, MountItem, Points, TextLayout, TextLink, ViewProps,
        canvas::{CanvasNode, Drawing},
        desktop::{DesktopCommand, MenuAction, MenuRole},
        geometry::{point, size},
        keymap::{Keymap, Keystroke},
    };

    use super::*;

    #[test]
    fn snapshots_copy_mounted_data_without_interpreting_it() {
        let mut screen = Screen::default();
        let mut events = EventMask::default();
        events.set(EventKind::Press, true);
        events.set(EventKind::KeyCommand, true);
        let mut props = ViewProps {
            test_id: Some("input".into()),
            ..Default::default()
        };
        props.accessibility.label = Some("Label".into());
        props.accessibility.hint = Some("Hint".into());
        props.accessibility.accessible = Some(false);
        props.accessibility.hidden = true;
        props.accessibility.role = AccessibilityRole::Search;
        props.accessibility.state = AccessibilityState {
            disabled: true,
            selected: true,
            checked: Some(Checked::Mixed),
            busy: true,
            expanded: Some(false),
            pressed: Some(Checked::False),
            current: Some(AccessibilityCurrent::Page),
            has_popup: true,
            invalid: true,
        };
        props.accessibility.value = Some(AccessibilityValue {
            min: Some(1.0),
            max: Some(9.5),
            now: Some(2.25),
            text: Some("Value".into()),
        });
        let frame = Bounds::new(
            point(Points(1.5), Points(-2.0)),
            size(Points(80.0), Points(32.25)),
        );
        let config = InputConfig {
            secure: true,
            ..Default::default()
        };
        let binding = KeyBinding::new(
            7,
            Keystroke::parse_with("ctrl+shift+k", false).unwrap(),
            When::Always,
            true,
        )
        .in_window(1);
        let mut items = vec![];
        for (id, kind, parent) in [
            (2, ViewKind::Stack, 1),
            (3, ViewKind::Screen, 2),
            (4, ViewKind::Screen, 2),
            (5, ViewKind::Input, 3),
            (6, ViewKind::Text, 3),
            (7, ViewKind::Canvas, 3),
            (8, ViewKind::Scroll, 3),
            (9, ViewKind::Modal, 1),
        ] {
            items.push(MountItem::Create { id, kind });
            items.push(MountItem::Insert {
                parent,
                child: id,
                before: None,
            });
        }
        items.extend([
            MountItem::ViewProps {
                id: 5,
                props: Box::new(props),
            },
            MountItem::Frame { id: 5, frame },
            MountItem::Events { id: 5, events },
            MountItem::ViewStyle {
                id: 5,
                style: Box::new(nsh_core::ViewStyle {
                    hidden: true,
                    overflow_hidden: true,
                    ..Default::default()
                }),
            },
            MountItem::Input {
                id: 5,
                config: Box::new(config),
            },
            MountItem::InputValue {
                id: 5,
                text: "private🙂".into(),
                count: 0,
            },
            MountItem::Text {
                id: 6,
                text: Arc::new(AttributedText {
                    text: "a🙂b".into(),
                    links: vec![TextLink {
                        start: 1,
                        end: 3,
                        node: 100,
                        events,
                        focus_stop: true,
                    }],
                    ..Default::default()
                }),
                layout: TextLayout(Arc::new(())),
                content: frame,
            },
            MountItem::Canvas {
                id: 7,
                drawing: Arc::new(Drawing {
                    nodes: vec![CanvasNode::new(frame, AccessibilityRole::Button, "Cell")],
                    ..Default::default()
                }),
            },
            MountItem::Keymap(Arc::new(Keymap::new(vec![binding]))),
            MountItem::Create {
                id: 50,
                kind: ViewKind::View,
            },
            MountItem::Delete { id: 50 },
            MountItem::Create {
                id: 51,
                kind: ViewKind::View,
            },
        ]);
        screen.apply(&MountBatch { items });
        screen.pop(4);
        screen.close(9);
        let snapshot = screen.snapshot();
        let view = |id| snapshot.views.iter().find(|v| v.id == id).unwrap();
        assert_eq!(snapshot.root, 1);
        assert_eq!(view(1).kind, None);
        assert_eq!(view(2).children, [3, 4]);
        assert_eq!(view(2).stack_screens, [3]);
        assert!(snapshot.views.iter().all(|v| v.id != 9 && v.id != 50));
        assert_eq!(view(51).parent, None);
        let input = view(5);
        assert_eq!(input.parent, Some(3));
        assert_eq!(input.frame, [1.5, -2.0, 80.0, 32.25]);
        assert_eq!(input.input_text.as_deref(), Some("private🙂"));
        assert!(input.secure && input.disabled && input.selected && input.accessibility_hidden);
        assert!(input.hidden && input.clips && !input.scrolls);
        assert!(view(8).scrolls);
        assert_eq!(input.kind.as_deref(), Some("input"));
        assert_eq!(input.test_id.as_deref(), Some("input"));
        assert_eq!(input.label.as_deref(), Some("Label"));
        assert_eq!(input.role, "search");
        assert_eq!(input.accessible, Some(false));
        assert_eq!(input.accessibility_hint.as_deref(), Some("Hint"));
        assert_eq!(input.accessibility_state.checked.as_deref(), Some("mixed"));
        assert_eq!(input.accessibility_state.pressed.as_deref(), Some("false"));
        assert_eq!(input.accessibility_state.current.as_deref(), Some("page"));
        assert!(
            input.accessibility_state.busy
                && input.accessibility_state.has_popup
                && input.accessibility_state.invalid
        );
        assert_eq!(input.accessibility_value.as_ref().unwrap().now, Some(2.25));
        assert_eq!(
            input.events,
            [
                EventKind::Press.prop_name(),
                EventKind::KeyCommand.prop_name()
            ]
        );
        assert_eq!(view(6).text.as_deref(), Some("a🙂b"));
        assert_eq!(
            view(100).span,
            Some(SpanSnapshot {
                view: 6,
                text: "🙂".into(),
                start: 1,
                end: 3
            })
        );
        assert!(
            snapshot.views.iter().any(|v| v.canvas_node
                && v.parent == Some(7)
                && v.label.as_deref() == Some("Cell"))
        );
        let keys = snapshot.keymap.as_ref().unwrap();
        assert_eq!(keys[0].keystroke, "ctrl+shift+k");
        assert_eq!(keys[0].id, 7);
        assert_eq!(keys[0].window, Some(1));
        assert!(keys[0].in_text && keys[0].repeat);
        assert_eq!(
            serde_json::from_value::<ScreenSnapshot>(serde_json::to_value(&snapshot).unwrap())
                .unwrap(),
            snapshot
        );
        screen.apply(&MountBatch {
            items: vec![MountItem::Delete { id: 5 }],
        });
        assert_eq!(input.input_text.as_deref(), Some("private🙂"));
    }

    #[test]
    fn menus_keep_commands_and_shortcuts_but_no_images() {
        let menu = vec![MenuItem::Submenu {
            label: "File".into(),
            enabled: false,
            inline: true,
            icon: Some(nsh_core::ImageSource {
                uri: "asset:/icon.png".into(),
                ..Default::default()
            }),
            symbol: Some("folder".into()),
            items: vec![
                MenuItem::Separator,
                MenuItem::Action(MenuAction {
                    id: 14,
                    label: "Quit".into(),
                    shortcut: Some(Shortcut::parse("cmd+q", true).unwrap()),
                    enabled: true,
                    checked: Some(false),
                    role: Some(MenuRole::Quit),
                    when: When::NotEditing,
                    icon: Some(nsh_core::ImageSource {
                        uri: "asset:/quit.png".into(),
                        ..Default::default()
                    }),
                    symbol: Some("quit".into()),
                    destructive: true,
                }),
            ],
        }];
        let mut screen = Screen::default();
        screen.apply(&MountBatch {
            items: vec![MountItem::Command {
                id: 1,
                command: Command::Desktop(Box::new(DesktopCommand::SetMenu(Some(menu.into())))),
            }],
        });
        let snapshot = screen.snapshot();
        let MenuItemSnapshot::Submenu {
            label,
            enabled,
            items,
            inline,
        } = &snapshot.app_menu.as_ref().unwrap()[0]
        else {
            panic!("submenu")
        };
        assert_eq!(label, "File");
        assert!(!enabled && *inline);
        assert!(matches!(items[0], MenuItemSnapshot::Separator));
        assert!(
            matches!(&items[1], MenuItemSnapshot::Action { id: 14, shortcut: Some(s), role: Some(r), when, destructive: true, .. } if s.key == "Q" && s.meta && r == "quit" && when == "not_editing")
        );
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("icon") && !json.contains("symbol") && !json.contains("asset:"));
        assert_eq!(
            serde_json::from_str::<ScreenSnapshot>(&json).unwrap(),
            snapshot
        );
    }

    #[cfg(feature = "agent")]
    #[test]
    fn snapshots_answer_locally_and_remotely_and_reject_bad_replies() {
        use crate::agent::{Model, RemoteScene, answer};
        use nsh_dev::protocol::{SceneQuery, TestStep};
        let model = Arc::new(Model::default());
        let expected = model.screen(Screen::snapshot);
        assert_eq!(expected.views.len(), 1);
        assert!(expected.keymap.is_none() && expected.app_menu.is_none());
        let source = model.clone();
        let remote = RemoteScene::new(move |step| {
            assert_eq!(
                step,
                TestStep::Scene {
                    query: SceneQuery::ScreenSnapshot
                }
            );
            answer(&source, &step).unwrap()
        });
        assert_eq!(remote.screen_snapshot().unwrap(), expected);
        model.apply(&MountBatch {
            items: vec![
                MountItem::Create {
                    id: 2,
                    kind: ViewKind::View,
                },
                MountItem::Insert {
                    parent: 1,
                    child: 2,
                    before: None,
                },
            ],
        });
        assert_eq!(remote.screen_snapshot().unwrap().views.len(), 2);
        model.reset();
        assert_eq!(remote.screen_snapshot().unwrap(), expected);
        let bad = RemoteScene::new(|_| Ok(serde_json::Value::Null));
        assert!(
            bad.screen_snapshot()
                .unwrap_err()
                .starts_with("the host answered oddly:")
        );
        let failed = RemoteScene::new(|_| Err("disconnected".into()));
        assert_eq!(failed.screen_snapshot().unwrap_err(), "disconnected");
    }
}
