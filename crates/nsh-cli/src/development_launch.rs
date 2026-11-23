use std::{
    any::Any,
    io::Write as _,
    process::{Command, Stdio},
    sync::OnceLock,
};

use anyhow::{Context as _, bail};

use crate::{android::Sdk, run::BUNDLE_ID};

/// The launch of a development host, after the host is installed and
/// before it starts. The command line currently reaches this point on
/// Android; the type and its hooks are platform-neutral so another host
/// backend does not need a door of its own for the same lifecycle point.
#[non_exhaustive]
pub struct DevelopmentHostLaunch<'a> {
    backend: Backend<'a>,
}

enum Backend<'a> {
    Android {
        sdk: &'a Sdk,
        serial: &'a str,
        extras: Vec<String>,
    },
}

impl<'a> DevelopmentHostLaunch<'a> {
    pub(crate) fn android(sdk: &'a Sdk, serial: &'a str) -> DevelopmentHostLaunch<'a> {
        DevelopmentHostLaunch {
            backend: Backend::Android {
                sdk,
                serial,
                extras: Vec::new(),
            },
        }
    }

    /// The deployment target's identifier (Android's adb serial).
    pub fn device(&self) -> &str {
        match &self.backend {
            Backend::Android { serial, .. } => serial,
        }
    }

    /// Passes an environment-like string to the host as it starts. On
    /// Android, where another app's process cannot inherit environment
    /// variables, the value is a string extra of its start intent.
    pub fn set_environment(&mut self, name: &str, value: &str) {
        match &mut self.backend {
            Backend::Android { extras, .. } => extras.extend([
                "--es".into(),
                name.to_owned(),
                crate::run::shell_quote(value),
            ]),
        }
    }

    /// Writes `contents` to a file private to the development host before
    /// it starts. `path` is relative to the host application's private
    /// data directory. The bytes go to the device command's standard
    /// input: they are not in its arguments, and a nonzero exit does not
    /// echo them in the error.
    pub fn write_private_file(&self, path: &str, contents: &[u8]) -> anyhow::Result<()> {
        private_path(path)?;
        match &self.backend {
            Backend::Android { sdk, serial, .. } => write_android_file(sdk, serial, path, contents),
        }
    }

    pub(crate) fn android_extras(&self) -> &[String] {
        match &self.backend {
            Backend::Android { extras, .. } => extras,
        }
    }
}

/// A private path stays inside the host's data directory on every
/// platform: relative, normalized and without a parent component.
fn private_path(path: &str) -> anyhow::Result<()> {
    let valid = !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    if !valid {
        bail!("private host file path must be relative and normalized");
    }
    Ok(())
}

/// Writes one private file on Android. `adb shell` joins its arguments
/// with spaces before the device shell reads them, so the `run-as` command
/// is one shell word whose `sh -c` script is quoted itself.
fn write_android_file(sdk: &Sdk, serial: &str, path: &str, contents: &[u8]) -> anyhow::Result<()> {
    let directory = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or(".");
    let script = format!(
        "umask 077 && mkdir -p {} && cat > {}",
        crate::run::shell_quote(directory),
        crate::run::shell_quote(path)
    );
    let write = format!(
        "run-as {BUNDLE_ID} sh -c {}",
        crate::run::shell_quote(&script)
    );
    let mut child = Command::new(sdk.adb())
        .args(["-s", serial, "shell", "-T", &write])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("run adb")?;
    child
        .stdin
        .take()
        .context("adb's standard input")?
        .write_all(contents)
        .context("write to adb")?;
    let output = child.wait_with_output().context("run adb")?;
    if !output.status.success() {
        let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if let Ok(input) = std::str::from_utf8(contents)
            && !input.is_empty()
        {
            stderr = stderr.replace(input, "<redacted>");
        }
        bail!("write the host's private file: {}", stderr.trim());
    }
    Ok(())
}

/// Attaches a tool to the launch of a development host. The hook runs
/// after the host is installed and before it starts; the returned value
/// is kept until the run ends, and an error stops it.
pub type DevelopmentHostLaunchHook =
    fn(&mut DevelopmentHostLaunch<'_>) -> anyhow::Result<Option<Box<dyn Any>>>;

static DEVELOPMENT_HOST_LAUNCH_HOOK: OnceLock<DevelopmentHostLaunchHook> = OnceLock::new();

/// Sets the hook called for each development host launch. A second call
/// returns the hook without installing it.
pub fn set_development_host_launch_hook(
    hook: DevelopmentHostLaunchHook,
) -> Result<(), DevelopmentHostLaunchHook> {
    DEVELOPMENT_HOST_LAUNCH_HOOK.set(hook)
}

pub(crate) fn hook() -> Option<DevelopmentHostLaunchHook> {
    DEVELOPMENT_HOST_LAUNCH_HOOK.get().copied()
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::{DevelopmentHostLaunch, private_path, set_development_host_launch_hook};

    #[test]
    fn development_host_launch_hook_installs_once() {
        fn hook(_: &mut DevelopmentHostLaunch<'_>) -> anyhow::Result<Option<Box<dyn Any>>> {
            Ok(None)
        }

        assert!(set_development_host_launch_hook(hook).is_ok());
        assert!(set_development_host_launch_hook(hook).is_err());
    }

    #[test]
    fn private_file_paths_stay_in_the_host_data() {
        assert!(private_path("files/configure").is_ok());
        assert!(private_path("token").is_ok());
        for path in [
            "",
            "/files/token",
            "files//token",
            "./token",
            "../token",
            "files/../token",
            "files/",
        ] {
            assert!(private_path(path).is_err(), "{path}");
        }
    }
}
