package dev.bubble.shell.test;

import android.accessibilityservice.AccessibilityService;
import android.app.Activity;
import android.app.Instrumentation;
import android.app.UiAutomation;
import android.content.Intent;
import android.os.Build;
import android.os.Bundle;
import android.os.SystemClock;
import android.util.Log;
import android.view.Display;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;

import dev.bubble.shell.BubbleNative;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;

/**
 * Drives the gesture fixture with real MotionEvents. The activity owns the
 * engine; this process only injects input and reads nodeText/nodeBox/scroll.
 */
public class ShellInstrumentation extends Instrumentation {
    private static final String TARGET = "dev.bubble.shell";
    private static final String ACTIVITY = "dev.bubble.shell.MainActivity";

    private final JSONArray checks = new JSONArray();
    private final Space space = new Space();
    private Activity activity;
    private UiAutomation ui;
    private Touch touch;
    private long started;
    private boolean passed;

    @Override
    public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        start();
    }

    @Override
    public void onStart() {
        super.onStart();
        started = SystemClock.elapsedRealtime();
        String json = "{\"suite\":\"gestures.android\",\"ok\":false,\"elapsed_ms\":0,\"checks\":[]}";
        try {
            ui = getUiAutomation();
            touch = new Touch();
            deleteResult();
            launch();
            if (activity == null) {
                check("activity", false, "startActivitySync returned null");
            } else {
                check("window focus", awaitFocus(true, 3000), "focused=" + windowFocused());
                warmUp();
                runAll();
            }
        } catch (Throwable error) {
            check("runner", false, String.valueOf(error));
        }
        try {
            json = report();
            writeResult(json);
            Log.i("BUBBLE_GESTURES", json);
        } catch (Throwable error) {
            passed = false;
            Log.e("BUBBLE_GESTURES", "report " + error);
        }
        Bundle results = new Bundle();
        results.putString(REPORT_KEY_IDENTIFIER, "gestures.android");
        results.putString(REPORT_KEY_STREAMRESULT, passed ? "gestures.android ok\n" : "gestures.android failed\n");
        finish(passed ? Activity.RESULT_OK : Activity.RESULT_CANCELED, results);
    }

    private void warmUp() {
        SystemClock.sleep(200);
        JSONObject initial = awaitFresh(null, "tap", 3000);
        if (initial == null) return;
        Rect pad = rect(initial, "pad");
        touch.begin(0, space.x(pad.cx()), space.y(pad.cy()));
        touch.up(0);
        touch.abandon();
        touch.takeFailure();
        String before = nodeText();
        runOnMainSync(() -> BubbleNative.INSTANCE.postEval("__gestureSelect('tap')"));
        if (awaitFresh(before, "tap", 5000) != null) space.read();
    }

    private void launch() {
        Intent intent = new Intent();
        intent.setClassName(TARGET, ACTIVITY);
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
        intent.putExtra("bubble_gestures", true);
        activity = startActivitySync(intent);
        waitForIdleSync();
    }

    private void resumeApp() {
        Intent intent = new Intent();
        intent.setClassName(TARGET, ACTIVITY);
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK
                | Intent.FLAG_ACTIVITY_SINGLE_TOP
                | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT);
        intent.putExtra("bubble_gestures", true);
        final Intent launch = intent;
        runOnMainSync(() -> activity.startActivity(launch));
        waitForIdleSync();
    }

    private void runAll() {
        JSONObject tap = awaitFresh(null, "tap", 8000);
        check("tap ready", tap != null, tap == null ? clip(nodeText()) : "pad " + padOf(tap));
        if (tap == null) return;
        space.read();
        faultsCheck("gesture mode skips m1");
        scenario("tap", () -> tapScene(tap));
        runScene("long", this::longScene);
        runScene("double", this::doubleScene);
        runScene("pan", this::panScene);
        runScene("transform", this::transformScene);
        runScene("parallel", this::parallelScene);
        runScene("reserve", this::reserveScene);
        runScene("contain", this::containScene);
        runScene("dispose", this::disposeScene);
        runScene("lifecycle", this::lifecycleScene);
        faultsCheck("gesture mode still skips m1");
    }

    private void runScene(String scene, Scene body) {
        JSONObject status = select(scene);
        if (status == null) return;
        scenario(scene, () -> body.run(status));
    }

    private void scenario(String name, Runnable body) {
        try {
            body.run();
        } catch (Throwable error) {
            check(name + " scenario", false, String.valueOf(error));
        } finally {
            if (touch != null && touch.open) touch.cancel();
            if (touch != null) {
                String failure = touch.takeFailure();
                if (failure != null) check(name + " inject", false, failure);
            }
        }
    }

    private JSONObject select(String scene) {
        String before = nodeText();
        final String source = "__gestureSelect('" + scene + "')";
        runOnMainSync(() -> BubbleNative.INSTANCE.postEval(source));
        JSONObject status = awaitFresh(before, scene, 5000);
        check(scene + " ready", status != null, status == null ? clip(nodeText()) : "pad " + padOf(status));
        if (status != null) space.read();
        return status;
    }

    private JSONObject awaitFresh(String before, String scene, long timeout) {
        long deadline = SystemClock.uptimeMillis() + timeout;
        while (SystemClock.uptimeMillis() < deadline) {
            String text = nodeText();
            JSONObject json = parse(text);
            if (json != null && (before == null || !text.equals(before)) && fresh(json, scene) && laidOut(json)) {
                return json;
            }
            SystemClock.sleep(32);
        }
        return null;
    }

    private static boolean fresh(JSONObject json, String scene) {
        return scene.equals(json.optString("scene"))
                && json.optBoolean("ready")
                && countEvents(json) == 0
                && padOf(json) != 0;
    }

    private boolean laidOut(JSONObject json) {
        Rect pad = rect(json, "pad");
        return pad.w > 1f && pad.h > 1f;
    }

    private void tapScene(JSONObject status) {
        Rect pad = rect(status, "pad");
        float x = space.x(pad.cx());
        float y = space.y(pad.cy());
        touch.begin(0, x, y);
        JSONObject held = holdStatus(280);
        check("held press", count(held, "tap", "pressStart") == 1, eventsOf(held));
        check("held has no tap", count(held, "tap", "tap") == 0, eventsOf(held));
        touch.cancel();
        JSONObject cancelled = awaitCount("tap", "pressCancel", 1, 1200);
        check("cancel press", count(cancelled, "tap", "pressCancel") == 1, eventsOf(cancelled));
        check("cancel suppresses tap", count(cancelled, "tap", "tap") == 0, eventsOf(cancelled));
        check("cancel has no pressEnd", count(cancelled, "tap", "pressEnd") == 0, eventsOf(cancelled));
        touch.begin(0, x, y);
        touch.up(0);
        awaitCount("tap", "tap", 1, 1200);
        JSONObject done = holdStatus(200);
        check("platform tap", count(done, "tap", "tap") == 1, eventsOf(done));
        check("platform one terminal",
                count(done, "tap", "tap") == 1 && count(done, "tap", "pressEnd") == 1,
                eventsOf(done));
    }

    private void longScene(JSONObject status) {
        Rect pad = rect(status, "pad");
        touch.begin(0, space.x(pad.cx()), space.y(pad.cy()));
        SystemClock.sleep(700);
        JSONObject started = awaitCount("long", "start", 1, 1000);
        check("long start", count(started, "long", "start") == 1, eventsOf(started));
        touch.up(0);
        awaitCount("long", "end", 1, 1200);
        JSONObject done = holdStatus(250);
        check("long one terminal",
                count(done, "long", "end") == 1 && count(done, "long", "cancel") == 0,
                eventsOf(done));
        check("long no tap", count(done, "tap", "tap") == 0, eventsOf(done));
    }

    private void doubleScene(JSONObject status) {
        Rect pad = rect(status, "pad");
        float x = space.x(pad.cx());
        float y = space.y(pad.cy());
        touch.begin(0, x, y);
        touch.up(0);
        SystemClock.sleep(80);
        touch.begin(0, x, y);
        touch.up(0);
        awaitCount("double", "doubleTap", 1, 1000);
        JSONObject doubled = holdStatus(400);
        check("double tap", count(doubled, "double", "doubleTap") == 1, eventsOf(doubled));
        check("double no single", count(doubled, "tap", "tap") == 0, eventsOf(doubled));
        touch.begin(0, x, y);
        touch.up(0);
        awaitCount("tap", "tap", 1, 1200);
        JSONObject single = holdStatus(200);
        check("delayed single", count(single, "tap", "tap") == 1, eventsOf(single));
        check("delayed keeps double", count(single, "double", "doubleTap") == 1, eventsOf(single));
    }

    private void panScene(JSONObject status) {
        Rect pad = rect(status, "pad");
        float x = pad.cx();
        float y = pad.cy();
        touch.begin(0, space.x(x), space.y(y));
        touch.move(0, space.x(x + 36f), space.y(y));
        touch.move(0, space.x(x + 68f), space.y(y));
        touch.up(0);
        awaitCount("pan", "end", 1, 1500);
        JSONObject done = holdStatus(250);
        int start = firstIndex(done, "pan", "start");
        int update = firstIndex(done, "pan", "update");
        int end = firstIndex(done, "pan", "end");
        check("pan ordered", start >= 0 && update > start && end > update, eventsOf(done));
        check("pan one terminal",
                count(done, "pan", "end") == 1 && count(done, "pan", "cancel") == 0,
                eventsOf(done));
        check("pan no tap", count(done, "tap", "tap") == 0, eventsOf(done));
    }

    private void transformScene(JSONObject status) {
        Rect pad = rect(status, "pad");
        if (pad.w < 260f || pad.h < 160f) {
            check("transform pad", false, pad.w + "x" + pad.h);
            return;
        }
        float cx = pad.cx();
        float cy = pad.cy();
        touch.begin(0, space.x(cx - 16f), space.y(cy));
        touch.move(0, space.x(cx + 16f), space.y(cy));
        touch.down(1, space.x(cx + 64f), space.y(cy));
        touch.moveTwo(0, space.x(cx + 4f), space.y(cy - 20f), 1, space.x(cx + 92f), space.y(cy + 16f));
        touch.moveTwo(0, space.x(cx - 8f), space.y(cy - 36f), 1, space.x(cx + 112f), space.y(cy + 28f));
        awaitCount("transform", "start", 1, 1500);
        JSONObject posed = holdStatus(150);
        JSONObject update = last(posed, "transform", "update");
        boolean geometry = false;
        if (update != null) {
            double tx = update.optDouble("translationX");
            double ty = update.optDouble("translationY");
            double scale = update.optDouble("scale");
            double rotation = update.optDouble("rotation");
            geometry = finite(tx) && finite(ty) && finite(scale) && finite(rotation)
                    && Math.hypot(tx, ty) >= 10.0
                    && Math.abs(scale - 1.0) >= 0.05
                    && Math.abs(rotation) >= 0.08;
        }
        check("transform geometry", geometry, update == null ? eventsOf(posed) : update.toString());
        check("transform one start", count(posed, "transform", "start") == 1, eventsOf(posed));
        check("transform pan cancel",
                count(posed, "pan", "start") == 1
                        && count(posed, "pan", "cancel") == 1
                        && count(posed, "pan", "end") == 0,
                eventsOf(posed));
        JSONObject before = update;
        touch.down(2, space.x(cx), space.y(cy + 30f));
        touch.move(2, space.x(cx - 24f), space.y(cy + 58f));
        touch.up(2);
        JSONObject ignored = holdStatus(250);
        JSONObject after = last(ignored, "transform", "update");
        boolean same = before != null && after != null
                && Math.abs(before.optDouble("translationX") - after.optDouble("translationX")) < 0.75
                && Math.abs(before.optDouble("translationY") - after.optDouble("translationY")) < 0.75
                && Math.abs(before.optDouble("scale") - after.optDouble("scale")) < 0.02
                && Math.abs(before.optDouble("rotation") - after.optDouble("rotation")) < 0.02
                && count(ignored, "transform", "start") == 1
                && count(ignored, "transform", "end") == 0;
        check("third pointer ignored", same, eventsOf(ignored));
        touch.up(0);
        awaitCount("transform", "end", 1, 1200);
        JSONObject ended = holdStatus(300);
        check("staggered first terminal",
                count(ended, "transform", "end") == 1 && count(ended, "transform", "cancel") == 0,
                eventsOf(ended));
        touch.up(1);
        JSONObject rest = holdStatus(300);
        check("staggered up one terminal",
                count(rest, "transform", "end") == 1 && count(rest, "transform", "cancel") == 0,
                eventsOf(rest));
        check("transform no tap", count(rest, "tap", "tap") == 0, eventsOf(rest));
    }

    private void parallelScene(JSONObject status) {
        Rect outer = rect(status, "outer");
        float x = outer.cx();
        float y = outer.cy();
        touch.begin(0, space.x(x), space.y(y));
        touch.move(0, space.x(x + 48f), space.y(y - 48f));
        touch.up(0);
        awaitCount("pan", "end", 1, 1500);
        JSONObject done = holdStatus(200);
        JSONObject end = last(done, "pan", "end");
        double tx = end == null ? 0.0 : end.optDouble("translationX");
        float scroll = scrollY(outer.handle);
        check("parallel pan", tx > 10.0, eventsOf(done));
        check("parallel scroll", scroll > 10f, Float.toString(scroll));
        check("parallel one terminal",
                count(done, "pan", "end") == 1 && count(done, "pan", "cancel") == 0,
                eventsOf(done));
    }

    private void reserveScene(JSONObject status) {
        Rect outer = rect(status, "outer");
        if (outer.h < 140f) {
            check("reserve room", false, Float.toString(outer.h));
            return;
        }
        float x = outer.cx();
        float y = outer.y + outer.h - 20f;
        touch.begin(0, space.x(x), space.y(y));
        touch.move(0, space.x(x), space.y(y - 30f));
        float pending = holdScroll(outer.handle, 120);
        check("reserve pending offset", Math.abs(pending) <= 1f, Float.toString(pending));
        touch.move(0, space.x(x), space.y(y - 90f));
        float replay = holdScroll(outer.handle, 120);
        check("reserve replay 90", Math.abs(replay - 90f) <= 8f, Float.toString(replay));
        touch.move(0, space.x(x), space.y(y - 110f));
        float total = holdScroll(outer.handle, 120);
        check("reserve total 110", Math.abs(total - 110f) <= 8f, Float.toString(total));
        touch.up(0);
    }

    private void containScene(JSONObject status) {
        Rect inner = rect(status, "inner");
        Rect outer = rect(status, "outer");
        float x = inner.cx();
        float y = inner.y + inner.h - 12f;
        touch.begin(0, space.x(x), space.y(y));
        touch.move(0, space.x(x), space.y(y - 90f));
        SystemClock.sleep(180);
        float innerY = scrollY(inner.handle);
        float outerY = scrollY(outer.handle);
        touch.up(0);
        check("contain inner", innerY >= 55f && innerY <= 60.5f, Float.toString(innerY));
        check("contain outer", Math.abs(outerY) <= 1f, Float.toString(outerY));
    }

    private void disposeScene(JSONObject status) {
        Rect pad = rect(status, "pad");
        float x = pad.cx();
        float y = pad.cy();
        touch.begin(0, space.x(x), space.y(y));
        touch.move(0, space.x(x + 36f), space.y(y));
        touch.move(0, space.x(x + 72f), space.y(y));
        awaitCount("pan", "cancel", 1, 1500);
        touch.move(0, space.x(x + 108f), space.y(y));
        JSONObject cancelled = holdStatus(220);
        int start = firstIndex(cancelled, "pan", "start");
        int update = firstIndex(cancelled, "pan", "update");
        int cancel = firstIndex(cancelled, "pan", "cancel");
        check("dispose cancel",
                start >= 0 && update > start && cancel > update
                        && count(cancelled, "pan", "cancel") == 1
                        && count(cancelled, "pan", "update") == 1,
                eventsOf(cancelled));
        touch.up(0);
        JSONObject lifted = holdStatus(220);
        check("dispose no end",
                count(lifted, "pan", "end") == 0
                        && count(lifted, "pan", "cancel") == 1
                        && count(lifted, "pan", "update") == 1,
                eventsOf(lifted));
        int frozen = countName(lifted, "pan");
        touch.begin(0, space.x(x), space.y(y));
        touch.move(0, space.x(x + 40f), space.y(y));
        touch.up(0);
        JSONObject later = holdStatus(250);
        check("dispose later touch", countName(later, "pan") == frozen, eventsOf(later));
    }

    private void lifecycleScene(JSONObject status) {
        Activity first = activity;
        Rect pad = rect(status, "pad");
        touch.begin(0, space.x(pad.cx()), space.y(pad.cy()));
        touch.move(0, space.x(pad.cx() + 40f), space.y(pad.cy()));
        JSONObject started = awaitCount("pan", "start", 1, 1200);
        check("lifecycle pan", count(started, "pan", "start") == 1, eventsOf(started));
        boolean backgrounded = goHome();
        check("lifecycle background", backgrounded, "focus " + windowFocused());
        touch.abandon();
        resumeApp();
        boolean foreground = awaitFocus(true, 2000);
        awaitCount("pan", "cancel", 1, 1500);
        JSONObject done = holdStatus(350);
        int reason = reasonOf(done, "pan", "cancel");
        check("lifecycle cancel",
                foreground && count(done, "pan", "cancel") == 1 && reason == 7,
                eventsOf(done) + " reason " + reason);
        check("lifecycle no end", count(done, "pan", "end") == 0, eventsOf(done));
        check("lifecycle instance",
                first != null && !first.isDestroyed() && done != null && "lifecycle".equals(done.optString("scene")),
                first == null ? "null" : "destroyed " + first.isDestroyed());
    }

    private boolean goHome() {
        long now = SystemClock.uptimeMillis();
        KeyEvent down = new KeyEvent(now, now, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_HOME, 0);
        KeyEvent up = new KeyEvent(now, now + 30, KeyEvent.ACTION_UP, KeyEvent.KEYCODE_HOME, 0);
        down.setSource(InputDevice.SOURCE_KEYBOARD);
        up.setSource(InputDevice.SOURCE_KEYBOARD);
        ui.injectInputEvent(down, true);
        ui.injectInputEvent(up, true);
        if (awaitFocus(false, 1500)) return true;
        ui.performGlobalAction(AccessibilityService.GLOBAL_ACTION_HOME);
        return awaitFocus(false, 1500);
    }

    private boolean awaitFocus(boolean want, long timeout) {
        long deadline = SystemClock.uptimeMillis() + timeout;
        while (SystemClock.uptimeMillis() < deadline) {
            if (windowFocused() == want) return true;
            SystemClock.sleep(40);
        }
        return windowFocused() == want;
    }

    private boolean windowFocused() {
        boolean[] focused = new boolean[] {false};
        try {
            runOnMainSync(() -> focused[0] = activity != null && activity.hasWindowFocus());
        } catch (Throwable ignored) {
            return false;
        }
        return focused[0];
    }

    private void faultsCheck(String name) {
        int[] faults = new int[] {-1};
        runOnMainSync(() -> faults[0] = BubbleNative.INSTANCE.faults());
        check(name, faults[0] == 0, Integer.toString(faults[0]));
    }

    private String nodeText() {
        String[] holder = new String[] {""};
        try {
            runOnMainSync(() -> {
                try {
                    String text = BubbleNative.INSTANCE.nodeText(3);
                    holder[0] = text == null ? "" : text;
                } catch (Throwable error) {
                    holder[0] = "";
                }
            });
        } catch (Throwable ignored) {
            return "";
        }
        return holder[0];
    }

    private float scrollY(int handle) {
        float[] holder = new float[] {0f};
        runOnMainSync(() -> holder[0] = BubbleNative.INSTANCE.scrollYOf(handle));
        return holder[0];
    }

    private float holdScroll(int handle, long ms) {
        SystemClock.sleep(ms);
        return scrollY(handle);
    }

    private Rect rect(JSONObject status, String name) {
        Rect rect = new Rect();
        JSONObject nodes = status == null ? null : status.optJSONObject("nodes");
        rect.handle = nodes == null ? 0 : nodes.optInt(name, 0);
        int handle = rect.handle;
        float[][] holder = new float[1][];
        runOnMainSync(() -> holder[0] = BubbleNative.INSTANCE.nodeBox(handle));
        float[] box = holder[0];
        if (box != null && box.length >= 4) {
            rect.x = box[0];
            rect.y = box[1];
            rect.w = box[2];
            rect.h = box[3];
        }
        return rect;
    }

    private JSONObject awaitCount(String name, String type, int expected, long timeout) {
        long deadline = SystemClock.uptimeMillis() + timeout;
        JSONObject last = null;
        while (SystemClock.uptimeMillis() < deadline) {
            last = parse(nodeText());
            if (last != null && count(last, name, type) >= expected) return last;
            SystemClock.sleep(32);
        }
        return last;
    }

    private JSONObject holdStatus(long ms) {
        SystemClock.sleep(ms);
        return parse(nodeText());
    }

    private static JSONObject parse(String text) {
        if (text == null || text.length() == 0) return null;
        try {
            return new JSONObject(text);
        } catch (JSONException error) {
            return null;
        }
    }

    private static int padOf(JSONObject json) {
        JSONObject nodes = json == null ? null : json.optJSONObject("nodes");
        return nodes == null ? 0 : nodes.optInt("pad", 0);
    }

    private static int countEvents(JSONObject status) {
        JSONArray events = status == null ? null : status.optJSONArray("events");
        return events == null ? -1 : events.length();
    }

    private static int count(JSONObject status, String name, String type) {
        JSONArray events = status == null ? null : status.optJSONArray("events");
        if (events == null) return -1;
        int found = 0;
        for (int i = 0; i < events.length(); i++) {
            JSONObject event = events.optJSONObject(i);
            if (event != null && name.equals(event.optString("name")) && type.equals(event.optString("type"))) {
                found += 1;
            }
        }
        return found;
    }

    private static int countName(JSONObject status, String name) {
        JSONArray events = status == null ? null : status.optJSONArray("events");
        if (events == null) return -1;
        int found = 0;
        for (int i = 0; i < events.length(); i++) {
            JSONObject event = events.optJSONObject(i);
            if (event != null && name.equals(event.optString("name"))) found += 1;
        }
        return found;
    }

    private static int firstIndex(JSONObject status, String name, String type) {
        JSONArray events = status == null ? null : status.optJSONArray("events");
        if (events == null) return -1;
        for (int i = 0; i < events.length(); i++) {
            JSONObject event = events.optJSONObject(i);
            if (event != null && name.equals(event.optString("name")) && type.equals(event.optString("type"))) {
                return i;
            }
        }
        return -1;
    }

    private static JSONObject last(JSONObject status, String name, String type) {
        JSONArray events = status == null ? null : status.optJSONArray("events");
        if (events == null) return null;
        JSONObject found = null;
        for (int i = 0; i < events.length(); i++) {
            JSONObject event = events.optJSONObject(i);
            if (event != null && name.equals(event.optString("name")) && type.equals(event.optString("type"))) {
                found = event;
            }
        }
        return found;
    }

    private static int reasonOf(JSONObject status, String name, String type) {
        JSONObject event = last(status, name, type);
        return event == null ? -1 : event.optInt("reason", -1);
    }

    private static String eventsOf(JSONObject status) {
        if (status == null) return "unparsed";
        JSONArray events = status.optJSONArray("events");
        if (events == null) return clip(status.toString());
        StringBuilder builder = new StringBuilder();
        for (int i = 0; i < events.length(); i++) {
            JSONObject event = events.optJSONObject(i);
            if (event == null) continue;
            if (builder.length() > 0) builder.append(',');
            builder.append(event.optString("name")).append(':').append(event.optString("type"));
        }
        return builder.length() == 0 ? "[]" : builder.toString();
    }

    private static boolean finite(double value) {
        return !Double.isNaN(value) && !Double.isInfinite(value);
    }

    private static String clip(String text) {
        if (text == null) return "";
        return text.length() <= 420 ? text : text.substring(0, 420);
    }

    private void check(String name, boolean ok, String detail) {
        JSONObject item = new JSONObject();
        try {
            item.put("name", name);
            item.put("ok", ok);
            item.put("detail", detail == null ? "" : detail);
            checks.put(item);
        } catch (JSONException ignored) {
        }
    }

    private void deleteResult() {
        File file = new File(getTargetContext().getFilesDir(), "gestures-result.json");
        if (file.exists() && !file.delete()) {
            check("clear previous result", false, file.getAbsolutePath());
        }
    }

    private String report() throws JSONException {
        passed = checks.length() > 0;
        for (int i = 0; i < checks.length(); i++) {
            JSONObject item = checks.optJSONObject(i);
            if (item == null || !item.optBoolean("ok")) passed = false;
        }
        JSONObject root = new JSONObject();
        root.put("suite", "gestures.android");
        root.put("ok", passed);
        root.put("elapsed_ms", SystemClock.elapsedRealtime() - started);
        root.put("checks", checks);
        return root.toString();
    }

    private void writeResult(String json) {
        File file = new File(getTargetContext().getFilesDir(), "gestures-result.json");
        FileOutputStream out = null;
        try {
            out = new FileOutputStream(file);
            out.write(json.getBytes(StandardCharsets.UTF_8));
            out.flush();
        } catch (IOException error) {
            passed = false;
            Log.e("BUBBLE_GESTURES", "write result " + error);
        } finally {
            if (out != null) {
                try {
                    out.close();
                } catch (IOException ignored) {
                }
            }
        }
    }

    private interface Scene {
        void run(JSONObject status);
    }

    private static final class Rect {
        int handle;
        float x;
        float y;
        float w;
        float h;

        float cx() {
            return x + w / 2f;
        }

        float cy() {
            return y + h / 2f;
        }
    }

    private final class Space {
        int locX;
        int locY;
        int padL;
        int padT;
        float density = 1f;

        void read() {
            runOnMainSync(() -> {
                View content = activity.findViewById(android.R.id.content);
                View shell = content;
                if (content instanceof ViewGroup && ((ViewGroup) content).getChildCount() > 0) {
                    shell = ((ViewGroup) content).getChildAt(0);
                }
                int[] location = new int[2];
                shell.getLocationOnScreen(location);
                locX = location[0];
                locY = location[1];
                padL = shell.getPaddingLeft();
                padT = shell.getPaddingTop();
                density = shell.getResources().getDisplayMetrics().density;
                if (density < 1f) density = 1f;
            });
        }

        float x(float logical) {
            return locX + padL + logical * density;
        }

        float y(float logical) {
            return locY + padT + logical * density;
        }
    }

    private final class Touch {
        private final ArrayList<Finger> fingers = new ArrayList<Finger>();
        private long downTime;
        private long eventTime;
        private boolean open;
        private String failure;

        void begin(int id, float x, float y) {
            if (open) cancel();
            fingers.clear();
            fingers.add(new Finger(id, x, y));
            open = true;
            for (int attempt = 0; attempt < 3; attempt++) {
                downTime = SystemClock.uptimeMillis();
                eventTime = downTime;
                if (inject(MotionEvent.ACTION_DOWN)) {
                    failure = null;
                    return;
                }
                SystemClock.sleep(50);
                awaitFocus(true, 1000);
            }
        }

        void down(int id, float x, float y) {
            if (!open) {
                begin(id, x, y);
                return;
            }
            stamp();
            fingers.add(new Finger(id, x, y));
            inject(MotionEvent.ACTION_POINTER_DOWN | ((fingers.size() - 1) << MotionEvent.ACTION_POINTER_INDEX_SHIFT));
        }

        void move(int id, float x, float y) {
            Finger finger = find(id);
            if (finger == null) return;
            stamp();
            finger.x = x;
            finger.y = y;
            inject(MotionEvent.ACTION_MOVE);
        }

        void moveTwo(int a, float ax, float ay, int b, float bx, float by) {
            Finger first = find(a);
            Finger second = find(b);
            if (first == null || second == null) return;
            stamp();
            first.x = ax;
            first.y = ay;
            second.x = bx;
            second.y = by;
            inject(MotionEvent.ACTION_MOVE);
        }

        void up(int id) {
            if (!open) return;
            int index = indexOf(id);
            if (index < 0) return;
            stamp();
            if (fingers.size() == 1) {
                inject(MotionEvent.ACTION_UP);
                fingers.clear();
                open = false;
            } else {
                inject(MotionEvent.ACTION_POINTER_UP | (index << MotionEvent.ACTION_POINTER_INDEX_SHIFT));
                fingers.remove(index);
            }
        }

        void cancel() {
            if (!open) return;
            stamp();
            inject(MotionEvent.ACTION_CANCEL);
            fingers.clear();
            open = false;
        }

        void abandon() {
            fingers.clear();
            open = false;
        }

        String takeFailure() {
            String failed = failure;
            failure = null;
            return failed;
        }

        private void stamp() {
            SystemClock.sleep(16);
            long now = SystemClock.uptimeMillis();
            if (now <= eventTime) now = eventTime + 1;
            eventTime = now;
        }

        private boolean inject(int action) {
            int count = fingers.size();
            if (count == 0 || ui == null) {
                failure = MotionEvent.actionToString(action) + " without pointers";
                return false;
            }
            boolean lifting = action == MotionEvent.ACTION_UP
                    || (action & MotionEvent.ACTION_MASK) == MotionEvent.ACTION_POINTER_UP
                    || action == MotionEvent.ACTION_CANCEL;
            MotionEvent.PointerProperties[] props = new MotionEvent.PointerProperties[count];
            MotionEvent.PointerCoords[] coords = new MotionEvent.PointerCoords[count];
            for (int i = 0; i < count; i++) {
                Finger finger = fingers.get(i);
                MotionEvent.PointerProperties prop = new MotionEvent.PointerProperties();
                prop.id = finger.id;
                prop.toolType = MotionEvent.TOOL_TYPE_FINGER;
                props[i] = prop;
                MotionEvent.PointerCoords coord = new MotionEvent.PointerCoords();
                coord.x = finger.x;
                coord.y = finger.y;
                coord.pressure = lifting ? 0f : 1f;
                coord.size = 1f;
                coords[i] = coord;
            }
            MotionEvent event = MotionEvent.obtain(
                    downTime,
                    eventTime,
                    action,
                    count,
                    props,
                    coords,
                    0,
                    0,
                    1f,
                    1f,
                    0,
                    0,
                    InputDevice.SOURCE_TOUCHSCREEN,
                    0);
            boolean accepted = true;
            try {
                if (!ui.injectInputEvent(event, true)) {
                    accepted = false;
                    failure = MotionEvent.actionToString(action);
                }
            } catch (Throwable error) {
                accepted = false;
                failure = MotionEvent.actionToString(action) + ": " + error;
            } finally {
                event.recycle();
            }
            return accepted;
        }

        private Finger find(int id) {
            int index = indexOf(id);
            return index < 0 ? null : fingers.get(index);
        }

        private int indexOf(int id) {
            for (int i = 0; i < fingers.size(); i++) {
                if (fingers.get(i).id == id) return i;
            }
            return -1;
        }
    }

    private static final class Finger {
        final int id;
        float x;
        float y;

        Finger(int id, float x, float y) {
            this.id = id;
            this.x = x;
            this.y = y;
        }
    }
}
