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
        check("cursor anchor", shell.getLastCursorAnchorInfo() != null, "null");

        float[] edgeBox = BubbleNative.INSTANCE.nodeBox(10);
        int edgeHandle = BubbleNative.INSTANCE.focusAt(
                edgeBox[0] + edgeBox[2] / 2f,
                edgeBox[1] + edgeBox[3] / 2f);
        check("focus edge field", edgeHandle == 10, "handle=" + edgeHandle);
        InputConnection edge = shell.onCreateInputConnection(new EditorInfo());
        check("edge connection", edge != null, "null");
        if (edge == null) return;

        check("edge commit ab", edge.commitText("ab", 1), "commitText");
        check("edge selection two", edge.setSelection(2, 2), "setSelection");
        check("edge negative cursor", edge.commitText("C", -1), "commitText");
        check("edge negative value", "abC".equals(BubbleNative.INSTANCE.nodeText(10)), BubbleNative.INSTANCE.nodeText(10));
        long[] negative = BubbleNative.INSTANCE.editSnapshot();
        check("edge negative selection", negative.length >= 7 && negative[2] == 1 && negative[3] == 1,
                java.util.Arrays.toString(negative));
        check("edge text before", "a".contentEquals(edge.getTextBeforeCursor(1, 0)),
                String.valueOf(edge.getTextBeforeCursor(1, 0)));
        check("edge text after", "bC".contentEquals(edge.getTextAfterCursor(2, 0)),
                String.valueOf(edge.getTextAfterCursor(2, 0)));

        check("edge selection start", edge.setSelection(0, 0), "setSelection");
        check("edge positive cursor", edge.commitText("D", 2), "commitText");
        check("edge positive value", "DabC".equals(BubbleNative.INSTANCE.nodeText(10)), BubbleNative.INSTANCE.nodeText(10));
        long[] positive = BubbleNative.INSTANCE.editSnapshot();
        check("edge positive selection", positive.length >= 7 && positive[2] == 2 && positive[3] == 2,
                java.util.Arrays.toString(positive));
        check("edge selection reset", edge.setSelection(0, 0), "setSelection");
        check("edge zero cursor", edge.commitText("E", 0), "commitText");
        check("edge zero value", "EDabC".equals(BubbleNative.INSTANCE.nodeText(10)), BubbleNative.INSTANCE.nodeText(10));
        long[] zero = BubbleNative.INSTANCE.editSnapshot();
        check("edge zero selection", zero.length >= 7 && zero[2] == 0 && zero[3] == 0,
                java.util.Arrays.toString(zero));

        check("edge select range", edge.setSelection(0, 2), "setSelection");
        check("edge selected text", "ED".contentEquals(edge.getSelectedText(0)),
                String.valueOf(edge.getSelectedText(0)));
        check("edge text after range", "ab".contentEquals(edge.getTextAfterCursor(2, 0)),
                String.valueOf(edge.getTextAfterCursor(2, 0)));

        check("edge emoji select", edge.setSelection(2, 2), "setSelection");
        check("edge emoji commit", edge.commitText("😀", 1), "commitText");
        check("edge emoji value", "ED😀abC".equals(BubbleNative.INSTANCE.nodeText(10)),
                BubbleNative.INSTANCE.nodeText(10));
        check("edge emoji select cursor", edge.setSelection(4, 4), "setSelection");
        check("edge codepoint delete", edge.deleteSurroundingTextInCodePoints(1, 0),
                "deleteSurroundingTextInCodePoints");
        check("edge codepoint value", "EDabC".equals(BubbleNative.INSTANCE.nodeText(10)),
                BubbleNative.INSTANCE.nodeText(10));

        check("edge compose region", edge.setComposingRegion(0, 2), "setComposingRegion");
        long[] region = BubbleNative.INSTANCE.editSnapshot();
        check("edge region range", region.length >= 7 && region[4] == 0 && region[5] == 2 && region[6] == 1,
                java.util.Arrays.toString(region));
        check("edge compose replace", edge.setComposingText("日本", 1), "setComposingText");
        check("edge compose value", "日本abC".equals(BubbleNative.INSTANCE.nodeText(10)),
                BubbleNative.INSTANCE.nodeText(10));
        check("edge inner selection", edge.setSelection(1, 2), "setSelection");
        check("edge inner selected", "本".contentEquals(edge.getSelectedText(0)),
                String.valueOf(edge.getSelectedText(0)));
        long[] inner = BubbleNative.INSTANCE.editSnapshot();
        check("edge inner range", inner.length >= 7 && inner[2] == 1 && inner[3] == 2
                        && inner[4] == 0 && inner[5] == 2 && inner[6] == 1,
                java.util.Arrays.toString(inner));
        check("edge compose delete", edge.deleteSurroundingText(1, 0), "deleteSurroundingText");
        check("edge compose delete value", "abC".equals(BubbleNative.INSTANCE.nodeText(10)),
                BubbleNative.INSTANCE.nodeText(10));
        long[] composeDeleted = BubbleNative.INSTANCE.editSnapshot();
        check("edge compose delete cleared", composeDeleted.length >= 7
                        && composeDeleted[5] == 0xFFFFFFFFL && composeDeleted[2] == 0 && composeDeleted[3] == 0,
                java.util.Arrays.toString(composeDeleted));

        long[] beforeBatch = BubbleNative.INSTANCE.editSnapshot();
        check("edge batch outer", edge.beginBatchEdit(), "beginBatchEdit");
        check("edge batch inner", edge.beginBatchEdit(), "beginBatchEdit");
        check("edge batch x", edge.commitText("X", 1), "commitText");
        check("edge batch y", edge.commitText("Y", 1), "commitText");
        check("edge batch value", "XYabC".equals(BubbleNative.INSTANCE.nodeText(10)),
                BubbleNative.INSTANCE.nodeText(10));
        long[] midBatch = BubbleNative.INSTANCE.editSnapshot();
        check("edge batch quiet revision", midBatch.length >= 7 && midBatch[1] == beforeBatch[1],
                java.util.Arrays.toString(midBatch));
        check("edge batch inner end", edge.endBatchEdit(), "endBatchEdit");
        long[] innerEnd = BubbleNative.INSTANCE.editSnapshot();
        check("edge batch inner revision", innerEnd.length >= 7 && innerEnd[1] == beforeBatch[1],
                java.util.Arrays.toString(innerEnd));
        check("edge batch outer end", edge.endBatchEdit(), "endBatchEdit");
        long[] afterBatch = BubbleNative.INSTANCE.editSnapshot();
        check("edge batch one revision", afterBatch.length >= 7 && afterBatch[1] == beforeBatch[1] + 1,
                java.util.Arrays.toString(afterBatch));

        edge.closeConnection();
        check("edge closed connection rejected", !edge.commitText("Q", 1), "closed connection accepted input");

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
