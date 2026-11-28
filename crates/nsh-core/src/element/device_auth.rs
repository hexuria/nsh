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
/// [`DeviceAuthKind::None`] for now. Read it on the engine (UI) thread.
pub fn device_auth_available() -> DeviceAuthKind {
    super::ambient::device_auth_available()
}

/// Asks the user to unlock for `reason`. Biometrics are the only accepted
/// device authentication unless `allow_device_credential` is set: the
/// phone's screen lock is never offered by default, and the app's own PIN
/// remains the app's separate fallback. On Android API 28-29 the system
/// default biometric strength applies; `BIOMETRIC_STRONG` can be requested
/// only on API 30 and later. Call this on the engine (UI) thread and await
/// the answer in a task; a call from another thread and unsupported
/// platforms answer [`DeviceAuthResult::NotAvailable`] at once.
pub fn device_auth(reason: &str, allow_device_credential: bool) -> Answer<DeviceAuthResult> {
    let (answer, slot) = Answer::new();
    let Some(available) = super::ambient::try_device_auth_available() else {
        log::warn!("device_auth called outside the engine thread; answering notAvailable");
        slot.borrow_mut().deliver(DeviceAuthResult::NotAvailable);
        return answer;
    };
    if available == DeviceAuthKind::None {
        slot.borrow_mut().deliver(DeviceAuthResult::NotAvailable);
        return answer;
    }
    if !super::ambient::act_if_running(Action::DeviceAuth {
        reason: reason.to_owned(),
        allow_device_credential,
        answer: slot.clone(),
    }) {
        log::warn!("device_auth called outside the engine thread; answering notAvailable");
        slot.borrow_mut().deliver(DeviceAuthResult::NotAvailable);
    }
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

    #[test]
    fn a_prompt_outside_the_engine_answers_not_available_instead_of_panicking() {
        let result = std::thread::spawn(|| {
            use std::{pin::Pin, task::Context};
            let mut answer = device_auth("Unlock the test", false);
            Pin::new(&mut answer).poll(&mut Context::from_waker(std::task::Waker::noop()))
        })
        .join()
        .unwrap();
        assert!(matches!(
            result,
            std::task::Poll::Ready(DeviceAuthResult::NotAvailable)
        ));
    }
}
