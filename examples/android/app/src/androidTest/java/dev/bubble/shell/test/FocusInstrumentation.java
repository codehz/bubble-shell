package dev.bubble.shell.test;

import android.app.Activity;
import android.app.Instrumentation;
import android.app.UiAutomation;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;
import android.util.Log;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;

import dev.bubble.shell.BubbleView;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.TimeoutException;

/** Exercises modal isolation through Android's service-facing tree and window input. */
public class FocusInstrumentation extends Instrumentation {
    private final JSONArray checks = new JSONArray();
    private boolean passed = true;
    private Activity activity;
    private UiAutomation ui;
    private BubbleView shell;

    @Override
    public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        start();
    }

    @Override
    public void onStart() {
        super.onStart();
        long started = SystemClock.elapsedRealtime();
        try {
            ui = getUiAutomation();
            Intent intent = new Intent();
            intent.setClassName("dev.bubble.shell", "dev.bubble.shell.MainActivity");
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
            intent.putExtra("bubble_gestures", true);
            intent.putExtra("bubble_fixture", "focus");
            activity = startActivitySync(intent);
            waitForIdleSync();
            runOnMainSync(() -> {
                ViewGroup root = activity.findViewById(android.R.id.content);
                View view = root.getChildAt(0);
                shell = (BubbleView) view;
                shell.requestFocus();
            });
            runFocus();
        } catch (Throwable error) {
            check("runner", false, String.valueOf(error));
        }
        try {
            JSONObject report = new JSONObject()
                    .put("suite", "focus.android")
                    .put("ok", passed)
                    .put("elapsed_ms", SystemClock.elapsedRealtime() - started)
                    .put("checks", checks);
            try (FileOutputStream out = new FileOutputStream(
                    new File(getTargetContext().getFilesDir(), "focus-result.json"))) {
                out.write(report.toString().getBytes(StandardCharsets.UTF_8));
            }
            Log.i("BUBBLE_FOCUS", report.toString());
        } catch (Throwable error) {
            passed = false;
            Log.e("BUBBLE_FOCUS", "report", error);
        }
        Bundle result = new Bundle();
        result.putString(REPORT_KEY_IDENTIFIER, "focus.android");
        result.putString(REPORT_KEY_STREAMRESULT, passed ? "focus.android ok\n" : "focus.android failed\n");
        finish(passed ? Activity.RESULT_OK : Activity.RESULT_CANCELED, result);
    }

    private void runFocus() {
        require("background ready", () -> visible("Open dialog") && visible("Background field"));
        AccessibilityNodeInfo background = find("Background field");
        AccessibilityNodeInfo opener = find("Open dialog");
        check("accessibility focus action advertised", background.getActionList().contains(
                AccessibilityNodeInfo.AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS), "Background field");
        AccessibilityNodeInfo focusSource = focusEvent(background);
        check("accessibility focus event identifies virtual field", focusSource != null
                && "Background field".contentEquals(focusSource.getContentDescription()),
                focusSource == null ? "null" : String.valueOf(focusSource.getContentDescription()));
        require("service accessibility focus", () -> accessibilityFocused("Background field"));
        require("service finds accessibility focus", () -> serviceFocus(
                AccessibilityNodeInfo.FOCUS_ACCESSIBILITY, "Background field"));

        key(KeyEvent.KEYCODE_TAB, false);
        require("Tab selects opener", () -> focused("Open dialog"));
        require("service finds keyboard focus", () -> serviceFocus(
                AccessibilityNodeInfo.FOCUS_INPUT, "Open dialog"));
        key(KeyEvent.KEYCODE_ENTER, false);
        require("Enter opens dialog", () -> visible("Settings") && visible("Dialog field"));
        require("modal hides background", () -> !visible("Open dialog") && !visible("Background field"));
        require("modal clears background accessibility focus", () -> !accessibilityFocused("Background field"));
        check("stale background focus rejected", !background.performAction(
                AccessibilityNodeInfo.ACTION_FOCUS), "background node retained before modal opened");
        check("stale background activation rejected", !opener.performAction(
                AccessibilityNodeInfo.ACTION_CLICK), "opener retained before modal opened");
        Bundle replacement = new Bundle();
        replacement.putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, "Blocked");
        check("stale background edit rejected", !background.performAction(
                AccessibilityNodeInfo.ACTION_SET_TEXT, replacement), "background field remains protected");

        key(KeyEvent.KEYCODE_TAB, false);
        require("modal Tab starts at field", () -> focused("Dialog field"));
        InputConnection[] connection = new InputConnection[1];
        runOnMainSync(() -> connection[0] = shell.onCreateInputConnection(new EditorInfo()));
        boolean[] committed = new boolean[1];
        runOnMainSync(() -> committed[0] = connection[0].commitText("!", 1));
        check("keyboard focused field accepts IME", committed[0], "commitText");
        require("IME value reaches service tree", () -> text("Dialog field").contains("!"));
        key(KeyEvent.KEYCODE_TAB, false);
        require("Tab leaves IME for button", () -> focused("Open nested dialog"));
        runOnMainSync(() -> committed[0] = connection[0].commitText("stale", 1));
        check("old IME connection rejected after Tab", !committed[0], "field session ended");
        key(KeyEvent.KEYCODE_TAB, false);
        require("Tab reaches close button", () -> focused("Close dialog"));
        key(KeyEvent.KEYCODE_TAB, false);
        require("Tab wraps inside modal", () -> focused("Dialog field"));
        key(KeyEvent.KEYCODE_TAB, true);
        require("Shift-Tab wraps inside modal", () -> focused("Close dialog"));
        key(KeyEvent.KEYCODE_ESCAPE, false);
        require("Esc clears focus without closing", () -> visible("Settings") && !anyInputFocus());
        key(KeyEvent.KEYCODE_TAB, true);
        require("Shift-Tab restarts at modal end", () -> focused("Close dialog"));

        AccessibilityNodeInfo outerField = find("Dialog field");
        AccessibilityNodeInfo outerClose = find("Close dialog");
        check("service focuses nested opener", find("Open nested dialog").performAction(
                AccessibilityNodeInfo.ACTION_FOCUS), "Open nested dialog");
        require("service input focus published", () -> focused("Open nested dialog"));
        key(KeyEvent.KEYCODE_SPACE, false);
        require("Space opens nested dialog", () -> visible("Confirmation") && visible("Close nested dialog"));
        require("nested modal hides outer controls", () -> !visible("Dialog field") && !visible("Close dialog"));
        check("nested modal rejects outer focus", !outerField.performAction(
                AccessibilityNodeInfo.ACTION_FOCUS), "outer field");
        check("nested modal rejects outer close", !outerClose.performAction(
                AccessibilityNodeInfo.ACTION_CLICK), "outer close");
        key(KeyEvent.KEYCODE_TAB, false);
        require("nested Tab selects sole control", () -> focused("Close nested dialog"));
        key(KeyEvent.KEYCODE_TAB, false);
        require("nested Tab stays inside", () -> focused("Close nested dialog"));
        key(KeyEvent.KEYCODE_TAB, true);
        require("nested Shift-Tab stays inside", () -> focused("Close nested dialog"));
        check("service closes nested dialog", find("Close nested dialog").performAction(
                AccessibilityNodeInfo.ACTION_CLICK), "Close nested dialog");
        require("outer dialog returns", () -> visible("Dialog field") && !visible("Confirmation")
                && !visible("Background field"));
        check("service focuses restored field", find("Dialog field").performAction(
                AccessibilityNodeInfo.ACTION_FOCUS), "Dialog field");
        require("restored field focus published", () -> focused("Dialog field"));
        Bundle changed = new Bundle();
        changed.putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, "Changed");
        check("service edits restored field", find("Dialog field").performAction(
                AccessibilityNodeInfo.ACTION_SET_TEXT, changed), "Changed");
        require("service edit value published", () -> "Changed".equals(text("Dialog field")));
        check("service closes outer dialog", find("Close dialog").performAction(
                AccessibilityNodeInfo.ACTION_CLICK), "Close dialog");
        require("background returns after close", () -> visible("Background field") && !visible("Settings"));
        require("background value was not changed", () -> "Background".equals(text("Background field")));
        key(KeyEvent.KEYCODE_TAB, false);
        require("Tab restarts in background", () -> focused("Open dialog"));
        key(KeyEvent.KEYCODE_TAB, false);
        require("background field reachable again", () -> focused("Background field"));
    }

    private void key(int code, boolean shift) {
        long now = SystemClock.uptimeMillis();
        int meta = shift ? KeyEvent.META_SHIFT_ON | KeyEvent.META_SHIFT_LEFT_ON : 0;
        sendKeySync(new KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0, meta));
        sendKeySync(new KeyEvent(now, SystemClock.uptimeMillis(), KeyEvent.ACTION_UP, code, 0, meta));
    }

    private AccessibilityNodeInfo focusEvent(AccessibilityNodeInfo node) {
        try {
            android.view.accessibility.AccessibilityEvent event = ui.executeAndWaitForEvent(
                    () -> node.performAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS),
                    candidate -> candidate.getEventType() == android.view.accessibility.AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED,
                    3000);
            return event.getSource();
        } catch (TimeoutException error) {
            check("accessibility focus event", false, error.toString());
            return null;
        }
    }

    private boolean serviceFocus(int type, String label) {
        AccessibilityNodeInfo root = ui.getRootInActiveWindow();
        AccessibilityNodeInfo focus = root == null ? null : root.findFocus(type);
        return focus != null && label.contentEquals(
                focus.getContentDescription() == null ? "" : focus.getContentDescription());
    }

    private AccessibilityNodeInfo find(String label) {
        AccessibilityNodeInfo root = ui.getRootInActiveWindow();
        if (root != null) root.refresh();
        return find(root, label);
    }

    private AccessibilityNodeInfo find(AccessibilityNodeInfo node, String label) {
        if (node == null) return null;
        if (label.contentEquals(node.getContentDescription() == null ? "" : node.getContentDescription())
                && "dev.bubble.shell".contentEquals(node.getPackageName() == null ? "" : node.getPackageName())) {
            return node;
        }
        for (int index = 0; index < node.getChildCount(); index++) {
            AccessibilityNodeInfo found = find(node.getChild(index), label);
            if (found != null) return found;
        }
        return null;
    }

    private boolean visible(String label) {
        AccessibilityNodeInfo node = find(label);
        return node != null && node.isVisibleToUser();
    }

    private boolean focused(String label) {
        AccessibilityNodeInfo node = find(label);
        return node != null && node.isFocused();
    }

    private boolean accessibilityFocused(String label) {
        AccessibilityNodeInfo node = find(label);
        return node != null && node.isAccessibilityFocused();
    }

    private boolean anyInputFocus() {
        for (String label : new String[] {"Open dialog", "Background field", "Dialog field",
                "Open nested dialog", "Close dialog", "Close nested dialog"}) {
            if (focused(label)) return true;
        }
        return false;
    }

    private String text(String label) {
        AccessibilityNodeInfo node = find(label);
        return node == null || node.getText() == null ? "" : node.getText().toString();
    }

    private interface Condition { boolean satisfied(); }

    private void require(String name, Condition condition) {
        long deadline = SystemClock.uptimeMillis() + 3000;
        do {
            if (condition.satisfied()) {
                check(name, true, "observed through UiAutomation");
                return;
            }
            SystemClock.sleep(32);
        } while (SystemClock.uptimeMillis() < deadline);
        String[] semantics = new String[1];
        runOnMainSync(() -> semantics[0] = dev.bubble.shell.BubbleNative.INSTANCE.semantics());
        check(name, false, "condition not observed within 3s; core=" + semantics[0]
                + "; service=" + describe(ui.getRootInActiveWindow()));
        throw new AssertionError(name);
    }

    private String describe(AccessibilityNodeInfo node) {
        if (node == null) return "null";
        StringBuilder result = new StringBuilder();
        result.append('[').append(node.getContentDescription()).append(" visible=")
                .append(node.isVisibleToUser()).append(" focused=").append(node.isFocused());
        for (int index = 0; index < node.getChildCount(); index++) {
            result.append(describe(node.getChild(index)));
        }
        return result.append(']').toString();
    }

    private void check(String name, boolean ok, String detail) {
        try {
            checks.put(new JSONObject().put("name", name).put("ok", ok).put("detail", detail));
        } catch (Exception error) {
            throw new RuntimeException(error);
        }
        if (!ok) {
            passed = false;
            Log.w("BUBBLE_FOCUS", name + ": " + detail);
        }
    }
}
