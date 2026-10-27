package com.longbridge.nsh;

import android.app.Activity;
import android.app.Dialog;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.widget.EditText;
import android.widget.TextView;

import java.io.ByteArrayOutputStream;
import java.util.concurrent.Callable;
import java.util.concurrent.FutureTask;
import java.util.concurrent.TimeUnit;

/**
 * The platform's part of `nsh test --target android` (the agent itself is
 * Rust, `agent.rs`): where a node's view is, and what a user's finger and
 * keyboard do there, as `MotionEvent`s and `KeyEvent`s dispatched to the
 * window's views, so they take the path real ones take (scroll views, text
 * fields, gesture detectors). The agent calls from a thread of its own;
 * each call does its work on the main thread and returns when it is done.
 * Coordinates are pixels from the window's top left.
 */
final class TestAgent {
    private TestAgent() {}

    private static final Handler MAIN = new Handler(Looper.getMainLooper());

    /** The dialog the app shows now, for answering it. */
    static Dialog shownDialog;

    /** `call` on the main thread, waited for (10 s at most); null on failure. */
    private static <T> T onMain(Callable<T> call) {
        return onMain(call, null);
    }

    private static <T> T onMain(Callable<T> call, T fallback) {
        FutureTask<T> task = new FutureTask<>(call);
        MAIN.post(task);
        try {
            return task.get(10, TimeUnit.SECONDS);
        } catch (Exception e) {
            android.util.Log.w("nsh", "test agent: " + e);
            return fallback;
        }
    }

    /** The view of node `id`'s bounds in the window: x, y, width, height; null when it does not show. */
    static float[] bounds(NshView host, int id) {
        return onMain(() -> {
            View v = host.mounter().views.get(id);
            if (v == null || !v.isShown()) return null;
            int[] at = new int[2];
            v.getLocationInWindow(at);
            return new float[] {at[0], at[1], v.getWidth(), v.getHeight()};
        });
    }

    /** {@link #hitAt}: nothing takes a touch there. */
    static final int HIT_NONE = -1;
    /** {@link #hitAt}: a view outside the app's nodes takes it. */
    static final int HIT_FOREIGN = -2;
    /** {@link #hitAt}: the main thread did not answer, or the walk threw. */
    static final int HIT_FAILED = Integer.MIN_VALUE;

    /**
     * The node whose view takes a touch at ({@code x}, {@code y}), or
     * {@link #HIT_NONE}, {@link #HIT_FOREIGN} or {@link #HIT_FAILED}.
     */
    static int hitAt(NshView host, float x, float y) {
        return onMain(() -> {
            View hit = topmost(target(host), x, y);
            for (View view = hit; view != null; view = view.getParent() instanceof View
                    ? (View) view.getParent() : null) {
                for (int i = 0; i < host.mounter().views.size(); i++) {
                    if (host.mounter().views.valueAt(i) == view) {
                        return host.mounter().views.keyAt(i);
                    }
                }
            }
            return hit == null ? HIT_NONE : HIT_FOREIGN;
        }, HIT_FAILED);
    }

    /**
     * The innermost view that takes a touch at ({@code x}, {@code y}) of
     * {@code v}, node or not: {@link TouchTracker#deepest} with foreign
     * views, but visiting a group's children as Android dispatches a touch
     * to them and draws them, the highest Z ({@link View#getZ}: elevation
     * and translation) first and, of equal Z, the last one first. A raised
     * view covers a later sibling it is drawn over.
     */
    private static View topmost(View v, float x, float y) {
        int pointerEvents = v instanceof Box ? ((Box) v).pointerEvents : 0;
        if (pointerEvents == 1 || v.getVisibility() != View.VISIBLE) return null;
        if (v instanceof ViewGroup && pointerEvents != 3) {
            ViewGroup g = (ViewGroup) v;
            View[] children = new View[g.getChildCount()];
            for (int i = 0; i < children.length; i++) children[i] = g.getChildAt(i);
            // Stable: of equal Z, the children stay in their order.
            java.util.Arrays.sort(children, (a, b) -> Float.compare(a.getZ(), b.getZ()));
            float[] pt = new float[2];
            for (int i = children.length - 1; i >= 0; i--) {
                View c = children[i];
                pt[0] = x + g.getScrollX() - c.getLeft();
                pt[1] = y + g.getScrollY() - c.getTop();
                android.graphics.Matrix m = c.getMatrix();
                if (!m.isIdentity()) {
                    android.graphics.Matrix inverse = new android.graphics.Matrix();
                    if (m.invert(inverse)) inverse.mapPoints(pt);
                }
                boolean inside = pt[0] >= 0 && pt[1] >= 0 && pt[0] < c.getWidth() && pt[1] < c.getHeight();
                // Outside the child, a descendant placed outside it may
                // still be there (`MountItem::Overflow`).
                if (!inside && !(c instanceof Box && ((Box) c).overflow != null && !((Box) c).clip
                        && ((Box) c).overflow.contains(pt[0], pt[1]))) continue;
                View hit = topmost(c, pt[0], pt[1]);
                if (hit != null) return hit;
            }
        }
        if (pointerEvents == 2) return null;
        // Reached through its overflow area: only its descendants count.
        if (x < 0 || y < 0 || x >= v.getWidth() || y >= v.getHeight()) return null;
        return v;
    }

    /**
     * Where the characters {@code start}..{@code end} (UTF-16 units) of
     * text view {@code id} are on their first line, in the window: x, y,
     * width, height; null when it does not show.
     */
    static float[] textBounds(NshView host, int id, int start, int end) {
        return onMain(() -> {
            View v = host.mounter().views.get(id);
            if (!(v instanceof TextBox) || !v.isShown()) return null;
            TextBox text = (TextBox) v;
            android.text.Layout layout = text.layout();
            if (layout == null) return null;
            // The box of the characters on their first line: the first one (empty
            // for an empty range, or one past the text).
            float[] boxes = TextLayouts.rangeRects(layout, start, end, 1f);
            if (boxes.length < 4) return null;
            int[] at = new int[2];
            v.getLocationInWindow(at);
            return new float[] {
                at[0] + text.contentX() + boxes[0],
                at[1] + text.contentY() + boxes[1],
                boxes[2],
                boxes[3],
            };
        });
    }

    /** The window's content size: width, height. */
    static float[] viewport(NshView host) {
        return onMain(() -> {
            View root = host.getRootView();
            return new float[] {root.getWidth(), root.getHeight()};
        });
    }

    /**
     * The NshView's size: width, height. What {@link #resizeViewport} sets,
     * from the window's top left, where the development host's view is.
     */
    static float[] contentSize(NshView host) {
        return onMain(() -> new float[] {host.getWidth(), host.getHeight()});
    }

    static boolean focusNode(NshView host, int id) {
        return onMain(() -> {
            View view = host.mounter().views.get(id);
            if (!(view instanceof InputBox)) return false;
            InputBox input = (InputBox) view;
            host.getRootView().clearFocus();
            input.edit.setFocusableInTouchMode(true);
            return input.edit.requestFocus();
        }, false);
    }

    static boolean appFocused(NshView host) {
        return onMain(host::hasWindowFocus, false);
    }

    static boolean resizeViewport(NshView host, int width, int height) {
        return onMain(() -> {
            View root = host.getRootView();
            if (width <= 0 || height <= 0 || width > root.getWidth() || height > root.getHeight()) {
                return false;
            }
            android.view.ViewGroup.LayoutParams params = host.getLayoutParams();
            if (params == null) return false;
            params.width = width;
            params.height = height;
            host.setLayoutParams(params);
            host.requestLayout();
            root.requestLayout();
            return true;
        }, false);
    }

    static boolean hover(NshView host, float x, float y) {
        return onMain(() -> {
            View root = target(host);
            long now = SystemClock.uptimeMillis();
            MotionEvent event = MotionEvent.obtain(now, now, MotionEvent.ACTION_HOVER_ENTER,
                    x, y, 0);
            event.setSource(android.view.InputDevice.SOURCE_MOUSE);
            root.dispatchGenericMotionEvent(event);
            event.recycle();
            event = MotionEvent.obtain(now, now, MotionEvent.ACTION_HOVER_MOVE,
                    x, y, 0);
            event.setSource(android.view.InputDevice.SOURCE_MOUSE);
            root.dispatchGenericMotionEvent(event);
            event.recycle();
            return true;
        }, false);
    }

    /** The time the finger went down, for the events of one gesture. */
    private static long downTime;
    /** The window that received the current finger's down. */
    private static View touchTarget;

    /** One finger's `action` (`MotionEvent.ACTION_*`) at `x`, `y` in the window. */
    static void touch(NshView host, int action, float x, float y) {
        onMain(() -> {
            long now = SystemClock.uptimeMillis();
            if (action == MotionEvent.ACTION_DOWN) {
                downTime = now;
                touchTarget = target(host);
            }
            View root = touchTarget != null ? touchTarget : target(host);
            MotionEvent e = MotionEvent.obtain(downTime, now, action, x, y, 0);
            e.setSource(android.view.InputDevice.SOURCE_TOUCHSCREEN);
            root.dispatchTouchEvent(e);
            e.recycle();
            if (action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_CANCEL) touchTarget = null;
            return null;
        });
    }

    /**
     * A finger's drag along ({@code xs}, {@code ys}): down at the first
     * point, a move to each next one a frame ({@code frameMs}) apart, up at
     * the last; scheduled on the UI thread as the display paces real ones.
     * Returns when it is done.
     */
    static void drag(NshView host, float[] xs, float[] ys, int frameMs) {
        java.util.concurrent.CountDownLatch done = new java.util.concurrent.CountDownLatch(1);
        MAIN.post(() -> {
            long start = SystemClock.uptimeMillis();
            View window = target(host);
            int last = xs.length - 1;
            for (int i = 0; i <= last; i++) {
                final int at = i;
                MAIN.postAtTime(() -> {
                    int action = at == 0 ? MotionEvent.ACTION_DOWN
                            : at == last ? MotionEvent.ACTION_UP : MotionEvent.ACTION_MOVE;
                    long now = SystemClock.uptimeMillis();
                    MotionEvent e = MotionEvent.obtain(start, now, action, xs[at], ys[at], 0);
                    e.setSource(android.view.InputDevice.SOURCE_TOUCHSCREEN);
                    window.dispatchTouchEvent(e);
                    e.recycle();
                    if (at == last) done.countDown();
                }, start + (long) at * frameMs);
            }
        });
        try {
            done.await(30, TimeUnit.SECONDS);
        } catch (InterruptedException ignored) {
            Thread.currentThread().interrupt();
        }
    }

    /**
     * Where touches go: the system dialog shown over the app, else the
     * app's topmost `Dialog` (a window of its own), else the app's window.
     */
    private static View target(NshView host) {
        Dialog d = shownDialog;
        if (d != null && d.isShowing() && d.getWindow() != null) return d.getWindow().getDecorView();
        View modal = host.mounter().topModalWindow();
        return modal != null ? modal : host.getRootView();
    }

    /** Types `text` into the focused field, key by key where keys type it. */
    static void type(NshView host, String text) {
        onMain(() -> {
            View focus = host.getRootView().findFocus();
            if (focus == null) return null;
            KeyEvent[] keys = KeyCharacterMap.load(KeyCharacterMap.VIRTUAL_KEYBOARD).getEvents(text.toCharArray());
            if (keys != null) {
                for (KeyEvent k : keys) focus.dispatchKeyEvent(k);
            } else if (focus instanceof EditText) {
                // No key types it (an input method's text): committed as
                // the input method commits it.
                android.view.inputmethod.InputConnection ic =
                        focus.onCreateInputConnection(new android.view.inputmethod.EditorInfo());
                if (ic != null) ic.commitText(text, 1);
            }
            return null;
        });
    }

    /** Presses the key `code` (`KeyEvent.KEYCODE_*`) with `meta` (`KeyEvent.META_*`). */
    static void key(NshView host, int code, int meta) {
        onMain(() -> {
            View focus = host.getRootView().findFocus();
            View to = focus != null ? focus : host.getRootView();
            long now = SystemClock.uptimeMillis();
            KeyEvent down = new KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0, meta);
            // With Ctrl, a shortcut first, as the window's input stage
            // (ViewRootImpl) offers it: Ctrl+A selects all in a text field.
            if ((meta & KeyEvent.META_CTRL_ON) == 0 || !to.dispatchKeyShortcutEvent(down)) {
                to.dispatchKeyEvent(down);
            }
            to.dispatchKeyEvent(new KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0, meta));
            return null;
        });
    }

    /**
     * Presses the key `code` (`KeyEvent.KEYCODE_*`) with `meta` (`KeyEvent.META_*`) on a hardware
     * keyboard: the window's view hierarchy gets it from its root, as the window's input stage
     * (ViewRootImpl) hands it over after the input method: with Control as a shortcut first.
     */
    static void pressKeys(NshView host, int code, int meta) {
        onMain(() -> {
            View window = target(host);
            long now = SystemClock.uptimeMillis();
            KeyEvent down = new KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0, meta,
                    KeyCharacterMap.VIRTUAL_KEYBOARD, 0, 0, android.view.InputDevice.SOURCE_KEYBOARD);
            if ((meta & KeyEvent.META_CTRL_ON) == 0 || !window.dispatchKeyShortcutEvent(down)) {
                window.dispatchKeyEvent(down);
            }
            window.dispatchKeyEvent(KeyEvent.changeAction(down, KeyEvent.ACTION_UP));
            return null;
        });
    }

    /** The text input `id` holds; null when it is no input. */
    static String inputText(NshView host, int id) {
        return onMain(() -> {
            View v = host.mounter().views.get(id);
            return v instanceof InputBox ? ((InputBox) v).edit.getText().toString() : null;
        });
    }

    /** The system's back: what the back button or gesture does. */
    @SuppressWarnings("deprecation")
    static boolean back(NshView host) {
        Boolean done = onMain(() -> {
            Dialog d = shownDialog;
            if (d != null && d.isShowing()) {
                d.onBackPressed();
                return true;
            }
            Activity a = SystemUi.activity(host.getContext());
            if (a == null) return false;
            a.onBackPressed();
            return true;
        });
        return done != null && done;
    }

    /**
     * Where to tap to answer the dialog shown: the middle of its button
     * titled `button` (window pixels of the dialog's window), or null
     * when it has none.
     */
    static float[] dialogPoint(String button) {
        return onMain(() -> {
            Dialog d = shownDialog;
            if (d == null || !d.isShowing() || d.getWindow() == null) return null;
            View found = find(d.getWindow().getDecorView(), button);
            if (found == null) return null;
            int[] at = new int[2];
            found.getLocationInWindow(at);
            return new float[] {at[0] + found.getWidth() / 2f, at[1] + found.getHeight() / 2f};
        });
    }

    private static View find(View v, String text) {
        if (v instanceof TextView && v.isShown() && text.contentEquals(((TextView) v).getText())) {
            // A list item's text, or a button's.
            return v;
        }
        if (v instanceof ViewGroup) {
            ViewGroup g = (ViewGroup) v;
            for (int i = 0; i < g.getChildCount(); i++) {
                View f = find(g.getChildAt(i), text);
                if (f != null) return f;
            }
        }
        return null;
    }

    /** What moved last time {@link #animating} looked. */
    private static long lastSignature;

    /**
     * Whether something moves: a view's place, offset, scroll position or
     * opacity changed since the last look (a screen sliding in, a dialog
     * fading, a list gliding after a fling), whatever animates it, or a
     * scroll view drew (its overscroll stretch relaxing).
     */
    static boolean animating(NshView host) {
        Boolean moving = onMain(() -> {
            long sig = signature(host.getRootView());
            Dialog d = shownDialog;
            if (d != null && d.isShowing() && d.getWindow() != null) {
                sig = sig * 31 + signature(d.getWindow().getDecorView());
            }
            boolean changed = sig != lastSignature;
            lastSignature = sig;
            return changed;
        });
        return moving != null && moving;
    }

    private static long signature(View v) {
        if (v.getVisibility() != View.VISIBLE) return 0;
        long h = v.getLeft();
        h = h * 31 + v.getTop();
        h = h * 31 + Float.floatToIntBits(v.getTranslationX());
        h = h * 31 + Float.floatToIntBits(v.getTranslationY());
        h = h * 31 + Float.floatToIntBits(v.getAlpha());
        h = h * 31 + Float.floatToIntBits(v.getScaleX());
        h = h * 31 + v.getScrollX();
        h = h * 31 + v.getScrollY();
        if (v instanceof ScrollBox) h = h * 31 + ((ScrollBox) v).frames;
        if (v instanceof ViewGroup) {
            ViewGroup g = (ViewGroup) v;
            for (int i = 0; i < g.getChildCount(); i++) h = h * 31 + signature(g.getChildAt(i));
        }
        return h;
    }

    /** A picture of the window (and the dialog over it), as a PNG. */
    static byte[] screenshot(NshView host) {
        return onMain(() -> {
            View root = host.getRootView();
            if (root.getWidth() == 0 || root.getHeight() == 0) return null;
            Bitmap b = Bitmap.createBitmap(root.getWidth(), root.getHeight(), Bitmap.Config.ARGB_8888);
            Canvas c = new Canvas(b);
            root.draw(c);
            Dialog d = shownDialog;
            if (d != null && d.isShowing() && d.getWindow() != null) {
                View dv = d.getWindow().getDecorView();
                int[] at = new int[2];
                dv.getLocationOnScreen(at);
                int[] rootAt = new int[2];
                root.getLocationOnScreen(rootAt);
                c.translate(at[0] - rootAt[0], at[1] - rootAt[1]);
                dv.draw(c);
            }
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            b.compress(Bitmap.CompressFormat.PNG, 100, out);
            b.recycle();
            return out.toByteArray();
        });
    }
}
