//! Main-thread AppKit event dispatch while agent work runs off-thread.

use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use nsh_platform_macos::{
    objc2::MainThreadMarker,
    objc2_app_kit::{NSApplication, NSEventMask},
    objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSRunLoop},
};

pub(super) fn pump() {
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

pub(super) fn until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "{what} never happened");
        pump();
    }
}

pub(super) fn worker<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
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
