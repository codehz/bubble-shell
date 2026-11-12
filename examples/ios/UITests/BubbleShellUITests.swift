import XCTest

final class BubbleShellUITests: XCTestCase {
    func testSwipeScrollsTheList() throws {
        let app = XCUIApplication(bundleIdentifier: "dev.bubble.shell")
        app.launch()
        // The driver taps the button, injects a dead loop, then remounts.
        // Swipe after the remount so the list is ready for input.
        sleep(3)
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
        start.press(forDuration: 0.05, thenDragTo: end)
        sleep(1)
    }

    func testGestureAcceptance() {
        let scenes = scenesToRun()
        if scenes.isEmpty { return }
        print("BUBBLE_GESTURE_ONLY \(scenes.joined(separator: ","))")
        for (index, scene) in scenes.enumerated() {
            let app = launch(scene, reset: index == 0)
            if let snap = waitArmed(app, scene) {
                exercise(scene, app, snap)
            } else {
                XCTFail("\(scene) did not arm snap=\(clip(lastSnap))")
            }
            app.terminate()
        }
    }

    private var lastSnap: [String: Any] = [:]

    private func launch(_ scene: String, reset: Bool) -> XCUIApplication {
        let app = XCUIApplication(bundleIdentifier: "dev.bubble.shell")
        app.launchEnvironment = [
            "BUBBLE_GESTURES": "1",
            "BUBBLE_GESTURE_SCENE": scene,
        ]
        if reset {
            app.launchEnvironment["BUBBLE_GESTURE_RESET"] = "1"
        }
        app.launch()
        return app
    }
    private func scenesToRun() -> [String] {
        let all = ["tap", "long", "double", "pan", "transform", "parallel", "reserve", "contain", "dispose", "lifecycle"]
        let raw = ProcessInfo.processInfo.environment["BUBBLE_GESTURE_ONLY"]
            ?? ProcessInfo.processInfo.environment["TEST_RUNNER_BUBBLE_GESTURE_ONLY"]
            ?? ""
        let wanted = raw.split { $0 == "," || $0 == ";" || $0.isWhitespace }.map(String.init).filter { !$0.isEmpty }
        if wanted.isEmpty { return all }
        let picked = wanted.filter { all.contains($0) }
        if picked.isEmpty {
            XCTFail("BUBBLE_GESTURE_ONLY=\(raw) matched no scenes")
        }
        return picked
    }

    private func exercise(_ scene: String, _ app: XCUIApplication, _ armed: [String: Any]) {
        switch scene {
        case "tap":
            guard let pad = node(armed, "pad") else { return fail("tap missing pad") }
            point(app, pad, 0.5, 0.5).tap()
            guard let snap = poll(app, 4, { count($0, "tap", "tap") == 1 && count($0, "tap", "pressEnd") == 1 }) else {
                return fail("tap did not complete")
            }
            expect(count(snap, "tap", "tap") == 1, "one tap")
            expect(count(snap, "tap", "pressStart") == 1 && count(snap, "tap", "pressEnd") == 1 && count(snap, "tap", "pressCancel") == 0, "tap press")
        case "long":
            guard let pad = node(armed, "pad") else { return fail("long missing pad") }
            point(app, pad, 0.5, 0.5).press(forDuration: 0.8)
            guard let snap = poll(app, 4, { count($0, "long", "end") == 1 }) else { return fail("long did not end") }
            expect(count(snap, "long", "start") == 1 && count(snap, "long", "end") == 1 && count(snap, "long", "cancel") == 0, "one long start/end")
            expect(countType(snap, "tap") == 0, "long produced no tap")
        case "double":
            guard let pad = node(armed, "pad") else { return fail("double missing pad") }
            let target = point(app, pad, 0.5, 0.5)
            target.doubleTap()
            guard poll(app, 3, { count($0, "double", "doubleTap") == 1 }) != nil else { return fail("doubleTap missing") }
            let window = Date().addingTimeInterval(0.45)
            while Date() < window {
                if countType(read(app) ?? [:], "tap") > 0 { return fail("tap arrived inside the double-tap window") }
                RunLoop.current.run(until: Date().addingTimeInterval(0.05))
            }
            guard countType(read(app) ?? [:], "tap") == 0 else { return fail("tap arrived inside the double-tap window") }
            target.tap()
            guard let snap = poll(app, 3, { doubleGap($0) }) else { return fail("delayed single tap missing") }
            expect(count(snap, "double", "doubleTap") == 1, "one doubleTap")
            expect(doubleGap(snap), "one tap at least 300ms after the doubleTap")
        case "pan":
            guard let pad = node(armed, "pad") else { return fail("pan missing pad") }
            drag(app, pad, 0.3, 0.5, dx: 100, dy: 0)
            guard let snap = poll(app, 4, { count($0, "pan", "end") == 1 }) else { return fail("pan did not end") }
            expect(count(snap, "pan", "start") == 1 && count(snap, "pan", "update") >= 1 && count(snap, "pan", "end") == 1 && count(snap, "pan", "cancel") == 0, "pan sequence")
            expect((events(snap).filter { ($0["name"] as? String) == "pan" }.map { dbl($0["translationX"]) }.max() ?? 0) > 10, "pan translationX")
            expect(countType(snap, "tap") == 0, "pan produced no tap")
        case "transform":
            let surface = self.surface(app)
            surface.pinch(withScale: 3, velocity: 1.5)
            guard poll(app, 6, {
                count($0, "transform", "start") == 1 && count($0, "transform", "end") == 1
                    && count($0, "transform", "cancel") == 0 && pinched($0)
            }) != nil else { return fail("pinch did not close") }
            surface.rotate(.pi / 2, withVelocity: .pi)
            guard let snap = poll(app, 6, {
                count($0, "transform", "start") == 2 && count($0, "transform", "end") == 2 && rotated($0) && finiteTransform($0)
            }) else { return fail("rotate did not close") }
            expect(count(snap, "transform", "cancel") == 0, "transform not cancelled")
            expect(pinched(snap) && rotated(snap) && finiteTransform(snap), "pinch scale and rotate angle")
            expect(countType(snap, "tap") == 0, "transform produced no tap")
            expect(count(snap, "pan", "end") == 0 && count(snap, "pan", "cancel") == count(snap, "pan", "start"), "pan cancelled only if it started")
        case "parallel":
            guard let outer = node(armed, "outer") else { return fail("parallel missing outer") }
            drag(app, outer, 0.4, 0.5, dx: 90, dy: -90)
            guard let snap = poll(app, 4, { count($0, "pan", "end") == 1 && dbl(node($0, "outer")?["scrollY"]) > 10 }) else {
                return fail("parallel pan/scroll missing")
            }
            let tx = events(snap).filter { ($0["name"] as? String) == "pan" }.map { dbl($0["translationX"]) }.max() ?? 0
            expect(count(snap, "pan", "start") == 1 && count(snap, "pan", "end") == 1 && count(snap, "pan", "cancel") == 0 && tx > 10, "parallel pan")
            expect(dbl(node(snap, "outer")?["scrollY"]) > 10, "parallel scrollY")
        case "reserve":
            guard let outer = node(armed, "outer") else { return fail("reserve missing outer") }
            drag(app, outer, 0.5, 0.7, dx: 0, dy: -120, fast: true)
            guard let snap = poll(app, 4, {
                let scroll = dbl(node($0, "outer")?["scrollY"])
                return scroll > 100 && scroll < 180
            }) else { return fail("reserve displacement missing") }
            let scroll = dbl(node(snap, "outer")?["scrollY"])
            expect(scroll > 100 && scroll < 180, "reserve scrolled once: \(scroll)")
            expect(count(snap, "long", "start") == 0 && count(snap, "long", "end") == 0, "reserve did not long-press")
            expect(countType(snap, "tap") == 0, "reserve produced no tap")
        case "contain":
            guard let inner = node(armed, "inner") else { return fail("contain missing inner") }
            let x = dbl(inner["x"]) + dbl(inner["w"]) * 0.5
            let y = dbl(inner["y"]) + dbl(inner["h"]) - 20
            drag(app, x: x, y: y, dx: 0, dy: -90, fast: true)
            guard let snap = poll(app, 4, {
                let innerY = dbl(node($0, "inner")?["scrollY"])
                return innerY >= 58 && innerY <= 60.5
            }) else { return fail("contain inner did not clamp") }
            let innerY = dbl(node(snap, "inner")?["scrollY"])
            let outerY = dbl(node(snap, "outer")?["scrollY"])
            expect(innerY >= 58 && innerY <= 60.5, "inner scroll \(innerY)")
            expect(abs(outerY) < 1, "outer stayed \(outerY)")
        case "dispose":
            guard let pad = node(armed, "pad") else { return fail("dispose missing pad") }
            drag(app, pad, 0.25, 0.5, dx: 70, dy: 0)
            guard let mid = poll(app, 4, { disposeClosed($0) }) else { return fail("dispose did not cancel") }
            let frozen = events(mid).filter { ($0["name"] as? String) == "pan" }.count
            drag(app, pad, 0.25, 0.55, dx: 70, dy: 0)
            let deadline = Date().addingTimeInterval(0.8)
            var stable = true
            while Date() < deadline {
                if let snap = read(app), events(snap).filter({ ($0["name"] as? String) == "pan" }).count != frozen {
                    stable = false
                    break
                }
                RunLoop.current.run(until: Date().addingTimeInterval(0.05))
            }
            let snap = read(app) ?? mid
            expect(disposeClosed(snap), "one pan cancel and no end")
            expect(stable && events(snap).filter { ($0["name"] as? String) == "pan" }.count == frozen, "later touch produced no pan")
        case "lifecycle":
            let before = dbl(armed["hides"])
            expect(count(armed, "pan", "start") == 0 && countType(armed, "tap") == 0, "lifecycle starts idle")
            XCUIDevice.shared.press(.home)
            app.activate()
            guard let snap = poll(app, 8, {
                dbl($0["hides"]) >= before + 1 && ($0["visible"] as? Bool) == true && count($0, "pan", "cancel") == 0
            }) else { return fail("visibility did not toggle") }
            expect(dbl(snap["hides"]) >= before + 1 && (snap["visible"] as? Bool) == true, "foreground/background wiring")
            expect(count(snap, "pan", "start") == 0 && count(snap, "pan", "cancel") == 0 && count(snap, "pan", "end") == 0, "visibility is not a held-touch cancel")
        default:
            fail("unknown scene \(scene)")
        }
    }

    @discardableResult
    private func expect(_ condition: Bool, _ message: String) -> Bool {
        XCTAssertTrue(condition, "\(message) snap=\(clip(lastSnap))")
        return condition
    }

    private func fail(_ message: String) {
        XCTFail("\(message) snap=\(clip(lastSnap))")
    }

    private func waitArmed(_ app: XCUIApplication, _ scene: String) -> [String: Any]? {
        let element = surface(app)
        _ = element.waitForExistence(timeout: 8)
        return poll(app, 12) { snap in
            guard snap["armed"] as? Bool == true, snap["scene"] as? String == scene, snap["ready"] as? Bool == true else { return false }
            guard (snap["events"] as? [Any])?.isEmpty != false else { return false }
            let primary = scene == "contain" ? "inner" : "pad"
            guard let box = node(snap, primary), dbl(box["w"]) > 1, dbl(box["h"]) > 1 else { return false }
            if scene == "parallel" || scene == "reserve" || scene == "contain" {
                guard let outer = node(snap, "outer"), dbl(outer["w"]) > 1, dbl(outer["h"]) > 1 else { return false }
            }
            return true
        }
    }

    private func poll(_ app: XCUIApplication, _ timeout: TimeInterval, _ body: ([String: Any]) -> Bool) -> [String: Any]? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let snap = read(app) {
                if body(snap) { return snap }
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
        if let snap = read(app), body(snap) { return snap }
        return nil
    }

    private func read(_ app: XCUIApplication) -> [String: Any]? {
        let element = surface(app)
        guard element.exists else { return nil }
        for text in [element.label, element.value as? String ?? ""] where text.contains("{") {
            guard let data = text.data(using: .utf8),
                  let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  obj["nodes"] != nil else { continue }
            lastSnap = obj
            return obj
        }
        return nil
    }

    private func surface(_ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)["bubble-gesture-surface"]
    }

    private func point(_ app: XCUIApplication, _ box: [String: Any], _ fx: Double, _ fy: Double) -> XCUICoordinate {
        let x = dbl(box["x"]) + dbl(box["w"]) * fx
        let y = dbl(box["y"]) + dbl(box["h"]) * fy
        return surface(app).coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y))
    }

    private func drag(_ app: XCUIApplication, _ box: [String: Any], _ fx: Double, _ fy: Double, dx: CGFloat, dy: CGFloat, fast: Bool = false) {
        drag(app, x: dbl(box["x"]) + dbl(box["w"]) * fx, y: dbl(box["y"]) + dbl(box["h"]) * fy, dx: dx, dy: dy, fast: fast)
    }

    private func drag(_ app: XCUIApplication, x: Double, y: Double, dx: CGFloat, dy: CGFloat, fast: Bool) {
        let start = surface(app).coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y))
        let end = start.withOffset(CGVector(dx: dx, dy: dy))
        if fast {
            start.press(forDuration: 0.02, thenDragTo: end, withVelocity: .fast, thenHoldForDuration: 0)
        } else {
            start.press(forDuration: 0.05, thenDragTo: end)
        }
    }

    private func node(_ snap: [String: Any], _ name: String) -> [String: Any]? {
        (snap["nodes"] as? [String: Any])?[name] as? [String: Any]
    }

    private func events(_ snap: [String: Any]) -> [[String: Any]] {
        (snap["events"] as? [Any])?.compactMap { $0 as? [String: Any] } ?? []
    }

    private func count(_ snap: [String: Any], _ name: String, _ type: String) -> Int {
        events(snap).filter { ($0["name"] as? String) == name && ($0["type"] as? String) == type }.count
    }

    private func countType(_ snap: [String: Any], _ type: String) -> Int {
        events(snap).filter { ($0["type"] as? String) == type }.count
    }

    private func dbl(_ value: Any?) -> Double {
        (value as? NSNumber)?.doubleValue ?? .nan
    }

    private func doubleGap(_ snap: [String: Any]) -> Bool {
        let doubles = events(snap).filter { ($0["name"] as? String) == "double" && ($0["type"] as? String) == "doubleTap" }
        let taps = events(snap).filter { ($0["name"] as? String) == "tap" && ($0["type"] as? String) == "tap" }
        guard doubles.count == 1, taps.count == 1 else { return false }
        return dbl(taps[0]["time"]) >= dbl(doubles[0]["time"]) + 300
    }

    private func pinched(_ snap: [String: Any]) -> Bool {
        events(snap).contains { ($0["name"] as? String) == "transform" && abs(dbl($0["scale"]) - 1) > 0.2 }
    }

    private func rotated(_ snap: [String: Any]) -> Bool {
        events(snap).contains { ($0["name"] as? String) == "transform" && abs(dbl($0["rotation"])) > 0.25 }
    }

    private func finiteTransform(_ snap: [String: Any]) -> Bool {
        let transforms = events(snap).filter { ($0["name"] as? String) == "transform" }
        return !transforms.isEmpty && transforms.allSatisfy { event in
            dbl(event["translationX"]).isFinite && dbl(event["translationY"]).isFinite
                && dbl(event["scale"]).isFinite && dbl(event["rotation"]).isFinite
        }
    }

    private func disposeClosed(_ snap: [String: Any]) -> Bool {
        let types = events(snap).filter { ($0["name"] as? String) == "pan" }.map { $0["type"] as? String ?? "" }
        let cancelAt = types.lastIndex(of: "cancel") ?? -1
        let trailing = types.enumerated().contains { index, type in
            cancelAt >= 0 && index > cancelAt && (type == "start" || type == "update" || type == "end")
        }
        return count(snap, "pan", "start") == 1 && count(snap, "pan", "update") >= 1
            && count(snap, "pan", "cancel") == 1 && count(snap, "pan", "end") == 0 && !trailing
    }

    private func clip(_ snap: [String: Any]) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: snap, options: [.sortedKeys]),
              var text = String(data: data, encoding: .utf8) else { return "<unreadable>" }
        if text.count > 1500 { text = String(text.prefix(1500)) }
        return text
    }
}
