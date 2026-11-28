package com.longbridge.nsh;

/** Input-composition arithmetic that does not need Android classes. */
final class Composition {
    private Composition() {}

    /**
     * The composition as the text itself has it, falling back to what the
     * connection last saw. A restarted input method can replay its cached
     * composition through a new connection whose cache is empty.
     */
    static CharSequence before(CharSequence inText, CharSequence cached) {
        return inText != null ? inText : cached;
    }

    /** The part of a commit that is not already present in the composition. */
    static String committedAddition(CharSequence before, CharSequence committed) {
        String added = committed.toString();
        if (before != null && added.startsWith(before.toString())) return added.substring(before.length());
        return added;
    }
}
