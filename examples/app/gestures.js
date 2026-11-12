import { signal, batch, flush, mount, view, text, forEach, show, tapGesture, doubleTapGesture, longPressGesture, panGesture, transformGesture } from "../runtime.js";

// A dedicated device fixture: handles come from the renderer, not the demo's
// incidental node order. Only the root status Text has a fixed handle (3).
let selected;
let status;
let sceneName = "tap";
let nodes = {};
let events = [];
let ready = false;

function publish() {
  status.update(() => JSON.stringify({ scene: sceneName, nodes, events, ready }));
}

function record(name, event) {
  events.push({ name, type: event.type, time: event.time,
    translationX: event.translationX, translationY: event.translationY,
    scale: event.scale, rotation: event.rotation, reason: event.reason });
  publish();
}

function tap() {
  tapGesture({ onTapStart: e => record("tap", e), onTapEnd: e => record("tap", e),
    onTapCancel: e => record("tap", e), onTap: e => record("tap", e) });
}

function pan(extra = {}) {
  panGesture({ axis: "both", minDistance: 10,
    onPanStart: e => record("pan", e), onPanUpdate: e => record("pan", e),
    onPanEnd: e => record("pan", e), onPanCancel: e => record("pan", e), ...extra });
}

function build(scene) {
  if (scene === "parallel" || scene === "reserve") {
    nodes.outer = view({ height: 300, flexShrink: 0, overflowY: "scroll", backgroundColor: "#26364a" }, () => {
      nodes.pad = view({ height: 800, flexShrink: 0 }, () => {
        if (scene === "parallel") pan({ axis: "x" });
        else {
          longPressGesture({ duration: 2000, maxDistance: 60, captureAxis: "y",
            onLongPress: e => record("long", e), onLongPressEnd: e => record("long", e),
            onLongPressCancel: e => record("long", e) });
          tap();
        }
        text({ color: "#ffffff", fontSize: 20 }, scene);
      });
    });
  } else if (scene === "contain") {
    nodes.outer = view({ height: 300, flexShrink: 0, overflowY: "scroll", backgroundColor: "#26364a" }, () => {
      nodes.inner = view({ height: 120, flexShrink: 0, overflowY: "scroll", overscrollY: false, backgroundColor: "#526b83" }, () => {
        view({ height: 180, flexShrink: 0 }, () => text({ color: "#ffffff" }, "contained scroll"));
      });
      view({ height: 600, flexShrink: 0 }, () => text({ color: "#ffffff" }, "outer scroll"));
    });
    nodes.pad = nodes.inner;
  } else {
    nodes.pad = view({ flexGrow: 1, flexShrink: 1, backgroundColor: "#26364a" }, () => {
      if (scene === "long") {
        longPressGesture({ duration: 500, maxDistance: 10,
          onLongPress: e => record("long", e), onLongPressEnd: e => record("long", e),
          onLongPressCancel: e => record("long", e) });
      }
      if (scene === "double") {
        doubleTapGesture({ onDoubleTap: e => record("double", e) });
      }
      if (scene === "pan" || scene === "lifecycle" || scene === "transform") pan();
      if (scene === "transform") {
        transformGesture({ mode: ["pan", "pinch", "rotate"], minDistance: 10,
          onTransformStart: e => record("transform", e), onTransformUpdate: e => record("transform", e),
          onTransformEnd: e => record("transform", e), onTransformCancel: e => record("transform", e) });
      }
      if (scene === "dispose") {
        const visible = signal(true);
        show(() => visible(), null, () => pan({ onPanUpdate: e => {
          record("pan", e);
          visible.update(() => false);
        } }));
      }
      tap();
      text({ color: "#ffffff", fontSize: 24 }, scene);
    });
  }
  ready = true;
  publish();
}
globalThis.__gestureSelect = scene => {
  batch(() => {
    if (!["tap", "long", "double", "pan", "transform", "parallel", "reserve", "contain", "dispose", "lifecycle"].includes(scene)) {
      throw new Error("unknown gesture scene " + scene);
    }
    nodes = {};
    events = [];
    ready = false;
    sceneName = scene;
    selected.update(version => version + 1);
  });
  flush();
};

mount(() => {
  selected = signal(0);
  status = signal("");
  return view({ flexGrow: 1, flexShrink: 1, backgroundColor: "#18212d" }, () => {
    text({ height: 60, flexShrink: 0, fontSize: 8, color: "#ffffff" }, () => status());
    forEach(() => [selected()], version => version, () => build(sceneName));
  });
});
