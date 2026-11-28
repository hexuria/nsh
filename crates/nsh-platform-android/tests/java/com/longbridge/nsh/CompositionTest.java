package com.longbridge.nsh;

/**
 * Composition replay arithmetic. Plain JDK, no Android APIs:
 *
 * <pre>
 * javac -d target/java-tests \
 *   crates/nsh-platform-android/java/com/longbridge/nsh/Composition.java \
 *   crates/nsh-platform-android/tests/java/com/longbridge/nsh/CompositionTest.java
 * java -cp target/java-tests com.longbridge.nsh.CompositionTest
 * </pre>
 */
public final class CompositionTest {
    public static void main(String[] args) {
        check("the span already in the text wins over an empty restarted connection",
            "1234.56".contentEquals(Composition.before("1234.56", null)));
        check("the connection cache is the fallback", "12".contentEquals(Composition.before(null, "12")));
        check("a replayed composition types nothing again",
            Composition.committedAddition("1234.56", "1234.56").isEmpty());
        check("only the suffix of an extended composition is committed",
            "7".equals(Composition.committedAddition("1234.56", "1234.567")));
        check("different committed text is not discarded",
            "x".equals(Composition.committedAddition("1234.56", "x")));
        System.out.println("CompositionTest: ok");
    }

    private static void check(String what, boolean ok) {
        if (!ok) throw new AssertionError(what);
    }
}
