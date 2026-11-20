//! Real shared GPUI-agent listener checks in a desktop AppKit process.
//! AppView construction starts the production Surface listener; protocol calls
//! run off-thread while the main thread dispatches native input and mount batches.
//!
//! NSH_AGENT_LISTENER_CHECK_ONLY=1 cargo test -p nsh-platform-macos --features dev-agent --test native -- --ignored
//! Requires a debug build with dev-agent and without gpui-agent-server.

use std::{
    io::ErrorKind,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use gpui_agent::{AgentClient, protocol::Op};
use nsh_base::button;
use nsh_core::prelude::*;
use nsh_platform_macos::{
    self as platform,
    objc2::{MainThreadMarker, MainThreadOnly},
    objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow,
        NSWindowStyleMask,
    },
    objc2_foundation::{NSPoint, NSRect, NSSize},
};

use super::agent_support::worker;

static GENERATION: AtomicUsize = AtomicUsize::new(0);

#[component]
fn ListenerFixture() -> impl IntoElement {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst);
    let pressed = signal(false);
    view().size_full().child(
        button()
            .accessibility_label(move || {
                Some(format!("Generation {generation}: pressed {}", pressed.get()).into())
            })
            .absolute()
            .left(px(20.0))
            .top(px(20.0))
            .w(px(300.0))
            .h(px(80.0))
            .on_press(move || pressed.set(true))
            .child(text(move || {
                format!("Generation {generation}: pressed {}", pressed.get())
            })),
    )
}

fn new_client(addr: SocketAddr, token: &str) -> AgentClient {
    AgentClient::connect(addr)
        .with_token(token)
        .with_timeout(Duration::from_secs(2))
}

fn wait_for_button(client: &mut AgentClient, name: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let tree = client
            .snapshot()
            .expect("snapshot")
            .tree
            .expect("snapshot tree");
        if let Some(node) = tree
            .flatten()
            .into_iter()
            .find(|node| node.role == "button" && node.name == name)
        {
            return node.id.clone();
        }
        assert!(
            Instant::now() < deadline,
            "button {name:?} missing from {tree:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn run() {
    if !cfg!(debug_assertions) {
        panic!("the agent listener checks require a debug build");
    }
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = reserved.local_addr().unwrap();
    drop(reserved);
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).expect("random agent token");
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    // SAFETY: this process-only case configures the environment before starting threads.
    unsafe {
        std::env::set_var("GPUI_AGENT", "1");
        std::env::set_var("GPUI_AGENT_ADDR", addr.to_string());
        std::env::set_var("GPUI_AGENT_TOKEN", &token);
    }

    let mtm = MainThreadMarker::new().expect("native checks need the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    app.finishLaunching();
    // SAFETY: the AppKit window is created and retained on the main thread.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(480.0, 360.0)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: this scope owns the window until after closing it.
    unsafe { window.setReleasedWhenClosed(false) };
    let view = platform::AppView::new(
        mtm,
        platform::AppOptions {
            size: (480, 360),
            ..Default::default()
        },
    );
    window.setContentView(Some(&view.view()));
    view.load(platform::App::Rust(platform::Root::of::<ListenerFixture>()));
    window.makeKeyAndOrderFront(None);
    window.makeKeyWindow();
    app.activate();

    let startup_token = token.clone();
    worker(move || {
        let mut client = new_client(addr, &startup_token);
        let hello = client.expect_ok(Op::Hello).expect("authenticated hello");
        assert_eq!(hello.v, 2);
        let hello = hello.hello.expect("hello info");
        assert_eq!(hello.protocol, 2);
        assert!(hello.features.iter().any(|feature| feature == "press"));
        let id = wait_for_button(&mut client, "Generation 0: pressed false");
        client
            .expect_ok(Op::Press {
                target: Some(id),
                x: None,
                y: None,
                expect_fingerprint: None,
            })
            .expect("native button press");
        wait_for_button(&mut client, "Generation 0: pressed true");
        let mut wrong = new_client(addr, "wrong-token");
        assert_eq!(
            wrong.expect_ok(Op::Hello).unwrap_err(),
            "invalid automation token"
        );
    });
    println!("agent_listener: startup, snapshot, press and authentication");

    // Cmd+R's reload_latest and reload both call AppView::load; no dev server is needed.
    // This replaces the engine and mounted content, retaining the Surface and its worker.
    view.load(platform::App::Rust(platform::Root::of::<ListenerFixture>()));
    worker(move || {
        let mut client = new_client(addr, &token);
        client
            .expect_ok(Op::Hello)
            .expect("hello after reload on the same address");
        wait_for_button(&mut client, "Generation 1: pressed false");
    });
    println!("agent_listener: reload on the same address with a new connection");

    let deadline = Instant::now() + Duration::from_secs(2);
    let (done, shutdown) = mpsc::sync_channel(1);
    // The watchdog also bounds a Drop that blocks the main thread before it can pump again.
    let watchdog = std::thread::spawn(move || {
        if shutdown
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .is_err()
        {
            eprintln!("agent_listener: dropping the AppView and refusing connections exceeded 2 s");
            std::process::abort();
        }
    });
    drop(view);
    window.close();
    worker(move || {
        loop {
            match TcpStream::connect_timeout(&addr, Duration::from_millis(25)) {
                Err(error) if error.kind() == ErrorKind::ConnectionRefused => break,
                Err(error) => panic!("unexpected shutdown connection error: {error}"),
                Ok(stream) => drop(stream),
            }
            assert!(
                Instant::now() < deadline,
                "the listener did not stop within 2 s"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    assert!(Instant::now() < deadline, "shutdown exceeded 2 s");
    done.send(()).unwrap();
    watchdog.join().unwrap();
    println!("agent_listener: nonblocking shutdown and refused connection within 2 s");
}
