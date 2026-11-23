//! What a development host exposes of the surface it opens.

use std::sync::{Arc, Weak};

use jni::objects::GlobalRef;
use nsh_testing::agent::Native;

use crate::host::Host;

/// An Android surface a development host opened: its native driver,
/// density, view and Java VM. The driver and the view are read-only handles
/// on the surface; owning neither changes its lifecycle.
#[non_exhaustive]
pub struct AndroidSurface {
    host: Weak<Host>,
    density: f32,
    view: GlobalRef,
    vm: *mut jni::sys::JavaVM,
}

impl AndroidSurface {
    pub(crate) fn new(host: &Arc<Host>, vm: *mut jni::sys::JavaVM) -> AndroidSurface {
        AndroidSurface {
            host: Arc::downgrade(host),
            density: host.density(),
            view: host.view.clone(),
            vm,
        }
    }

    /// A native driver of this surface, as the platform's test agent uses.
    pub fn native(&self) -> Box<dyn Native> {
        Box::new(crate::agent::AndroidNative::new(
            self.host.clone(),
            self.density,
        ))
    }

    /// The display's density: pixels per point.
    pub fn density(&self) -> f32 {
        self.density
    }

    /// The surface's `NshView`, held by a global JNI reference.
    pub fn view(&self) -> &GlobalRef {
        &self.view
    }

    /// The Java VM the surface runs in.
    pub fn java_vm(&self) -> *mut jni::sys::JavaVM {
        self.vm
    }
}
