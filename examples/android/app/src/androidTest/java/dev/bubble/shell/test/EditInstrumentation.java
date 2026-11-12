package dev.bubble.shell.test;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;
import android.util.Log;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;

import dev.bubble.shell.BubbleNative;
import dev.bubble.shell.BubbleView;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;

/** Drives the focused field through the real Android InputConnection. */
public class EditInstrumentation extends Instrumentation {
    private static final String TARGET = "dev.bubble.shell";
    private static final String ACTIVITY = "dev.bubble.shell.MainActivity";
    private final JSONArray checks = new JSONArray();
    private boolean passed = true;
    private Activity activity;

    @Override
    public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        start();
    }

    @Override
    public void onStart() {
        super.onStart();
        try {
            Intent intent = new Intent();
            intent.setClassName(TARGET, ACTIVITY);
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
            intent.putExtra("bubble_gestures", true);
            intent.putExtra("bubble_fixture", "editing");
            activity = startActivitySync(intent);
            waitForIdleSync();
            SystemClock.sleep(300);
            waitForIdleSync();
            runOnMainSync(this::runEdits);
        } catch (Throwable error) {
            check("runner", false, String.valueOf(error));
        }
        try {
            JSONObject json = new JSONObject()
                    .put("suite", "edit.android")
                    .put("ok", passed)
                    .put("checks", checks);
            Log.i("BUBBLE_EDIT", json.toString());
            File file = new File(getTargetContext().getFilesDir(), "edit-result.json");
            try (FileOutputStream out = new FileOutputStream(file)) {
                out.write(json.toString().getBytes(StandardCharsets.UTF_8));
            }
        } catch (Throwable error) {
            Log.e("BUBBLE_EDIT", "report", error);
        }
        Bundle results = new Bundle();
        results.putString(REPORT_KEY_IDENTIFIER, "edit.android");
        results.putString(REPORT_KEY_STREAMRESULT, passed ? "edit.android ok\n" : "edit.android failed\n");
        finish(passed ? Activity.RESULT_OK : Activity.RESULT_CANCELED, results);
    }

    private void runEdits() {
        View root = activity.findViewById(android.R.id.content);
        View view = root instanceof ViewGroup ? ((ViewGroup) root).getChildAt(0) : null;
        if (!(view instanceof BubbleView)) {
            check("view", false, String.valueOf(view));
            return;
        }
        BubbleView shell = (BubbleView) view;
        float[] box = BubbleNative.INSTANCE.nodeBox(4);
        int handle = BubbleNative.INSTANCE.focusAt(box[0] + box[2] / 2f, box[1] + box[3] / 2f);
        check("focus", handle == 4, "handle=" + handle + " box=" + box[0] + "," + box[1]);
        if (handle != 4) return;

        EditorInfo info = new EditorInfo();
        InputConnection ic = shell.onCreateInputConnection(info);
        check("connection", ic != null, "null");
        if (ic == null) return;

        check("commit a", ic.commitText("a", 1), "commitText");
        check("commit b", ic.commitText("b", 1), "commitText");
        check("value ab", "ab".equals(BubbleNative.INSTANCE.nodeText(4)), BubbleNative.INSTANCE.nodeText(4));

        check("compose に", ic.setComposingText("に", 1), "setComposingText");
        check("compose 日本", ic.setComposingText("日本", 1), "setComposingText");
        long[] composing = BubbleNative.INSTANCE.editSnapshot();
        check("composing range", composing.length >= 7 && composing[5] == 4 && composing[6] == 1,
                "length=" + composing.length + " snapshot=" + java.util.Arrays.toString(composing));
        check("composed value", "ab日本".equals(BubbleNative.INSTANCE.nodeText(4)), BubbleNative.INSTANCE.nodeText(4));

        check("finish composing", ic.finishComposingText(), "finishComposingText");
        long[] finished = BubbleNative.INSTANCE.editSnapshot();
        check("composition cleared", finished.length >= 7 && finished[5] == 0xFFFFFFFFL, java.util.Arrays.toString(finished));

        check("delete surrounding", ic.deleteSurroundingText(1, 0), "deleteSurroundingText");
        check("deleted value", "ab日".equals(BubbleNative.INSTANCE.nodeText(4)), BubbleNative.INSTANCE.nodeText(4));

        check("text before", "b日".contentEquals(ic.getTextBeforeCursor(2, 0)), String.valueOf(ic.getTextBeforeCursor(2, 0)));
        check("selected none", ic.getSelectedText(0) == null, String.valueOf(ic.getSelectedText(0)));
        check("extracted", ic.getExtractedText(new android.view.inputmethod.ExtractedTextRequest(), 0) != null, "null");

        check("batch begin", ic.beginBatchEdit(), "beginBatchEdit");
        check("batch x", ic.commitText("X", 1), "commitText");
        check("batch y", ic.commitText("Y", 1), "commitText");
        check("batch value", "ab日XY".equals(BubbleNative.INSTANCE.nodeText(4)), BubbleNative.INSTANCE.nodeText(4));
        check("batch end", ic.endBatchEdit(), "endBatchEdit");

        float[] other = BubbleNative.INSTANCE.nodeBox(7);
        int next = BubbleNative.INSTANCE.focusAt(other[0] + other[2] / 2f, other[1] + other[3] / 2f);
        check("focus other", next == 7, "handle=" + next);
        check("stale connection rejected", !ic.commitText("Z", 1), "old connection accepted input");
    }

    private void check(String name, boolean ok, String detail) {
        try {
            checks.put(new JSONObject().put("name", name).put("ok", ok).put("detail", detail));
        } catch (Throwable ignored) {
        }
        if (!ok) {
            passed = false;
            Log.w("BUBBLE_EDIT", name + ": " + detail);
        }
    }
}
