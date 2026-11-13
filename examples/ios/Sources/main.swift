import CoreGraphics
import CoreText
import UIKit


func nodeText(_ handle: UInt32) -> String {
    var buf = [CChar](repeating: 0, count: 256)
    guard bubble_node_text(handle, &buf, UInt32(buf.count)) == 0 else { return "" }
    return String(cString: buf)
}

func nodeTextLarge(_ handle: UInt32) -> String {
    var cap = 64 * 1024
    let limit = 1024 * 1024
    while cap <= limit {
        var buf = [CChar](repeating: 0, count: cap)
        if bubble_node_text(handle, &buf, UInt32(cap)) != 0 { return "" }
        let end = buf.firstIndex(of: 0) ?? buf.count
        if end < cap - 1 || cap == limit {
            let bytes = buf[..<end].map { UInt8(bitPattern: $0) }
            return String(decoding: bytes, as: UTF8.self)
        }
        cap *= 2
    }
    return ""
}

func jsonDouble(_ value: Any?) -> Double {
    (value as? NSNumber)?.doubleValue ?? .nan
}

func gestureReasonName(_ value: Any?) -> String {
    let number = jsonDouble(value)
    guard number.isFinite else { return "?" }
    switch Int(number) {
    case 0: return "NONE"
    case 1: return "MOVED"
    case 2: return "SCROLLED"
    case 3: return "SECOND_POINTER"
    case 4: return "UPGRADE"
    case 5: return "DISPOSED"
    case 6: return "TARGET_DISPOSED"
    case 7: return "HOST_CANCEL"
    default: return "\(Int(number))"
    }
}

func finiteFloat(_ value: Float) -> Double {
    let number = Double(value)
    return number.isFinite ? number : 0
}

struct GestureStatus {
    var scene = ""
    var ready = false
    var events: [[String: Any]] = []
    var nodes: [String: UInt32] = [:]
}

func parseGestureStatus(_ text: String) -> GestureStatus? {
    guard let data = text.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
    var nodes: [String: UInt32] = [:]
    if let raw = obj["nodes"] as? [String: Any] {
        for (name, value) in raw {
            if let number = value as? NSNumber {
                nodes[name] = number.uint32Value
            }
        }
    }
    let events = (obj["events"] as? [Any])?.compactMap { $0 as? [String: Any] } ?? []
    return GestureStatus(
        scene: obj["scene"] as? String ?? "",
        ready: obj["ready"] as? Bool ?? false,
        events: events,
        nodes: nodes
    )
}

func gestureEventList(_ snap: [String: Any]) -> [[String: Any]] {
    (snap["events"] as? [Any])?.compactMap { $0 as? [String: Any] } ?? []
}

func gestureNodes(_ snap: [String: Any]) -> [String: Any] {
    snap["nodes"] as? [String: Any] ?? [:]
}

func gestureNode(_ snap: [String: Any], _ name: String) -> [String: Any]? {
    gestureNodes(snap)[name] as? [String: Any]
}

final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        true
    }
}

@objc(SceneDelegate)
final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?
    let driver = AppDriver()

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
        guard let scene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: scene)
        let controller = UIViewController()
        driver.view.frame = window.bounds
        driver.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        controller.view.addSubview(driver.view)
        window.rootViewController = controller
        window.makeKeyAndVisible()
        self.window = window
        driver.start()
    }

    func sceneDidBecomeActive(_ scene: UIScene) {
        driver.setVisible(true)
    }

    func sceneWillResignActive(_ scene: UIScene) {
        driver.setVisible(false)
    }
}


final class AppDriver {
    static let scenes = ["tap", "long", "double", "pan", "transform", "parallel", "reserve", "contain", "dispose", "lifecycle"]

    let view = BubbleView()
    var frames = 0
    var frameMs = 0.0
    var last = CACurrentMediaTime()
    var phase = 0
    var hangFrames = 0
    let started = CACurrentMediaTime()
    var link: CADisplayLink?
    var engineReady = false
    var devURL: URL?
    var devStamp = ""
    var devBusy = false
    var lastDev = 0.0
    var appVisible = true
    var hides = 0
    var selectState = 0
    var padBeforeSelect: UInt32 = 0
    var sawSelectGap = false
    var priorChecks: [[String: Any]] = []
    var priorElapsed = 0
    var lastSignature = ""
    var editRan = false

    var editResultURL: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("edit-result.json")
    }

    var gestureMode: Bool { ProcessInfo.processInfo.environment["BUBBLE_GESTURES"] == "1" }
    var editMode: Bool { ProcessInfo.processInfo.environment["BUBBLE_EDIT"] == "1" }
    var m1Mode: Bool { ProcessInfo.processInfo.environment["BUBBLE_EVAL"] == "1" }
    var fixtureName: String {
        let requested = ProcessInfo.processInfo.environment["BUBBLE_FIXTURE"]
            ?? (editMode ? "editing" : (gestureMode ? "gestures" : "counter"))
        return requested.range(of: "^[a-z0-9-]+$", options: .regularExpression) == nil
            ? "counter"
            : requested
    }
    var gestureReset: Bool { ProcessInfo.processInfo.environment["BUBBLE_GESTURE_RESET"] == "1" }
    var gestureScene: String {
        let requested = ProcessInfo.processInfo.environment["BUBBLE_GESTURE_SCENE"] ?? "tap"
        return Self.scenes.contains(requested) ? requested : ""
    }
    var resultURL: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("gestures-result.json")
    }

    func setVisible(_ next: Bool) {
        guard next != appVisible else { return }
        appVisible = next
        if !next {
            hides += 1
            view.dropTrackedTouches()
        }
        guard engineReady else { return }
        bubble_set_visible(next ? 1 : 0)
    }

    func start() {
        var host = BubbleHost(measure: measure, ctx: nil, layout: layoutText)
        guard bubble_create(&host) == 0 else { return }
        engineReady = true
        if let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
            try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
            _ = base.path.withCString { bubble_set_storage_dir($0) }
        }
        bubble_set_visible(appVisible ? 1 : 0)
        if let fontURL = Bundle.main.url(forResource: "HostFont", withExtension: "ttf"),
           let data = try? Data(contentsOf: fontURL) {
            data.withUnsafeBytes { raw in
                if let base = raw.bindMemory(to: UInt8.self).baseAddress {
                    bubble_register_font("HostFont", base, UInt32(data.count))
                }
            }
            if let provider = CGDataProvider(data: data as CFData), let font = CGFont(provider) {
                CTFontManagerRegisterGraphicsFont(font, nil)
                if let name = font.postScriptName as String? {
                    hostFamily = name
                    view.family = name
                }
            }
        }
        if let raw = ProcessInfo.processInfo.environment["BUBBLE_DEV"],
           let url = URL(string: raw) {
            devURL = url
        }
        if devURL == nil {
            guard let manifestURL = Bundle.main.url(
                      forResource: fixtureName,
                      withExtension: "json",
                      subdirectory: "fixtures"
                  ),
                  let entryURL = Bundle.main.url(
                      forResource: fixtureName,
                      withExtension: "js",
                      subdirectory: "fixtures"
                  ),
                  let manifest = try? String(contentsOf: manifestURL, encoding: .utf8),
                  let entry = try? Data(contentsOf: entryURL) else { return }
            loadBundle(manifest, entry)
        }
        NSLog("bubble bounds %@", NSCoder.string(for: view.bounds))
        if gestureMode {
            view.configureGestureSurface(true)
            view.tracePointers = true
            loadPriorReport()
        }
        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func loadBundle(_ manifest: String, _ entry: Data) {
        let loaded: Int32 = manifest.withCString { manifest in
            entry.withUnsafeBytes { raw in
                bubble_load(manifest, raw.bindMemory(to: UInt8.self).baseAddress, UInt32(entry.count))
            }
        }
        if loaded != 0 { NSLog("bubble load failed") }
        devStamp = manifest + "\n" + (String(data: entry, encoding: .utf8) ?? "")
    }

    func pollDev() {
        guard let devURL, !devBusy else { return }
        let now = CACurrentMediaTime()
        if now - lastDev < 1 { return }
        lastDev = now
        devBusy = true
        URLSession.shared.dataTask(with: devURL) { data, _, _ in
            defer { DispatchQueue.main.async { self.devBusy = false } }
            guard let data,
                  let object = try? JSONSerialization.jsonObject(with: data) as? [String: String],
                  let manifest = object["manifest"],
                  let entry = object["entry"] else { return }
            let stamp = manifest + "\n" + entry
            if stamp == self.devStamp { return }
            let bytes = Data(entry.utf8)
            DispatchQueue.main.async {
                self.loadBundle(manifest, bytes)
            }
        }.resume()
    }

    @objc func tick() {
        pollDev()
        let now = CACurrentMediaTime()
        let dt = (now - last) * 1000
        last = now
        view.refresh()
        frames += 1
        if editMode {
            if !editRan && frames >= 8 {
                editRan = true
                runEditAcceptance()
            }
            return
        }
        if gestureMode {
            advanceGestures()
            return
        }
        if !m1Mode { return }
        if frames > 5 { frameMs += dt }
        if phase == 0 && frames == 20 {
            var x: Float = 0, y: Float = 0, w: Float = 0, h: Float = 0
            _ = bubble_node_box(4, &x, &y, &w, &h)
            let (px, py) = (x + w / 2, y + h / 2)
            _ = bubble_pointer(0, 1, px, py, CACurrentMediaTime() * 1000, 0, 1)
            _ = bubble_pointer(2, 1, px, py, CACurrentMediaTime() * 1000 + 20, 0, 0)
            phase = 1
        } else if phase == 1 && nodeText(3) == "Count: 1" {
            bubble_post_eval("while (true) {}")
            phase = 2
            hangFrames = 0
        } else if phase == 2 {
            if bubble_faults() == 0 { hangFrames += 1 }
            if bubble_faults() > 0 && nodeText(3) == "Count: 0" && bubble_scroll_y_of(6) >= 40 {
                finish()
                phase = 3
            }
        }
        if phase != 3 && CACurrentMediaTime() - started > 12 {
            finish()
            phase = 3
        }
    }

    func advanceGestures() {
        let parsed = parseGestureStatus(nodeTextLarge(3))
        armSelection(parsed)
        let snap = gestureSnapshot(parsed)
        writeGestureReport(gestureChecks(snap))
        if let data = try? JSONSerialization.data(withJSONObject: snap, options: [.sortedKeys]),
           let json = String(data: data, encoding: .utf8) {
            view.publishGestureSnapshot(json)
        }
    }

    func armSelection(_ parsed: GestureStatus?) {
        guard selectState < 2, let parsed else { return }
        let pad = parsed.nodes["pad"] ?? 0
        if selectState == 0 {
            guard parsed.ready, parsed.scene == "tap", pad != 0 else { return }
            padBeforeSelect = pad
            sawSelectGap = false
            guard !gestureScene.isEmpty else {
                selectState = 2
                return
            }
            postGestureSelect(gestureScene)
            selectState = 1
            return
        }
        if !parsed.ready || pad == 0 || !parsed.events.isEmpty {
            sawSelectGap = true
        }
        if parsed.ready && parsed.scene == gestureScene && parsed.events.isEmpty && pad != 0
            && (pad != padBeforeSelect || sawSelectGap) {
            selectState = 2
        }
    }

    func postGestureSelect(_ scene: String) {
        let source = "globalThis.__gestureSelect(\"\(scene)\")"
        source.withCString { bubble_post_eval($0) }
    }

    func gestureSnapshot(_ parsed: GestureStatus?) -> [String: Any] {
        var nodes: [String: Any] = [:]
        if let parsed {
            for (name, handle) in parsed.nodes {
                nodes[name] = nodeObservation(handle)
            }
        }
        return [
            "scene": parsed?.scene ?? "",
            "ready": parsed?.ready ?? false,
            "armed": selectState == 2,
            "requested": gestureScene,
            "visible": appVisible,
            "hides": hides,
            "faults": Int(bubble_faults()),
            "events": parsed?.events ?? [],
            "nodes": nodes,
        ]
    }

    func nodeObservation(_ handle: UInt32) -> [String: Any] {
        var x: Float = 0, y: Float = 0, w: Float = 0, h: Float = 0
        let code = bubble_node_box(handle, &x, &y, &w, &h)
        let inset = view.safeAreaInsets
        return [
            "handle": Int(handle),
            "x": code == 0 ? Double(x) + Double(inset.left) : 0,
            "y": code == 0 ? Double(y) + Double(inset.top) : 0,
            "w": code == 0 ? Double(w) : 0,
            "h": code == 0 ? Double(h) : 0,
            "scrollX": finiteFloat(bubble_scroll_x_of(handle)),
            "scrollY": finiteFloat(bubble_scroll_y_of(handle)),
        ]
    }

    func gestureChecks(_ snap: [String: Any]) -> [[String: Any]] {
        let scene = gestureScene.isEmpty ? "gesture" : gestureScene
        let armed = snap["armed"] as? Bool == true
        let events = gestureEventList(snap)
        let sceneReady = armed && (snap["ready"] as? Bool == true) && (snap["scene"] as? String) == gestureScene && !gestureScene.isEmpty
        func sized(_ name: String) -> Bool {
            guard let node = gestureNode(snap, name) else { return false }
            return jsonDouble(node["w"]) > 1 && jsonDouble(node["h"]) > 1
        }
        let needsOuter = scene == "parallel" || scene == "reserve" || scene == "contain"
        let primary = scene == "contain" ? "inner" : "pad"
        let boxes = sceneReady && sized(primary) && (!needsOuter || sized("outer"))
        var out: [[String: Any]] = []
        func add(_ suffix: String, _ ok: Bool, _ detail: String, gated: Bool = true) {
            out.append(["name": "\(scene).\(suffix)", "ok": gated ? (boxes && ok) : ok, "detail": detail])
        }
        func count(_ name: String, _ type: String) -> Int {
            events.filter { ($0["name"] as? String) == name && ($0["type"] as? String) == type }.count
        }
        func countType(_ type: String) -> Int {
            events.filter { ($0["type"] as? String) == type }.count
        }
        let summary = events.map { event in
            "\(event["name"] as? String ?? "?"):\(event["type"] as? String ?? "?"):\(gestureReasonName(event["reason"]))"
        }.joined(separator: ",")
        add("ready", boxes, "armed=\(armed) scene=\(snap["scene"] as? String ?? "") ready=\(snap["ready"] as? Bool ?? false)", gated: false)
        switch scene {
        case "tap":
            add("tap", count("tap", "tap") == 1, summary)
            add("press", count("tap", "pressStart") == 1 && count("tap", "pressEnd") == 1 && count("tap", "pressCancel") == 0, summary)
        case "long":
            add("hold", count("long", "start") == 1 && count("long", "end") == 1 && count("long", "cancel") == 0, summary)
            add("notap", countType("tap") == 0, summary)
        case "double":
            let doubles = events.filter { ($0["name"] as? String) == "double" && ($0["type"] as? String) == "doubleTap" }
            let taps = events.filter { ($0["name"] as? String) == "tap" && ($0["type"] as? String) == "tap" }
            let gap = doubles.count == 1 && taps.count == 1 && jsonDouble(taps[0]["time"]) >= jsonDouble(doubles[0]["time"]) + 300
            add("double", doubles.count == 1, summary)
            add("single", gap, summary)
        case "pan":
            let tx = events.filter { ($0["name"] as? String) == "pan" }.map { jsonDouble($0["translationX"]) }.max() ?? -.infinity
            let ordered = count("pan", "start") == 1 && count("pan", "update") >= 1 && count("pan", "end") == 1 && count("pan", "cancel") == 0
            add("sequence", ordered && tx > 10, summary)
            add("notap", countType("tap") == 0, summary)
        case "transform":
            let transforms = events.filter { ($0["name"] as? String) == "transform" }
            let finite = !transforms.isEmpty && transforms.allSatisfy { event in
                jsonDouble(event["translationX"]).isFinite && jsonDouble(event["translationY"]).isFinite
                    && jsonDouble(event["scale"]).isFinite && jsonDouble(event["rotation"]).isFinite
            }
            let closed = count("transform", "start") == 2 && count("transform", "end") == 2 && count("transform", "cancel") == 0
            let pinched = transforms.contains { abs(jsonDouble($0["scale"]) - 1) > 0.2 }
            let rotated = transforms.contains { abs(jsonDouble($0["rotation"])) > 0.25 }
            add("pinch", closed && pinched, summary)
            add("rotate", closed && rotated, summary)
            add("finite", finite, summary)
            add("notap", countType("tap") == 0, summary)
            add("pan", count("pan", "end") == 0 && count("pan", "cancel") == count("pan", "start"), summary)
        case "parallel":
            let tx = events.filter { ($0["name"] as? String) == "pan" }.map { jsonDouble($0["translationX"]) }.max() ?? -.infinity
            let ordered = count("pan", "start") == 1 && count("pan", "end") == 1 && count("pan", "cancel") == 0
            add("pan", ordered && tx > 10, summary)
            let scroll = jsonDouble(gestureNode(snap, "outer")?["scrollY"])
            add("scroll", scroll > 10, "scrollY=\(scroll)")
        case "reserve":
            let scroll = jsonDouble(gestureNode(snap, "outer")?["scrollY"])
            add("once", scroll > 100 && scroll < 180, "scrollY=\(scroll)")
            add("nolong", count("long", "start") == 0 && count("long", "end") == 0, summary)
            add("notap", countType("tap") == 0, summary)
        case "contain":
            let inner = jsonDouble(gestureNode(snap, "inner")?["scrollY"])
            let outer = jsonDouble(gestureNode(snap, "outer")?["scrollY"])
            add("inner", inner >= 58 && inner <= 60.5, "scrollY=\(inner)")
            add("outer", abs(outer) < 1, "scrollY=\(outer)")
        case "dispose":
            let types = events.filter { ($0["name"] as? String) == "pan" }.map { $0["type"] as? String ?? "" }
            let cancelAt = types.lastIndex(of: "cancel") ?? -1
            let trailing = types.enumerated().contains { index, type in
                cancelAt >= 0 && index > cancelAt && (type == "start" || type == "update" || type == "end")
            }
            let closed = count("pan", "start") == 1 && count("pan", "update") >= 1 && count("pan", "cancel") == 1 && count("pan", "end") == 0 && !trailing
            add("cancel", closed, summary)
        case "lifecycle":
            let visible = snap["visible"] as? Bool == true
            let hidden = jsonDouble(snap["hides"]) >= 1
            add("visibility", visible && hidden, "foreground/background only; hides=\(snap["hides"] ?? 0) visible=\(visible); not a held-touch cancel")
            add("idle", count("pan", "start") == 0 && count("pan", "cancel") == 0 && count("pan", "end") == 0 && countType("tap") == 0, "no gesture events; held-touch cancel was not run")
        default:
            add("scene", false, "missing or unknown BUBBLE_GESTURE_SCENE", gated: false)
        }
        return out
    }

    func loadPriorReport() {
        if gestureReset {
            try? FileManager.default.removeItem(at: resultURL)
            return
        }
        guard let data = try? Data(contentsOf: resultURL),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        priorElapsed = (obj["elapsed_ms"] as? NSNumber)?.intValue ?? 0
        let prefix = (gestureScene.isEmpty ? "gesture" : gestureScene) + "."
        if let checks = obj["checks"] as? [Any] {
            priorChecks = checks.compactMap { $0 as? [String: Any] }.filter { check in
                let name = check["name"] as? String ?? ""
                return !name.hasPrefix(prefix)
            }
        }
    }

    func writeGestureReport(_ checks: [[String: Any]]) {
        let elapsed = priorElapsed + Int((CACurrentMediaTime() - started) * 1000)
        let all = priorChecks + checks
        let ok = !all.isEmpty && all.allSatisfy { ($0["ok"] as? Bool) == true }
        let doc: [String: Any] = [
            "suite": "gestures.ios",
            "ok": ok,
            "elapsed_ms": elapsed,
            "checks": all,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: doc, options: [.sortedKeys]) else { return }
        try? FileManager.default.createDirectory(at: resultURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: resultURL, options: .atomic)
        let signature = all.map { "\($0["name"] as? String ?? "")=\(($0["ok"] as? Bool) == true)" }.joined(separator: ";")
        if signature != lastSignature, let text = String(data: data, encoding: .utf8) {
            lastSignature = signature
            NSLog("BUBBLE_GESTURES %@", text)
        }
    }

    func runEditAcceptance() {
        var checks: [[String: Any]] = []
        func add(_ name: String, _ ok: Bool, _ detail: String) {
            checks.append(["name": name, "ok": ok, "detail": detail])
        }
        func write() {
            let ok = !checks.isEmpty && checks.allSatisfy { ($0["ok"] as? Bool) == true }
            let doc: [String: Any] = [
                "suite": "edit.ios",
                "ok": ok,
                "elapsed_ms": Int((CACurrentMediaTime() - started) * 1000),
                "checks": checks,
            ]
            guard let data = try? JSONSerialization.data(withJSONObject: doc, options: [.sortedKeys]) else { return }
            try? FileManager.default.createDirectory(
                at: editResultURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try? data.write(to: editResultURL, options: .atomic)
            if let text = String(data: data, encoding: .utf8) {
                NSLog("BUBBLE_EDIT %@", text)
            }
        }
        func snapshot() -> (session: UInt64, revision: UInt64, anchor: Int, active: Int, compStart: Int, compEnd: Int, composing: Bool)? {
            var session: UInt64 = 0
            var revision: UInt64 = 0
            var anchor: UInt32 = 0
            var active: UInt32 = 0
            var compStart: UInt32 = 0
            var compEnd: UInt32 = 0
            var composing: UInt32 = 0
            guard bubble_edit_snapshot(
                &session, &revision, &anchor, &active, &compStart, &compEnd, &composing
            ) == 0 else { return nil }
            return (
                session,
                revision,
                Int(anchor),
                Int(active),
                Int(compStart),
                Int(compEnd),
                composing != 0
            )
        }
        func marked(_ range: UITextRange?) -> (Int, Int)? {
            guard let range = range as? IndexedRange else { return nil }
            return (range.startIndex, range.endIndex)
        }
        func selected() -> (Int, Int)? {
            marked(view.selectedTextRange)
        }
        func isRange(_ value: (Int, Int)?, _ expected: (Int, Int)) -> Bool {
            value?.0 == expected.0 && value?.1 == expected.1
        }

        var x: Float = 0, y: Float = 0, w: Float = 0, h: Float = 0
        guard bubble_node_box(4, &x, &y, &w, &h) == 0 else {
            add("field box", false, "bubble_node_box failed")
            write()
            return
        }
        let first = bubble_focus_at(x + w / 2, y + h / 2)
        add("focus", first == 4, "handle=\(first)")
        guard first == 4 else {
            write()
            return
        }

        view.insertText("a")
        view.insertText("b")
        add("value ab", nodeText(4) == "ab", nodeText(4))

        view.setMarkedText("に", selectedRange: NSRange(location: 1, length: 0))
        add("marked に", isRange(marked(view.markedTextRange), (2, 3)), String(describing: marked(view.markedTextRange)))
        add("caret in composition", isRange(selected(), (3, 3)), String(describing: selected()))
        add("composed に", nodeText(4) == "abに", nodeText(4))

        view.setMarkedText("日本", selectedRange: NSRange(location: 2, length: 0))
        add("marked 日本", isRange(marked(view.markedTextRange), (2, 4)), String(describing: marked(view.markedTextRange)))
        add("caret after compose", isRange(selected(), (4, 4)), String(describing: selected()))
        add("composed 日本", nodeText(4) == "ab日本", nodeText(4))

        view.selectedTextRange = IndexedRange(3, 4)
        add("inner composition selection", isRange(selected(), (3, 4)), String(describing: selected()))
        let inner = IndexedRange(3, 4)
        add("inner composition text", view.text(in: inner) == "本", String(describing: view.text(in: inner)))
        add("marked stays", isRange(marked(view.markedTextRange), (2, 4)), String(describing: marked(view.markedTextRange)))
        let caret = view.caretRect(for: IndexedPosition(3))
        add(
            "caret geometry",
            caret.height > 0 && caret.minX.isFinite && caret.minY.isFinite,
            "\(caret)"
        )
        let firstRect = view.firstRect(for: inner)
        add(
            "first rect geometry",
            firstRect.height > 0 && firstRect.minX.isFinite && firstRect.minY.isFinite,
            "\(firstRect)"
        )
        let rects = view.selectionRects(for: inner)
        add(
            "selection geometry",
            !rects.isEmpty && rects.contains { $0.rect.width > 0 && $0.rect.height > 0 },
            "\(rects.map(\.rect))"
        )

        view.unmarkText()
        add("unmark clears", view.markedTextRange == nil, String(describing: view.markedTextRange))
        add("unmark value", nodeText(4) == "ab日本", nodeText(4))
        add("unmark keeps selection", isRange(selected(), (3, 4)), String(describing: selected()))

        view.deleteBackward()
        add("delete selection", nodeText(4) == "ab日", nodeText(4))
        add("delete selection caret", isRange(selected(), (3, 3)), String(describing: selected()))
        view.insertText("😀")
        add("insert emoji", nodeText(4) == "ab日😀", nodeText(4))
        add("emoji caret", isRange(selected(), (5, 5)), String(describing: selected()))
        view.deleteBackward()
        add("delete emoji", nodeText(4) == "ab日", nodeText(4))
        add("emoji deleted caret", isRange(selected(), (3, 3)), String(describing: selected()))

        guard bubble_node_box(7, &x, &y, &w, &h) == 0 else {
            add("second field box", false, "bubble_node_box failed")
            write()
            return
        }
        let second = bubble_focus_at(x + w / 2, y + h / 2)
        add("focus second", second == 7, "handle=\(second)")
        add("second empty", nodeText(7) == "", nodeText(7))
        let emptyCaret = view.caretRect(for: IndexedPosition(0))
        add(
            "second caret geometry",
            emptyCaret.height > 0 && emptyCaret.minX.isFinite && emptyCaret.minY.isFinite,
            "\(emptyCaret)"
        )
        if let state = snapshot() {
            add(
                "second session",
                state.anchor == 0 && state.active == 0 && !state.composing,
                "anchor=\(state.anchor) active=\(state.active) composing=\(state.composing)"
            )
        } else {
            add("second session", false, "snapshot failed")
        }
        write()
    }

    func finish() {
        let samples = max(frames - 5, 1)
        let average = frameMs / Double(samples)
        let text = nodeText(3)
        let scrolled = bubble_scroll_y_of(6)
        let checks: [(String, Bool, String)] = [
            ("remount text", text == "Count: 0", text),
            ("faults", bubble_faults() > 0, "\(bubble_faults())"),
            ("frames during hang", hangFrames >= 10, "\(hangFrames)"),
            ("frame budget", average < 20, String(format: "%.2f", average)),
            ("font epoch", bubble_font_epoch() > 0, "\(bubble_font_epoch())"),
            ("scroll offset", scrolled >= 40, String(format: "%.1f", scrolled)),
            ("paint bytes", view.paint.count > 16, "\(view.paint.count)"),
        ]
        let ok = checks.allSatisfy { $0.1 }
        let body = checks.map { name, passed, detail in
            #"{"name":"\#(name)","ok":\#(passed),"detail":"\#(detail)"}"#
        }.joined(separator: ",")
        let elapsed = Int((CACurrentMediaTime() - started) * 1000)
        let json = #"{"suite":"m1.ios","ok":\#(ok),"elapsed_ms":\#(elapsed),"checks":[\#(body)]}"#
        let url = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("m1-result.json")
        try? json.data(using: .utf8)?.write(to: url)
        NSLog("BUBBLE_EVAL %@", json)
    }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))
