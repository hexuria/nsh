use std::{
    any::Any,
    io::Write as _,
    path::PathBuf,
    process::{Command, Stdio},
    sync::OnceLock,
};

use anyhow::{Context as _, bail};

use crate::android::Sdk;

/// The launch of the Android development host on one device.
#[non_exhaustive]
pub struct AndroidLaunch<'a> {
    sdk: &'a Sdk,
    serial: &'a str,
    extras: Vec<String>,
}

impl<'a> AndroidLaunch<'a> {
    pub(crate) fn new(sdk: &'a Sdk, serial: &'a str) -> AndroidLaunch<'a> {
        AndroidLaunch {
            sdk,
            serial,
            extras: Vec::new(),
        }
    }

    /// The adb serial of the device the host launches on.
    pub fn serial(&self) -> &str {
        self.serial
    }

    /// The adb binary this launch uses.
    pub fn adb_path(&self) -> PathBuf {
        self.sdk.adb()
    }

    /// Runs adb on this launch's device.
    pub fn adb(&self, args: &[&str]) -> anyhow::Result<String> {
        crate::android::adb(self.sdk, self.serial, args)
    }

    /// Runs adb on this launch's device, writing `input` to its standard
    /// input. The input never appears in adb's arguments; errors redact it.
    pub fn adb_with_input(&self, args: &[&str], input: &str) -> anyhow::Result<String> {
        let mut child = Command::new(self.sdk.adb())
            .args(["-s", self.serial])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("run adb")?;
        child
            .stdin
            .take()
            .context("adb's standard input")?
            .write_all(input.as_bytes())
            .context("write to adb")?;
        let output = child.wait_with_output().context("run adb")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = if input.is_empty() {
                stderr.into_owned()
            } else {
                stderr.replace(input, "<redacted>")
            };
            bail!("adb {} failed: {}", args.join(" "), stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Adds a string extra to the host's launch intent.
    pub fn intent_extra(&mut self, name: &str, value: &str) {
        self.extras.extend([
            "--es".into(),
            name.to_owned(),
            crate::run::shell_quote(value),
        ]);
    }

    pub(crate) fn extras(&self) -> &[String] {
        &self.extras
    }
}

/// Attaches a tool to the launch of the Android development host. The hook
/// runs after the device is prepared and before its start intent is sent;
/// the returned value is kept until the run ends, and an error stops it.
pub type AndroidLaunchHook = fn(&mut AndroidLaunch<'_>) -> anyhow::Result<Option<Box<dyn Any>>>;

static ANDROID_LAUNCH_HOOK: OnceLock<AndroidLaunchHook> = OnceLock::new();

/// Sets the hook called for each Android development host launch. A second
/// call returns the hook without installing it.
pub fn set_android_launch_hook(hook: AndroidLaunchHook) -> Result<(), AndroidLaunchHook> {
    ANDROID_LAUNCH_HOOK.set(hook)
}

pub(crate) fn hook() -> Option<AndroidLaunchHook> {
    ANDROID_LAUNCH_HOOK.get().copied()
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::{AndroidLaunch, set_android_launch_hook};

    #[test]
    fn android_launch_hook_installs_once() {
        fn hook(_: &mut AndroidLaunch<'_>) -> anyhow::Result<Option<Box<dyn Any>>> {
            Ok(None)
        }

        assert!(set_android_launch_hook(hook).is_ok());
        assert!(set_android_launch_hook(hook).is_err());
    }
}
