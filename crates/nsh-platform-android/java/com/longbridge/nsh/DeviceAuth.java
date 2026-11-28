package com.longbridge.nsh;

import android.app.Activity;
import android.content.Context;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;

/**
 * Device-authentication prompts ({@code nsh_core::element::device_auth}). The first implementation is Android's
 * framework biometric prompt: biometrics only by default, with the device credential added only when the app allows
 * it. The user cancel action answers {@code cancelled}; the API 28-29 negative button ("Use PIN", where the device
 * credential is not allowed) answers {@code no}. A failed biometric sample keeps the system prompt waiting rather
 * than answering {@code no}; five failed samples answer {@code lockedOut} through the prompt's lockout error.
 */
final class DeviceAuth {
    private DeviceAuth() {}

    /** What the device can unlock with: 0 none, 1 biometric, 2 device credential only. */
    static int availability(Context context) {
        if (Build.VERSION.SDK_INT < 28) return 0;
        if (Build.VERSION.SDK_INT >= 30) {
            android.hardware.biometrics.BiometricManager manager =
                    context.getSystemService(android.hardware.biometrics.BiometricManager.class);
            if (manager == null) return 0;
            int strong = manager.canAuthenticate(
                    android.hardware.biometrics.BiometricManager.Authenticators.BIOMETRIC_STRONG);
            if (strong == android.hardware.biometrics.BiometricManager.BIOMETRIC_SUCCESS) return 1;
            int credential = manager.canAuthenticate(
                    android.hardware.biometrics.BiometricManager.Authenticators.DEVICE_CREDENTIAL);
            return credential == android.hardware.biometrics.BiometricManager.BIOMETRIC_SUCCESS ? 2 : 0;
        }
        if (Build.VERSION.SDK_INT == 29) {
            android.hardware.biometrics.BiometricManager manager =
                    context.getSystemService(android.hardware.biometrics.BiometricManager.class);
            if (manager != null && manager.canAuthenticate()
                    == android.hardware.biometrics.BiometricManager.BIOMETRIC_SUCCESS) return 1;
        }
        android.hardware.fingerprint.FingerprintManager fingerprints =
                context.getSystemService(android.hardware.fingerprint.FingerprintManager.class);
        try {
            if (fingerprints != null && fingerprints.isHardwareDetected() && fingerprints.hasEnrolledFingerprints()) {
                return 1;
            }
        } catch (SecurityException e) {
            return 0;
        }
        android.app.KeyguardManager keyguard =
                (android.app.KeyguardManager) context.getSystemService(Context.KEYGUARD_SERVICE);
        return keyguard != null && keyguard.isDeviceSecure() ? 2 : 0;
    }

    /** Shows prompt {@code id} on the activity's main thread and answers it through {@code mounter}. */
    static void prompt(Context context, Mounter mounter, int id, String reason, boolean allowDeviceCredential) {
        Handler main = new Handler(Looper.getMainLooper());
        main.post(() -> show(context, mounter, id, reason, allowDeviceCredential));
    }

    private static void show(Context context, Mounter mounter, int id, String reason, boolean allowDeviceCredential) {
        boolean[] answered = new boolean[] {false};
        Activity activity = SystemUi.activity(context);
        if (activity == null || activity.isFinishing() || activity.isDestroyed()
                || Build.VERSION.SDK_INT < 28) {
            answerOnce(answered, mounter, id, "notAvailable");
            return;
        }
        java.util.concurrent.Executor executor = command -> new Handler(Looper.getMainLooper()).post(command);
        android.hardware.biometrics.BiometricPrompt.Builder builder =
                new android.hardware.biometrics.BiometricPrompt.Builder(activity)
                        .setTitle("Unlock")
                        .setDescription(reason);
        if (Build.VERSION.SDK_INT >= 30) {
            int authenticators = android.hardware.biometrics.BiometricManager.Authenticators.BIOMETRIC_STRONG;
            if (allowDeviceCredential) {
                authenticators |= android.hardware.biometrics.BiometricManager.Authenticators.DEVICE_CREDENTIAL;
            }
            builder.setAllowedAuthenticators(authenticators);
        } else if (allowDeviceCredential) {
            builder.setDeviceCredentialAllowed(true);
        }
        if (!allowDeviceCredential) {
            builder.setNegativeButton("Use PIN", executor, (dialog, which) -> answerOnce(answered, mounter, id, "no"));
        }
        android.hardware.biometrics.BiometricPrompt prompt = builder.build();
        android.os.CancellationSignal cancellation = new android.os.CancellationSignal();
        // The framework prompt itself never times out. NSH cancels a prompt left
        // unanswered for half a minute so an app is never promised an answer forever;
        // the emulator proof uses this path for `cancelled`.
        new Handler(Looper.getMainLooper()).postDelayed(cancellation::cancel, 30_000);
        prompt.authenticate(cancellation, executor,
                new android.hardware.biometrics.BiometricPrompt.AuthenticationCallback() {
            @Override public void onAuthenticationError(int errorCode, CharSequence error) {
                switch (errorCode) {
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT:
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT_PERMANENT:
                        answerOnce(answered, mounter, id, "lockedOut");
                        break;
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_NO_BIOMETRICS:
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_HW_NOT_PRESENT:
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_NO_DEVICE_CREDENTIAL:
                        answerOnce(answered, mounter, id, "notAvailable");
                        break;
                    default:
                        answerOnce(answered, mounter, id, "cancelled");
                }
            }

            @Override public void onAuthenticationSucceeded(
                    android.hardware.biometrics.BiometricPrompt.AuthenticationResult result) {
                answerOnce(answered, mounter, id, "yes");
            }

            @Override public void onAuthenticationFailed() {
                // The system prompt keeps waiting after a non-match; the core gets one final answer only.
            }
                });
    }

    private static void answerOnce(boolean[] answered, Mounter mounter, int id, String result) {
        if (!answered[0]) {
            answered[0] = true;
            answer(mounter, id, result);
        }
    }

    private static void answer(Mounter mounter, int id, String result) {
        mounter.eventMap(1, Native.DEVICE_AUTH, new String[] {"id", "result"}, new Object[] {(double) id, result});
    }
}
