package com.longbridge.nsh;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

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
        try {
            return availabilityUnchecked(context);
        } catch (SecurityException e) {
            return 0;
        } catch (RuntimeException e) {
            return 0;
        }
    }

    private static int availabilityUnchecked(Context context) {
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
        try {
            showUnchecked(context, mounter, id, reason, allowDeviceCredential, answered);
        } catch (SecurityException e) {
            answerOnce(answered, null, null, mounter, id, "notAvailable");
        } catch (RuntimeException e) {
            answerOnce(answered, null, null, mounter, id, "notAvailable");
        } catch (Error e) {
            answerOnce(answered, null, null, mounter, id, "notAvailable");
        }
    }

    private static void showUnchecked(Context context, Mounter mounter, int id, String reason,
            boolean allowDeviceCredential, boolean[] answered) {
        Activity activity = SystemUi.activity(context);
        if (activity == null || activity.isFinishing() || activity.isDestroyed()
                || Build.VERSION.SDK_INT < 28) {
            answerOnce(answered, null, null, mounter, id, "notAvailable");
            return;
        }
        if (allowDeviceCredential && Build.VERSION.SDK_INT == 29 && availabilityUnchecked(context) == 2) {
            confirmCredential(activity, reason, mounter, id, answered, null, null);
            return;
        }
        java.util.concurrent.Executor executor = command -> new Handler(Looper.getMainLooper()).post(command);
        Handler handler = new Handler(Looper.getMainLooper());
        Runnable[] timeout = new Runnable[1];
        android.os.CancellationSignal cancellation = new android.os.CancellationSignal();
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
        }
        // API 29's setDeviceCredentialAllowed handoff reports the biometric sheet's
        // dismissal as a terminal error before the credential result on emulator
        // builds. Use the same visible flow explicitly: biometric first, then the
        // negative action opens the platform credential confirmation whose result
        // this fragment-owned request answers.
        boolean manualCredential = allowDeviceCredential && Build.VERSION.SDK_INT == 29;
        boolean credentialOffered = allowDeviceCredential && Build.VERSION.SDK_INT >= 29;
        if (!credentialOffered || manualCredential) {
            builder.setNegativeButton("Use PIN", executor,
                    (dialog, which) -> {
                        if (manualCredential) {
                            cancellation.cancel();
                            confirmCredential(activity, reason, mounter, id, answered, handler, timeout[0]);
                        } else {
                            answerOnce(answered, handler, timeout[0], mounter, id, "no");
                        }
                    });
        }
        android.hardware.biometrics.BiometricPrompt prompt = builder.build();
        if (!credentialOffered) {
            // Apple and Windows do not time a credential prompt out. A biometric-only
            // prompt still settles: its timeout cancels the prompt and answers itself.
            timeout[0] = () -> {
                cancellation.cancel();
                answerOnce(answered, handler, timeout[0], mounter, id, "cancelled");
            };
        }
        prompt.authenticate(cancellation, executor,
                new android.hardware.biometrics.BiometricPrompt.AuthenticationCallback() {
            @Override public void onAuthenticationError(int errorCode, CharSequence error) {
                switch (errorCode) {
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT:
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT_PERMANENT:
                        answerOnce(answered, handler, timeout[0], mounter, id, "lockedOut");
                        break;
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_NO_BIOMETRICS:
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_HW_NOT_PRESENT:
                    case android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_NO_DEVICE_CREDENTIAL:
                        answerOnce(answered, handler, timeout[0], mounter, id, "notAvailable");
                        break;
                    default:
                        // API 29 reports ERROR_CANCELED (and on some builds the hidden
                        // negative-button error 13) when its sheet closes for the explicit
                        // credential confirmation. Those are not final answers: the fragment's
                        // activity result settles the request. Real user cancellation keeps
                        // ERROR_USER_CANCELED and answers cancelled.
                        boolean credentialTransition = errorCode
                                == android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_CANCELED
                                || errorCode == 13;
                        if (!(credentialOffered && credentialTransition)) {
                            answerOnce(answered, handler, timeout[0], mounter, id, "cancelled");
                        }
                }
            }

            @Override public void onAuthenticationSucceeded(
                    android.hardware.biometrics.BiometricPrompt.AuthenticationResult result) {
                answerOnce(answered, handler, timeout[0], mounter, id, "yes");
            }

            @Override public void onAuthenticationFailed() {
                // The system prompt keeps waiting after a non-match; the core gets one final answer only.
            }
                });
        if (!credentialOffered) handler.postDelayed(timeout[0], 30_000);
    }

    private static final String TAG = "nsh-device-auth";
    private static final String RESULT_FRAGMENT = "nsh-device-auth-results";

    /** One API 29 device-credential confirmation held by {@link CredentialResults}. */
    private static final class CredentialRequest {
        final Mounter mounter;
        final int id;
        final boolean[] answered;
        final Handler timeoutHandler;
        final Runnable timeout;

        CredentialRequest(Mounter mounter, int id, boolean[] answered,
                Handler timeoutHandler, Runnable timeout) {
            this.mounter = mounter;
            this.id = id;
            this.answered = answered;
            this.timeoutHandler = timeoutHandler;
            this.timeout = timeout;
        }
    }

    /** Starts credential confirmation and hears its result without the activity forwarding it. */
    @SuppressWarnings("deprecation")
    public static final class CredentialResults extends android.app.Fragment {
        private final android.util.SparseArray<CredentialRequest> pending = new android.util.SparseArray<>();
        private int next = 0x4d00;

        @Override
        public void onCreate(Bundle state) {
            super.onCreate(state);
            setRetainInstance(true);
        }

        void start(Intent intent, CredentialRequest request) {
            int code = next++;
            if (next > 0x4dff) next = 0x4d00;
            pending.put(code, request);
            try {
                startActivityForResult(intent, code);
            } catch (RuntimeException e) {
                Log.w(TAG, "cannot show the device credential prompt", e);
                pending.remove(code);
                answerOnce(request.answered, request.timeoutHandler, request.timeout,
                        request.mounter, request.id, "notAvailable");
            }
        }

        @Override
        public void onActivityResult(int code, int result, Intent data) {
            CredentialRequest request = pending.get(code);
            if (request == null) {
                super.onActivityResult(code, result, data);
                return;
            }
            pending.remove(code);
            answerOnce(request.answered, request.timeoutHandler, request.timeout,
                    request.mounter, request.id, result == Activity.RESULT_OK ? "yes" : "cancelled");
        }
    }

    private static void confirmCredential(Activity activity, String reason, Mounter mounter, int id,
            boolean[] answered, Handler timeoutHandler, Runnable timeout) {
        try {
            android.app.KeyguardManager keyguard =
                    (android.app.KeyguardManager) activity.getSystemService(Context.KEYGUARD_SERVICE);
            Intent intent = keyguard == null ? null
                    : keyguard.createConfirmDeviceCredentialIntent("Unlock", reason);
            if (intent == null) {
                answerOnce(answered, timeoutHandler, timeout, mounter, id, "notAvailable");
                return;
            }
            android.app.FragmentManager manager = activity.getFragmentManager();
            CredentialResults results = (CredentialResults) manager.findFragmentByTag(RESULT_FRAGMENT);
            if (results == null) {
                results = new CredentialResults();
                manager.beginTransaction().add(results, RESULT_FRAGMENT).commitNow();
            }
            results.start(intent, new CredentialRequest(mounter, id, answered, timeoutHandler, timeout));
        } catch (RuntimeException e) {
            Log.w(TAG, "cannot show the device credential prompt", e);
            answerOnce(answered, timeoutHandler, timeout, mounter, id, "notAvailable");
        } catch (Error e) {
            answerOnce(answered, timeoutHandler, timeout, mounter, id, "notAvailable");
        }
    }

    private static void answerOnce(boolean[] answered, Handler timeoutHandler, Runnable timeout,
            Mounter mounter, int id, String result) {
        if (!answered[0]) {
            answered[0] = true;
            if (timeoutHandler != null && timeout != null) timeoutHandler.removeCallbacks(timeout);
            answer(mounter, id, result);
        }
    }

    private static void answer(Mounter mounter, int id, String result) {
        mounter.eventMap(1, Native.DEVICE_AUTH, new String[] {"id", "result"}, new Object[] {(double) id, result});
    }
}
