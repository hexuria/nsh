//! Native adapter checks: run on the process main thread with a desktop session.
//! GPUI_AGENT=0 NSH_NATIVE_AGENT_CHECK_ONLY=1 cargo test -p nsh-platform-macos --features dev-agent --test native -- --ignored

use std::{
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use nsh_core::{NodeId, ROOT_ID, element::desktop::window, prelude::*};
use nsh_dev::protocol::{StepOutcome, TestStep};
use nsh_platform_macos::{
    self as platform,
    objc2::{MainThreadMarker, MainThreadOnly, Message, rc::Retained},
    objc2_app_kit::{
        NSAccessibility, NSApplication, NSApplicationActivationPolicy, NSBackingStoreType,
        NSEventMask, NSView, NSWindow, NSWindowStyleMask,
    },
    objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSPoint, NSRect, NSRunLoop, NSSize},
};
use nsh_testing::agent::{Agent, Model, Native, Scene};

static MAIN_CLICKS: AtomicUsize = AtomicUsize::new(0);
static SECONDARY_CLICKS: AtomicUsize = AtomicUsize::new(0);
static KEYS: AtomicUsize = AtomicUsize::new(0);

#[component]
fn Windows() -> impl IntoElement {
    let secondary = signal(0usize);
    view()
        .size_full()
        .child(
            view()
                .test_id("main-action")
                .label("Main action")
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .w(px(320.0))
                .h(px(80.0))
                .on_press(|| {
                    MAIN_CLICKS.fetch_add(1, Ordering::SeqCst);
                }),
        )
        .child(
            view()
                .test_id("group")
                .absolute()
                .left(px(0.0))
                .top(px(100.0))
                .w(px(320.0))
                .h(px(100.0))
                .child(
                    view()
                        .test_id("child")
                        .focusable(true)
                        .w(px(100.0))
                        .h(px(40.0))
                        .on_key_down(|_: &KeyEvent| {
                            KEYS.fetch_add(1, Ordering::SeqCst);
                        }),
                ),
        )
        .child(
            window()
                .id("agent-secondary")
                .title("Agent secondary")
                .default_size(size(px(320.0), px(240.0)))
                .content(move || {
                    view()
                        .size_full()
                        .child(
                            view()
                                .test_id("secondary-action")
                                .label("Secondary action")
                                .absolute()
                                .left(px(20.0))
                                .top(px(20.0))
                                .w(px(100.0))
                                .h(px(40.0))
                                .on_press(move || {
                                    SECONDARY_CLICKS.fetch_add(1, Ordering::SeqCst);
                                    secondary.update(|count| *count += 1);
                                }),
                        )
                        .child(
                            text(move || format!("Secondary count: {}", secondary.get()))
                                .absolute()
                                .left(px(20.0))
                                .top(px(100.0)),
                        )
                }),
        )
}

fn pump() {
    let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
    // SAFETY: AppKit's immutable default run-loop mode.
    let mode = unsafe { NSDefaultRunLoopMode };
    if let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
        NSEventMask::Any,
        Some(&NSDate::dateWithTimeIntervalSinceNow(0.005)),
        mode,
        true,
    ) {
        app.sendEvent(&event);
    }
    app.updateWindows();
    NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.001));
}

fn until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "{what} never happened");
        pump();
    }
}

fn worker<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        tx.send(f()).ok();
    });
    let mut result = None;
    until("the native agent worker", || match rx.try_recv() {
        Ok(value) => {
            result = Some(value);
            true
        }
        Err(mpsc::TryRecvError::Empty) => false,
        Err(mpsc::TryRecvError::Disconnected) => {
            panic!("native agent worker panicked")
        }
    });
    thread.join().unwrap();
    result.unwrap()
}

fn call<T: Send + 'static>(
    native: &mut Option<Box<dyn Native>>,
    f: impl FnOnce(&dyn Native) -> T + Send + 'static,
) -> T {
    let owned = native.take().unwrap();
    let (owned, result) = worker(move || {
        let result = f(&*owned);
        (owned, result)
    });
    *native = Some(owned);
    result
}

fn node(model: &Model, name: &str) -> NodeId {
    model
        .semantic_tree()
        .unwrap()
        .into_iter()
        .find(|node| node.test_id.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("missing node {name}"))
        .id
}

fn find(view: &NSView, id: &str) -> Option<Retained<NSView>> {
    if view
        .accessibilityIdentifier()
        .is_some_and(|value| value.to_string() == id)
    {
        return Some(view.retain());
    }
    view.subviews().iter().find_map(|child| find(&child, id))
}

fn second_window_hit_test(model: &Model, native: &mut Option<Box<dyn Native>>) {
    let main = node(model, "main-action");
    let secondary = node(model, "secondary-action");
    let root = model
        .window_root("agent-secondary")
        .unwrap()
        .expect("secondary root");
    assert_ne!(root, ROOT_ID);
    call(native, move |native| {
        let at = native.bounds(secondary).unwrap().center();
        assert!(
            native.bounds(main).unwrap().contains(&at),
            "fixture must overlap in window-local coordinates"
        );
        native.target_window(root).unwrap();
        assert_eq!(native.hit_at(at).unwrap(), Some(secondary));
        native.target_window(ROOT_ID).unwrap();
        assert_eq!(native.hit_at(at).unwrap(), Some(main));
        assert!(native.target_window(NodeId::MAX).is_err());
    });
    println!("native_agent: second_window_hit_test");
}

fn secondary_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    NSApplication::sharedApplication(mtm)
        .windows()
        .iter()
        .find(|window| {
            window
                .contentView()
                .is_some_and(|root| find(&root, "secondary-action").is_some())
        })
        .expect("secondary window")
}

fn application_focus(mtm: MainThreadMarker, native: &mut Option<Box<dyn Native>>, main: &NSWindow) {
    let app = NSApplication::sharedApplication(mtm);
    let secondary = secondary_window(mtm);
    main.makeKeyAndOrderFront(None);
    app.activate();
    main.makeKeyWindow();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !(app.isActive() && main.isKeyWindow()) && Instant::now() < deadline {
        pump();
    }
    let focused = app.isActive() && (main.isKeyWindow() || secondary.isKeyWindow());
    assert_eq!(call(native, |native| native.app_focused()), focused);
    if !focused {
        println!(
            "native_agent: application activation or key status is not observable in this session"
        );
    }
    secondary.orderOut(None);
    main.orderOut(None);
    until("the surface's windows losing key status", || {
        !main.isKeyWindow() && !secondary.isKeyWindow()
    });
    assert!(!call(native, |native| native.app_focused()));
    secondary.orderFront(None);
    main.makeKeyAndOrderFront(None);
    main.makeKeyWindow();
    println!("native_agent: application_focus");
}

fn targeted_key_and_group_rejection(
    model: Arc<Model>,
    native: &mut Option<Box<dyn Native>>,
    main: &NSWindow,
    root: &NSView,
) -> Agent {
    let child = node(&model, "child");
    let group = node(&model, "group");
    main.makeKeyAndOrderFront(None);
    main.makeKeyWindow();
    let view = find(root, "child").expect("mounted child");
    call(native, move |native| native.focus_node(child)).unwrap();
    assert!(
        main.firstResponder()
            .is_some_and(|responder| responder.downcast_ref::<NSView>() == Some(&*view))
    );
    // The targeted key must move focus back to the child itself.
    main.makeFirstResponder(None);
    assert!(
        !main
            .firstResponder()
            .is_some_and(|responder| responder.downcast_ref::<NSView>() == Some(&*view))
    );
    let owned = native.take().unwrap();
    let agent = worker(move || {
        let agent = Agent::requiring_native_hit_test(model.clone(), owned);
        agent.key_node(child, "a").unwrap();
        assert_eq!(
            agent.key_node(group, "a").unwrap_err(),
            format!("node {group} cannot receive native focus")
        );
        assert!(agent.key_node(NodeId::MAX, "a").is_err());
        agent
    });
    until("the child's targeted key", || {
        KEYS.load(Ordering::SeqCst) == 1
    });
    assert!(
        main.firstResponder()
            .is_some_and(|responder| responder.downcast_ref::<NSView>() == Some(&*view))
    );
    println!("native_agent: targeted_key_and_group_rejection");
    agent
}

fn named_click_changes_only_secondary(agent: Agent, model: Arc<Model>, secondary: &NSWindow) {
    secondary.makeKeyAndOrderFront(None);
    secondary.makeKeyWindow();
    worker(move || {
        let outcome = agent.take(TestStep::Press {
            selector: "label:Secondary action".into(),
        });
        assert!(matches!(outcome, StepOutcome::Done { .. }), "{outcome:?}");
        // The press lands now; the count's text arrives with a later mount batch.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !model
            .texts()
            .unwrap()
            .iter()
            .any(|text| text == "Secondary count: 1")
        {
            assert!(
                Instant::now() < deadline,
                "the secondary count never updated: clicks {}, texts {:?}",
                SECONDARY_CLICKS.load(Ordering::SeqCst),
                model.texts()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(SECONDARY_CLICKS.load(Ordering::SeqCst), 1);
        assert_eq!(MAIN_CLICKS.load(Ordering::SeqCst), 0);
    });
    println!("native_agent: named_click_changes_only_secondary");
}

pub(super) fn run() {
    let mtm = MainThreadMarker::new().expect("native checks need the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    app.finishLaunching();
    MAIN_CLICKS.store(0, Ordering::SeqCst);
    SECONDARY_CLICKS.store(0, Ordering::SeqCst);
    KEYS.store(0, Ordering::SeqCst);
    // SAFETY: AppKit windows are created and retained on the main thread.
    let main = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(480.0, 360.0)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: This scope owns the window until after closing it.
    unsafe { main.setReleasedWhenClosed(false) };
    let surface: Rc<platform::AppView> = platform::AppView::new(
        mtm,
        platform::AppOptions {
            size: (480, 360),
            ..Default::default()
        },
    );
    let (model, native) = platform::dev::native_test_agent(&surface);
    let mut native = Some(native);
    main.setContentView(Some(&surface.view()));
    surface.load(platform::App::Rust(platform::Root::of::<Windows>()));
    main.makeKeyAndOrderFront(None);
    app.activate();
    until("the secondary control", || {
        model
            .semantic_tree()
            .unwrap()
            .iter()
            .any(|node| node.test_id.as_deref() == Some("secondary-action"))
    });
    until("the secondary window", || {
        model.window_root("agent-secondary").unwrap().is_some()
    });
    // Embedded windows do not use run_app's key-window lifecycle.
    main.makeKeyWindow();
    application_focus(mtm, &mut native, &main);
    second_window_hit_test(&model, &mut native);
    let agent =
        targeted_key_and_group_rejection(model.clone(), &mut native, &main, &surface.view());
    named_click_changes_only_secondary(agent, model, &secondary_window(mtm));
    surface.stop();
    main.close();
}
