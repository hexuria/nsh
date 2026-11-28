//! Unlocking with the device's own authentication: biometrics by default,
//! the device's credential only where the app asks for it. Android carries
//! the first implementation; the other platforms report none for now.

use super::{Answer, task::Action};
use crate::{EventPayload, PayloadValue};

/// What the device can unlock with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceAuthKind {
    /// No device authentication is available.
    #[default]
    None,
    /// A biometric is enrolled and can be asked.
    Biometric,
    /// No biometric can be asked, but the device credential can.
    DeviceCredentialOnly,
}

/// How a device-authentication prompt ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceAuthResult {
    /// The device accepted the user.
    Yes,
    /// The user was not accepted. On Android this is the negative button
    /// (`Use PIN`) where the device credential is not allowed; a rejected
    /// biometric sample itself keeps the system prompt waiting.
    No,
    /// The device cannot authenticate here.
    #[default]
    NotAvailable,
    /// The user dismissed the prompt.
    Cancelled,
    /// Too many attempts; the user waits or unlocks the device first.
    LockedOut,
}

impl DeviceAuthKind {
    /// The wire name scripts read.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Biometric => "biometric",
            Self::DeviceCredentialOnly => "deviceCredentialOnly",
        }
    }
}

impl DeviceAuthResult {
    /// The wire name platforms report.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::NotAvailable => "notAvailable",
            Self::Cancelled => "cancelled",
            Self::LockedOut => "lockedOut",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "yes" => Self::Yes,
            "no" => Self::No,
            "notAvailable" => Self::NotAvailable,
            "cancelled" => Self::Cancelled,
            "lockedOut" => Self::LockedOut,
            _ => return None,
        })
    }

    /// The root's [`EventKind::DeviceAuth`](crate::EventKind::DeviceAuth)
    /// payload answering request `id`.
    pub fn payload(self, id: u32) -> EventPayload {
        EventPayload::Map(vec![
            ("id", PayloadValue::Number(id as f64)),
            ("result", PayloadValue::Str(self.as_str().into())),
        ])
    }
}

/// What the device can unlock with now (sync and cheap): biometrics when
/// one is enrolled, the device credential where it alone is available,
/// none elsewhere. Android refreshes this with the environment (including
/// on resume); Linux, the web, terminals, macOS, iOS and Windows report
/// [`DeviceAuthKind::None`] for now.
pub fn device_auth_available() -> DeviceAuthKind {
    super::ambient::device_auth_available()
}

/// Asks the user to unlock for `reason`. Biometrics are the only accepted
/// device authentication unless `allow_device_credential` is set: the
/// phone's screen lock is never offered by default, and the app's own PIN
/// remains the app's separate fallback. Await the answer in a task;
/// unsupported platforms answer
/// [`DeviceAuthResult::NotAvailable`] at once.
pub fn device_auth(reason: &str, allow_device_credential: bool) -> Answer<DeviceAuthResult> {
    let (answer, slot) = Answer::new();
    if device_auth_available() == DeviceAuthKind::None {
        slot.borrow_mut().deliver(DeviceAuthResult::NotAvailable);
        return answer;
    }
    super::ambient::act(Action::DeviceAuth {
        reason: reason.to_owned(),
        allow_device_credential,
        answer: slot,
    });
    answer
}

/// Parses a root [`EventKind::DeviceAuth`](crate::EventKind::DeviceAuth)
/// payload into its request and result.
pub(crate) fn answer_from_payload(payload: &EventPayload) -> Option<(u32, DeviceAuthResult)> {
    let EventPayload::Map(fields) = payload else {
        return None;
    };
    let id = fields.iter().find_map(|(k, v)| match v {
        PayloadValue::Number(n) if *k == "id" => Some(*n as u32),
        _ => None,
    })?;
    let result = fields.iter().find_map(|(k, v)| match v {
        PayloadValue::Str(s) if *k == "result" => DeviceAuthResult::parse(s),
        _ => None,
    })?;
    Some((id, result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_result_round_trips_its_payload() {
        for result in [
            DeviceAuthResult::Yes,
            DeviceAuthResult::No,
            DeviceAuthResult::NotAvailable,
            DeviceAuthResult::Cancelled,
            DeviceAuthResult::LockedOut,
        ] {
            assert_eq!(answer_from_payload(&result.payload(42)), Some((42, result)));
        }
    }
}
