//! `nsh/device`'s requests to the system: opening a URL outside the app
//! ([`crate::open_url`]). Needs the `openUrl` capability when the app
//! declares capabilities (else it rejects with `ERR_ACCESS_DENIED`).

use crate::element::{JsError, JsObject, Later, PropEnum, System, module};

/// What `openUrl` did.
#[derive(Debug, Clone, Copy, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub(crate) struct OpenUrlResult {
    /// Whether an app opened the URL (`false` when none handles it).
    pub(crate) opened: bool,
}

/// How `deviceAuth` ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub(crate) enum DeviceAuthResult {
    /// The device accepted the user.
    Yes,
    /// The user was not accepted.
    No,
    /// The device cannot authenticate here.
    #[default]
    NotAvailable,
    /// The user dismissed the prompt.
    Cancelled,
    /// Too many attempts; the user waits or unlocks the device first.
    LockedOut,
}

impl From<crate::element::DeviceAuthResult> for DeviceAuthResult {
    fn from(result: crate::element::DeviceAuthResult) -> Self {
        match result {
            crate::element::DeviceAuthResult::Yes => Self::Yes,
            crate::element::DeviceAuthResult::No => Self::No,
            crate::element::DeviceAuthResult::NotAvailable => Self::NotAvailable,
            crate::element::DeviceAuthResult::Cancelled => Self::Cancelled,
            crate::element::DeviceAuthResult::LockedOut => Self::LockedOut,
        }
    }
}

/// What the app asks of the system it runs on.
#[module]
pub(crate) struct Device;

#[module(flat)]
impl Device {
    /// Opens `url` outside the app: a web page in the browser, a `mailto:`
    /// link in the mail app, a `tel:` link in the phone app, any scheme
    /// another app registered. The embedding app may take it first and
    /// route it its own way. Resolves with `{ opened }`: whether it opened
    /// (`false` when no app handles it). Rejects with a `TypeError` for a
    /// URL it never opens (not absolute, `javascript:`, `data:`, `file:`,
    /// ...), and with `ERR_ACCESS_DENIED` when the `openUrl` capability
    /// does not grant its scheme.
    pub(crate) fn open_url(url: String, sys: System) -> Result<Later<OpenUrlResult>, JsError> {
        crate::open_url::check_url(&url).map_err(JsError::Type)?;
        if !sys.access.allows_open_url(&url) {
            return Err(crate::open_url::not_granted(&url));
        }
        Ok(Later::new(async move {
            Ok::<_, JsError>(OpenUrlResult {
                opened: crate::prelude::open_url(url).await,
            })
        }))
    }

    /// Asks the user to unlock for `reason`. Biometrics are the only
    /// accepted device authentication unless `allowDeviceCredential` is
    /// true; the app handles its own PIN fallback. Resolves with `"yes"`
    /// when the device accepts the user, `"no"` when the user chooses the
    /// app's PIN fallback, `"notAvailable"` when the device cannot
    /// authenticate here, `"cancelled"` when the user dismisses the prompt
    /// or it times out, or `"lockedOut"` after too many attempts. A
    /// biometric sample that does not match keeps the system prompt waiting.
    pub(crate) fn device_auth(
        reason: String,
        allow_device_credential: Option<bool>,
    ) -> Later<DeviceAuthResult> {
        Later::new(async move {
            Ok::<_, JsError>(
                crate::prelude::device_auth(&reason, allow_device_credential.unwrap_or(false))
                    .await
                    .into(),
            )
        })
    }
}
