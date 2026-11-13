import CoreGraphics
import CoreText
import UIKit

@_silgen_name("bubble_create")
func bubble_create(_ host: UnsafePointer<BubbleHost>?) -> Int32
@_silgen_name("bubble_destroy")
func bubble_destroy()
@_silgen_name("bubble_load")
func bubble_load(_ manifest: UnsafePointer<CChar>?, _ entry: UnsafePointer<UInt8>?, _ entryLen: UInt32) -> Int32
@_silgen_name("bubble_poll_request")
func bubble_poll_request(_ id: UnsafeMutablePointer<UInt32>?, _ method: UnsafeMutablePointer<CChar>?, _ methodCap: UInt32, _ params: UnsafeMutablePointer<CChar>?, _ paramsCap: UInt32) -> Int32
@_silgen_name("bubble_finish_request")
func bubble_finish_request(_ id: UInt32, _ ok: Int32, _ result: UnsafePointer<CChar>?)
@_silgen_name("bubble_frame")
func bubble_frame(_ width: Float, _ height: Float, _ paint: UnsafeMutablePointer<UnsafePointer<UInt8>?>?, _ paintLen: UnsafeMutablePointer<UInt32>?) -> Int32
@_silgen_name("bubble_pointer")
func bubble_pointer(_ action: UInt32, _ id: UInt32, _ x: Float, _ y: Float, _ timeMs: Double, _ pointerKind: UInt32, _ buttons: UInt32) -> Int32
@_silgen_name("bubble_edit")
func bubble_edit(_ action: UInt32, _ text: UnsafePointer<UInt8>?, _ textLen: UInt32, _ a: UInt32, _ b: UInt32) -> Int32
@_silgen_name("bubble_edit_snapshot")
func bubble_edit_snapshot(_ session: UnsafeMutablePointer<UInt64>?, _ revision: UnsafeMutablePointer<UInt64>?, _ anchor: UnsafeMutablePointer<UInt32>?, _ active: UnsafeMutablePointer<UInt32>?, _ compStart: UnsafeMutablePointer<UInt32>?, _ compEnd: UnsafeMutablePointer<UInt32>?, _ composing: UnsafeMutablePointer<UInt32>?) -> Int32
@_silgen_name("bubble_edit_commit")
func bubble_edit_commit(_ session: UInt64, _ text: UnsafePointer<UInt8>?, _ textLen: UInt32, _ newCursorPosition: Int32) -> Int32
@_silgen_name("bubble_edit_compose")
func bubble_edit_compose(_ session: UInt64, _ text: UnsafePointer<UInt8>?, _ textLen: UInt32, _ newCursorPosition: Int32) -> Int32
@_silgen_name("bubble_edit_replace")
func bubble_edit_replace(_ session: UInt64, _ start: UInt32, _ end: UInt32, _ text: UnsafePointer<UInt8>?, _ textLen: UInt32, _ newCursorPosition: Int32, _ composing: UInt32) -> Int32
@_silgen_name("bubble_edit_select")
func bubble_edit_select(_ session: UInt64, _ anchor: UInt32, _ active: UInt32) -> Int32
@_silgen_name("bubble_edit_set_composition")
func bubble_edit_set_composition(_ session: UInt64, _ start: UInt32, _ end: UInt32) -> Int32
@_silgen_name("bubble_edit_finish_composition")
func bubble_edit_finish_composition(_ session: UInt64) -> Int32
@_silgen_name("bubble_edit_delete_surrounding")
func bubble_edit_delete_surrounding(_ session: UInt64, _ before: UInt32, _ after: UInt32) -> Int32
@_silgen_name("bubble_edit_delete_surrounding_code_points")
func bubble_edit_delete_surrounding_code_points(_ session: UInt64, _ before: UInt32, _ after: UInt32) -> Int32
@_silgen_name("bubble_edit_undo")
func bubble_edit_undo(_ session: UInt64) -> Int32
@_silgen_name("bubble_edit_redo")
func bubble_edit_redo(_ session: UInt64) -> Int32
@_silgen_name("bubble_edit_batch_begin")
func bubble_edit_batch_begin(_ session: UInt64) -> Int32
@_silgen_name("bubble_edit_batch_end")
func bubble_edit_batch_end(_ session: UInt64) -> Int32
@_silgen_name("bubble_edit_batch_abort")
func bubble_edit_batch_abort(_ session: UInt64) -> Int32
@_silgen_name("bubble_field_at")
func bubble_field_at(_ x: Float, _ y: Float) -> UInt32
@_silgen_name("bubble_focus_at")
func bubble_focus_at(_ x: Float, _ y: Float) -> UInt32
@_silgen_name("bubble_field_flags")
func bubble_field_flags(_ handle: UInt32) -> UInt32
@_silgen_name("bubble_focused")
func bubble_focused() -> UInt32
@_silgen_name("bubble_key")
func bubble_key(_ action: UInt32, _ timeMs: Double) -> Int32
@_silgen_name("bubble_semantics")
func bubble_semantics(_ buf: UnsafeMutablePointer<CChar>?, _ cap: UInt32) -> Int32
@_silgen_name("bubble_a11y_action")
func bubble_a11y_action(_ handle: UInt32, _ action: UInt32, _ text: UnsafePointer<UInt8>?, _ len: UInt32) -> Int32
@_silgen_name("bubble_edit_menu")
func bubble_edit_menu(_ action: UInt32, _ clip: UnsafePointer<UInt8>?, _ clipLen: UInt32, _ out: UnsafeMutablePointer<CChar>?, _ outCap: UInt32) -> Int32
@_silgen_name("bubble_selection_handles")
func bubble_selection_handles(
    _ sx: UnsafeMutablePointer<Float>?, _ sy: UnsafeMutablePointer<Float>?,
    _ sw: UnsafeMutablePointer<Float>?, _ sh: UnsafeMutablePointer<Float>?,
    _ ex: UnsafeMutablePointer<Float>?, _ ey: UnsafeMutablePointer<Float>?,
    _ ew: UnsafeMutablePointer<Float>?, _ eh: UnsafeMutablePointer<Float>?
) -> Int32
@_silgen_name("bubble_selection")
func bubble_selection(_ start: UnsafeMutablePointer<UInt32>?, _ end: UnsafeMutablePointer<UInt32>?, _ markStart: UnsafeMutablePointer<UInt32>?, _ markEnd: UnsafeMutablePointer<UInt32>?) -> Int32
@_silgen_name("bubble_unit_at")
func bubble_unit_at(_ x: Float, _ y: Float, _ unit: UnsafeMutablePointer<UInt32>?) -> Int32
@_silgen_name("bubble_caret_at")
func bubble_caret_at(_ unit: UInt32, _ x: UnsafeMutablePointer<Float>?, _ y: UnsafeMutablePointer<Float>?, _ w: UnsafeMutablePointer<Float>?, _ h: UnsafeMutablePointer<Float>?) -> Int32
@_silgen_name("bubble_selection_count")
func bubble_selection_count() -> UInt32
@_silgen_name("bubble_selection_rect")
func bubble_selection_rect(_ index: UInt32, _ x: UnsafeMutablePointer<Float>?, _ y: UnsafeMutablePointer<Float>?, _ w: UnsafeMutablePointer<Float>?, _ h: UnsafeMutablePointer<Float>?) -> Int32
@_silgen_name("bubble_tick")
func bubble_tick(_ timeMs: Double) -> Int32
@_silgen_name("bubble_scroll")
func bubble_scroll(_ x: Float, _ y: Float, _ dx: Float, _ dy: Float)
@_silgen_name("bubble_scroll_x_of")
func bubble_scroll_x_of(_ handle: UInt32) -> Float
@_silgen_name("bubble_scroll_y_of")
func bubble_scroll_y_of(_ handle: UInt32) -> Float
@_silgen_name("bubble_register_font")
func bubble_register_font(_ family: UnsafePointer<CChar>?, _ data: UnsafePointer<UInt8>?, _ len: UInt32)
@_silgen_name("bubble_font_epoch")
func bubble_font_epoch() -> UInt32
@_silgen_name("bubble_node_text")
func bubble_node_text(_ handle: UInt32, _ buf: UnsafeMutablePointer<CChar>?, _ cap: UInt32) -> Int32
@_silgen_name("bubble_node_box")
func bubble_node_box(_ handle: UInt32, _ x: UnsafeMutablePointer<Float>?, _ y: UnsafeMutablePointer<Float>?, _ w: UnsafeMutablePointer<Float>?, _ h: UnsafeMutablePointer<Float>?) -> Int32
@_silgen_name("bubble_post_eval")
func bubble_post_eval(_ source: UnsafePointer<CChar>?)
@_silgen_name("bubble_set_visible")
func bubble_set_visible(_ visible: Int32)
@_silgen_name("bubble_faults")
func bubble_faults() -> UInt32
@_silgen_name("bubble_generation")
func bubble_generation() -> UInt32
@_silgen_name("bubble_last_error")
func bubble_last_error() -> UnsafePointer<CChar>?

struct BubbleHost {
    var measure: (@convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, Float, Float, UInt16, Float, Int32, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?) -> Void)?
    var ctx: UnsafeMutableRawPointer?
    var layout: (@convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, Float, Float, Float, UInt16, UInt8, Float, UInt64, UnsafeMutablePointer<UInt8>?, UInt32) -> Int32)?
}

var hostFamily = "Arial"

let measure: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, Float, Float, UInt16, Float, Int32, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?) -> Void = { _, text, len, fontSize, lineHeight, weight, maxWidth, hasMax, outW, outH in
    let bytes = text.map { Array(UnsafeBufferPointer(start: $0, count: Int(len))) } ?? []
    let string = String(bytes: bytes, encoding: .utf8) ?? ""
    let size = hostFamily.isEmpty
        ? UIFont.systemFont(ofSize: CGFloat(fontSize), weight: weight >= 600 ? .semibold : .regular)
        : (UIFont(name: hostFamily, size: CGFloat(fontSize)) ?? UIFont.systemFont(ofSize: CGFloat(fontSize)))
    let limit = hasMax != 0 ? CGFloat(maxWidth) : CGFloat.greatestFiniteMagnitude
    let rect = (string as NSString).boundingRect(
        with: CGSize(width: limit, height: .greatestFiniteMagnitude),
        options: [.usesLineFragmentOrigin, .usesFontLeading],
        attributes: [.font: size],
        context: nil
    )
    outW?.pointee = Float(ceil(rect.width))
    if lineHeight > 0 {
        let natural = ceil(size.lineHeight)
        let lines = max(1, ceil(rect.height / natural))
        outH?.pointee = Float(lines * CGFloat(lineHeight))
    } else {
        outH?.pointee = Float(ceil(rect.height))
    }
}

let layoutText: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, Float, Float, Float, UInt16, UInt8, Float, UInt64, UnsafeMutablePointer<UInt8>?, UInt32) -> Int32 = { _, text, len, width, fontSize, lineHeight, weight, align, density, revision, out, cap in
    let bytes = text.map { Array(UnsafeBufferPointer(start: $0, count: Int(len))) } ?? []
    let string = String(bytes: bytes, encoding: .utf8) ?? ""
    let data = layoutSnapshot(string, CGFloat(width), CGFloat(fontSize), CGFloat(lineHeight), weight, align, density, revision)
    if data.count > Int(cap) { return Int32(data.count) }
    guard let out else { return -1 }
    data.withUnsafeBytes { raw in
        if let base = raw.bindMemory(to: UInt8.self).baseAddress {
            out.update(from: base, count: data.count)
        }
    }
    return Int32(data.count)
}

func layoutSnapshot(_ text: String, _ width: CGFloat, _ fontSize: CGFloat, _ lineHeight: CGFloat, _ weight: UInt16, _ align: UInt8, _ density: Float, _ revision: UInt64) -> Data {
    let font = hostFamily.isEmpty
        ? UIFont.systemFont(ofSize: fontSize, weight: weight >= 600 ? .semibold : .regular)
        : (UIFont(name: hostFamily, size: fontSize) ?? UIFont.systemFont(ofSize: fontSize))
    let paragraph = NSMutableParagraphStyle()
    if lineHeight > 0 {
        paragraph.minimumLineHeight = lineHeight
        paragraph.maximumLineHeight = lineHeight
    }
    paragraph.alignment = align == 1 ? .center : (align == 2 ? .right : .left)
    let attributed = NSAttributedString(string: text, attributes: [.font: font, .paragraphStyle: paragraph])
    let setter = CTFramesetterCreateWithAttributedString(attributed)
    let pathHeight: CGFloat = 100000
    let path = CGPath(rect: CGRect(x: 0, y: 0, width: max(width, 1), height: pathHeight), transform: nil)
    let frame = CTFramesetterCreateFrame(setter, CFRange(location: 0, length: attributed.length), path, nil)
    let lines = (CTFrameGetLines(frame) as? [CTLine]) ?? []
    var origins = Array(repeating: CGPoint.zero, count: max(lines.count, 1))
    if !lines.isEmpty {
        CTFrameGetLineOrigins(frame, CFRange(location: 0, length: 0), &origins)
    }
    let units = text.utf16.count
    var carets = Array(repeating: (Float(0), Float(0), Float(lineHeight)), count: units + 1)
    var runs: [(UInt32, UInt32, Float, Float, Float, Float, UInt8)] = []
    if lines.isEmpty {
        carets[0] = (0, 0, Float(lineHeight))
    }
    for (index, line) in lines.enumerated() {
        var ascent: CGFloat = 0
        var descent: CGFloat = 0
        var leading: CGFloat = 0
        CTLineGetTypographicBounds(line, &ascent, &descent, &leading)
        let y = Float(pathHeight - origins[index].y - ascent)
        let h = Float(max(lineHeight, ascent + descent))
        let range = CTLineGetStringRange(line)
        let start = range.location
        var end = range.location + range.length
        let ns = text as NSString
        while end > start {
            let ch = ns.character(at: end - 1)
            if ch != 10 && ch != 13 { break }
            end -= 1
        }
        if start <= units {
            for unit in start...min(end, units) {
                let x = Float(CTLineGetOffsetForStringIndex(line, unit, nil))
                carets[unit] = (x, y, h)
            }
        }
        if end > start {
            let glyphRuns = (CTLineGetGlyphRuns(line) as? [CTRun]) ?? []
            for run in glyphRuns {
                let rr = CTRunGetStringRange(run)
                let runStart = max(rr.location, start)
                let runEnd = min(rr.location + rr.length, end)
                if runStart >= runEnd { continue }
                let x0 = CGFloat(CTLineGetOffsetForStringIndex(line, runStart, nil))
                let x1 = CGFloat(CTLineGetOffsetForStringIndex(line, runEnd, nil))
                let level: UInt8 = CTRunGetStatus(run).contains(.rightToLeft) ? 1 : 0
                runs.append((UInt32(runStart), UInt32(runEnd), Float(min(x0, x1)), y, Float(abs(x1 - x0)), h, level))
            }
        }
    }
    var data = Data()
    func put32(_ value: UInt32) { var v = value.littleEndian; data.append(Data(bytes: &v, count: 4)) }
    func put16(_ value: UInt16) { var v = value.littleEndian; data.append(Data(bytes: &v, count: 2)) }
    func put64(_ value: UInt64) { var v = value.littleEndian; data.append(Data(bytes: &v, count: 8)) }
    func putF(_ value: Float) { var v = value.bitPattern.littleEndian; data.append(Data(bytes: &v, count: 4)) }
    put32(0x314C5342)
    put32(UInt32(units))
    put32(UInt32(runs.count))
    putF(Float(width))
    putF(Float(fontSize))
    putF(Float(lineHeight))
    putF(density)
    put16(weight)
    data.append(align)
    data.append(0)
    put64(revision)
    for caret in carets {
        putF(caret.0); putF(caret.1); putF(caret.2)
    }
    for run in runs {
        put32(run.0); put32(run.1); putF(run.2); putF(run.3); putF(run.4); putF(run.5)
        data.append(run.6); data.append(0); data.append(0); data.append(0)
    }
    return data
}

func drainHostCalls() {
    while true {
        var id: UInt32 = 0
        let method = UnsafeMutablePointer<CChar>.allocate(capacity: 128)
        let params = UnsafeMutablePointer<CChar>.allocate(capacity: 8192)
        method.initialize(repeating: 0, count: 128)
        params.initialize(repeating: 0, count: 8192)
        let code = bubble_poll_request(&id, method, 128, params, 8192)
        let methodName = String(cString: method)
        let paramText = String(cString: params)
        method.deallocate()
        params.deallocate()
        if code != 1 { return }
        let object = (try? JSONSerialization.jsonObject(with: Data(paramText.utf8))) as? [String: Any] ?? [:]
        func finish(_ ok: Int32, _ result: String) {
            result.withCString { bubble_finish_request(id, ok, $0) }
        }
        switch methodName {
        case "toast":
            print("toast:", object["message"] ?? "")
            finish(1, "null")
        case "openURL":
            if let url = URL(string: String(describing: object["url"] ?? "")) {
                UIApplication.shared.open(url)
            }
            finish(1, "null")
        case "clipboard.readText":
            let text = UIPasteboard.general.string ?? ""
            let quoted = String(data: (try? JSONEncoder().encode(text)) ?? Data("\"\"".utf8), encoding: .utf8) ?? "\"\""
            finish(1, quoted)
        case "clipboard.writeText":
            UIPasteboard.general.string = String(describing: object["text"] ?? "")
            finish(1, "null")
        case "fetch":
            let urlText = String(describing: object["url"] ?? "")
            guard let url = URL(string: urlText) else {
                finish(0, "bad url")
                continue
            }
            var request = URLRequest(url: url)
            if let verb = object["method"] as? String, !verb.isEmpty { request.httpMethod = verb }
            if let body = object["body"] as? String { request.httpBody = Data(body.utf8) }
            URLSession.shared.dataTask(with: request) { data, response, error in
                if let error {
                    finish(0, error.localizedDescription)
                    return
                }
                let status = (response as? HTTPURLResponse)?.statusCode ?? 0
                let text = data.flatMap { String(data: $0, encoding: .utf8) } ?? ""
                let encoded = String(data: (try? JSONEncoder().encode(text)) ?? Data("\"\"".utf8), encoding: .utf8) ?? "\"\""
                finish(1, "{\"status\":\(status),\"text\":\(encoded)}")
            }.resume()
        default:
            finish(0, "no host handler for \(methodName)")
        }
    }
}


final class BubbleView: UIView {
    var paint = Data()
    var family = "Arial"
    private var imageCache: [String: UIImage] = [:]
    weak var inputDelegate: UITextInputDelegate?
    lazy var textTokenizer: UITextInputStringTokenizer = UITextInputStringTokenizer(textInput: self)

    private func imageFor(_ src: String) -> UIImage? {
        if let image = imageCache[src] {
            return image
        }
        let path = src.hasPrefix("/") ? src : (Bundle.main.path(forResource: src, ofType: nil) ?? "")
        guard let image = UIImage(contentsOfFile: path) else {
            return nil
        }
        imageCache[src] = image
        return image
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        isMultipleTouchEnabled = true
        contentMode = .redraw
        backgroundColor = .black
        isAccessibilityElement = false
    }

    required init?(coder: NSCoder) { nil }
    func contentSize() -> CGSize {
        let inset = safeAreaInsets
        return CGSize(width: max(1, bounds.width - inset.left - inset.right), height: max(1, bounds.height - inset.top - inset.bottom))
    }

    private var pointerIds: [ObjectIdentifier: UInt32] = [:]
    private var nextPointerId: UInt32 = 1
    var keyboardKind: UIKeyboardType = .default
    var secureEntry = false
    var contentKind: UITextContentType? = nil
    var tracePointers = false
    private var traceOrigin: [UInt32: CGPoint] = [:]
    private var traceLast: [UInt32: CGPoint] = [:]
    private var traceMoves: [UInt32: Int] = [:]
    private var selectionDrag: (touch: ObjectIdentifier, anchor: Int)?
    private var semanticElements: [UIAccessibilityElement] = []
    private var semanticSnapshot = Data()
    private var modalHandle: UInt32 = 0
    private var inputSession: UInt64 = 0
    private let inactiveInputView = UIView(frame: .zero)
    private var exposesGestureSurface = false
    override var isAccessibilityElement: Bool {
        get { exposesGestureSurface }
        set { exposesGestureSurface = newValue }
    }

    fileprivate func handleNavigationPress(_ key: UIKey) -> Bool {
        guard key.modifierFlags.intersection([.command, .control, .alternate]).isEmpty else { return false }
        let action: UInt32
        switch key.keyCode {
        case .keyboardTab: action = key.modifierFlags.contains(.shift) ? 1 : 0
        case .keyboardEscape: action = 4
        case .keyboardReturnOrEnter where editSnapshot() == nil: action = 2
        case .keyboardSpacebar where editSnapshot() == nil: action = 3
        default: return false
        }
        guard bubble_key(action, CACurrentMediaTime() * 1000) > 0 else { return false }
        afterNavigationKey()
        return true
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let remaining = presses.filter { press in
            guard let key = press.key else { return true }
            return !handleNavigationPress(key)
        }
        if !remaining.isEmpty { super.pressesBegan(Set(remaining), with: event) }
    }
    override var accessibilityElements: [Any]? {
        get { semanticElements.isEmpty ? nil : semanticElements }
        set { _ = newValue }
    }

    private func pointerId(_ touch: UITouch) -> UInt32 {
        let key = ObjectIdentifier(touch)
        if let id = pointerIds[key] {
            return id
        }
        let id = nextPointerId
        nextPointerId += 1
        pointerIds[key] = id
        return id
    }

    private func sendPointer(_ action: UInt32, _ touch: UITouch, id known: UInt32? = nil) {
        let id = known ?? pointerId(touch)
        let point = touch.location(in: self)
        let logicalX = Float(point.x - safeAreaInsets.left)
        let logicalY = Float(point.y - safeAreaInsets.top)
        if tracePointers {
            logPointerSample(action, id, logicalX, logicalY)
        }
        _ = bubble_pointer(
            action,
            id,
            logicalX,
            logicalY,
            CACurrentMediaTime() * 1000,
            0,
            action == 2 || action == 3 ? 0 : 1
        )
    }

    private func logPointerSample(_ action: UInt32, _ id: UInt32, _ x: Float, _ y: Float) {
        let logical = CGPoint(x: CGFloat(x), y: CGFloat(y))
        let origin = traceOrigin[id] ?? logical
        let previous = traceLast[id] ?? logical
        let dx = logical.x - origin.x
        let dy = logical.y - origin.y
        let name: String
        var emit = false
        switch action {
        case 0:
            name = "DOWN"
            traceOrigin[id] = logical
            traceLast[id] = logical
            traceMoves[id] = 0
            emit = true
        case 1:
            name = "MOVE"
            let seen = traceMoves[id] ?? 0
            traceLast[id] = logical
            if seen < 3 {
                traceMoves[id] = seen + 1
                emit = true
            }
        case 2:
            name = "UP"
            traceLast[id] = logical
            emit = true
            traceOrigin.removeValue(forKey: id)
            traceLast.removeValue(forKey: id)
        default:
            name = "CANCEL"
            emit = true
            traceOrigin.removeValue(forKey: id)
            traceLast.removeValue(forKey: id)
        }
        guard emit else { return }
        NSLog(
            "BUBBLE_GESTURE_POINTER %@ id=%u logical=(%.2f,%.2f) dDown=(%.2f,%.2f) dist=%.2f step=%.2f",
            name,
            id,
            x,
            y,
            Float(dx),
            Float(dy),
            Float(hypot(dx, dy)),
            Float(hypot(logical.x - previous.x, logical.y - previous.y))
        )
    }

    func dropTrackedTouches() {
        pointerIds.removeAll()
        traceOrigin.removeAll()
        traceLast.removeAll()
        traceMoves.removeAll()
    }

    func configureGestureSurface(_ enabled: Bool) {
        isAccessibilityElement = enabled
        accessibilityIdentifier = enabled ? "bubble-gesture-surface" : nil
        if !enabled {
            accessibilityLabel = nil
            accessibilityValue = nil
        }
    }

    func publishGestureSnapshot(_ json: String) {
        accessibilityLabel = json
        accessibilityValue = json
    }

    func refresh() {
        _ = bubble_tick(CACurrentMediaTime() * 1000)
        drainHostCalls()
        var ptr: UnsafePointer<UInt8>?
        var len: UInt32 = 0
        let size = contentSize()
        let code = bubble_frame(Float(size.width), Float(size.height), &ptr, &len)
        if code == 0, let ptr, len > 0 {
            paint = Data(bytes: ptr, count: Int(len))
        }
        updateAccessibility()
        updateInputSession()
        setNeedsDisplay()
    }
    override func draw(_ rect: CGRect) {
        guard let ctx = UIGraphicsGetCurrentContext() else { return }
        ctx.translateBy(x: safeAreaInsets.left, y: safeAreaInsets.top)
        var cursor = paint.startIndex
        func take(_ n: Int) -> Data? {
            guard cursor + n <= paint.endIndex else { return nil }
            let slice = paint[cursor..<(cursor + n)]
            cursor += n
            return Data(slice)
        }
        func u32() -> UInt32? {
            guard let raw = take(4) else { return nil }
            return raw.withUnsafeBytes { $0.load(as: UInt32.self) }.littleEndian
        }
        func f32() -> CGFloat? {
            guard let raw = take(4) else { return nil }
            let bits = raw.withUnsafeBytes { $0.load(as: UInt32.self) }.littleEndian
            return CGFloat(Float(bitPattern: bits))
        }
        func u16() -> UInt16? {
            guard let raw = take(2) else { return nil }
            return raw.withUnsafeBytes { $0.load(as: UInt16.self) }.littleEndian
        }
        func u8() -> UInt8? {
            guard let raw = take(1) else { return nil }
            return raw.first
        }
        guard let count = u32() else { return }
        var alpha: CGFloat = 1
        var alphas: [CGFloat] = []
        for _ in 0..<count {
            guard let tag = paint.indices.contains(cursor) ? paint[cursor] : nil else { return }
            cursor += 1
            switch tag {
            case 1:
                guard let x = f32(), let y = f32(), let w = f32(), let h = f32(), let radius = f32(), let red = f32(), let green = f32(), let blue = f32(), let alpha = f32() else { return }
                colorOf(red, green, blue, alpha).setFill()
                UIBezierPath(roundedRect: CGRect(x: x, y: y, width: w, height: h), cornerRadius: radius).fill()
            case 2:
                guard let x = f32(), let y = f32(), let w = f32(), let h = f32(), let font = f32(), let red = f32(), let green = f32(), let blue = f32(), let alpha = f32(), let lineHeight = f32(), let weight = u16(), let align = u8(), let nbytes = u32() else { return }
                guard let raw = take(Int(nbytes)) else { return }
                let text = String(data: raw, encoding: .utf8) ?? ""
                let uiFont: UIFont = hostFamily.isEmpty
                    ? UIFont.systemFont(ofSize: font, weight: weight >= 600 ? .semibold : .regular)
                    : (UIFont(name: hostFamily, size: font) ?? UIFont.systemFont(ofSize: font))
                let paragraph = NSMutableParagraphStyle()
                if lineHeight > 0 {
                    paragraph.minimumLineHeight = lineHeight
                    paragraph.maximumLineHeight = lineHeight
                }
                paragraph.alignment = align == 1 ? .center : (align == 2 ? .right : .left)
                let color = colorOf(red, green, blue, alpha)
                let attributed = NSAttributedString(string: text, attributes: [
                    .font: uiFont,
                    .foregroundColor: color,
                    .paragraphStyle: paragraph,
                ])
                let pathHeight: CGFloat = 100000
                let setter = CTFramesetterCreateWithAttributedString(attributed)
                let path = CGPath(rect: CGRect(x: 0, y: 0, width: max(CGFloat(w), 1), height: pathHeight), transform: nil)
                let frame = CTFramesetterCreateFrame(setter, CFRange(location: 0, length: attributed.length), path, nil)
                if let ctx = UIGraphicsGetCurrentContext() {
                    ctx.saveGState()
                    ctx.translateBy(x: CGFloat(x), y: CGFloat(y))
                    ctx.clip(to: CGRect(x: 0, y: 0, width: CGFloat(max(w, 0)), height: CGFloat(max(h, 0))))
                    ctx.translateBy(x: 0, y: pathHeight)
                    ctx.scaleBy(x: 1, y: -1)
                    ctx.setFillColor(color.cgColor)
                    CTFrameDraw(frame, ctx)
                    ctx.restoreGState()
                }
            case 7:
                guard let x = f32(), let y = f32(), let w = f32(), let h = f32(), let radius = f32(), let fit = u8(), let nbytes = u32() else { return }
                guard let raw = take(Int(nbytes)) else { return }
                let src = String(data: raw, encoding: .utf8) ?? ""
                guard let image = imageFor(src) else {
                    break
                }
                let size = image.size
                var rect = CGRect(x: x, y: y, width: w, height: h)
                if fit == 0 {
                    let scale = min(w / max(size.width, 1), h / max(size.height, 1))
                    let dw = size.width * scale
                    let dh = size.height * scale
                    rect = CGRect(x: x + (w - dw) / 2, y: y + (h - dh) / 2, width: dw, height: dh)
                } else if fit == 1 {
                    let scale = max(w / max(size.width, 1), h / max(size.height, 1))
                    let dw = size.width * scale
                    let dh = size.height * scale
                    rect = CGRect(x: x + (w - dw) / 2, y: y + (h - dh) / 2, width: dw, height: dh)
                }
                ctx.saveGState()
                UIBezierPath(roundedRect: CGRect(x: x, y: y, width: w, height: h), cornerRadius: radius).addClip()
                image.draw(in: rect)
                ctx.restoreGState()
            case 3:
                guard let x = f32(), let y = f32(), let w = f32(), let h = f32() else { return }
                ctx.saveGState()
                ctx.clip(to: CGRect(x: x, y: y, width: w, height: h))
            case 4:
                ctx.restoreGState()
            case 5:
                guard let opacity = f32() else { return }
                alphas.append(alpha)
                alpha *= opacity
                ctx.saveGState()
                ctx.setAlpha(alpha)
            case 6:
                ctx.restoreGState()
                alpha = alphas.popLast() ?? 1
            default:
                return
            }
        }
        drawSelectionHandles(ctx)
    }

    private func drawSelectionHandles(_ ctx: CGContext) {
        var sx: Float = 0, sy: Float = 0, sw: Float = 0, sh: Float = 0
        var ex: Float = 0, ey: Float = 0, ew: Float = 0, eh: Float = 0
        guard bubble_selection_handles(&sx, &sy, &sw, &sh, &ex, &ey, &ew, &eh) == 0 else { return }
        ctx.setFillColor(UIColor(red: 0.30, green: 0.64, blue: 1.0, alpha: 0.95).cgColor)
        for (x, y, w, h) in [(sx, sy, sw, sh), (ex, ey, ew, eh)] {
            ctx.fillEllipse(in: CGRect(x: CGFloat(x) + CGFloat(w) / 2 - 7, y: CGFloat(y) + CGFloat(h) / 2 - 7, width: 14, height: 14))
        }
    }

    private func updateAccessibility() {
        if isAccessibilityElement {
            return
        }
        var buf = [CChar](repeating: 0, count: 262144)
        guard bubble_semantics(&buf, UInt32(buf.count)) == 0 else { return }
        let data = Data(buf.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) })
        guard data != semanticSnapshot else { return }
        semanticSnapshot = data
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let nodes = object["nodes"] as? [[String: Any]] else { return }
        let previous = Dictionary(uniqueKeysWithValues: semanticElements.compactMap { element in
            (element as? BubbleA11yElement).map { ($0.handle, $0) }
        })
        let modal = (object["modal"] as? NSNumber)?.uint32Value ?? 0
        let modalChanged = modal != modalHandle
        modalHandle = modal
        accessibilityViewIsModal = modal != 0
        semanticElements = nodes.compactMap { node in
            guard node["hidden"] as? Bool != true,
                  let raw = node["handle"] as? NSNumber else { return nil }
            let element = previous[raw.uint32Value]
                ?? BubbleA11yElement(accessibilityContainer: self, handle: raw.uint32Value, view: self)
            let bounds = node["bounds"] as? [Double] ?? [0, 0, 0, 0]
            element.accessibilityFrameInContainerSpace = CGRect(
                x: CGFloat(bounds[0]) + safeAreaInsets.left,
                y: CGFloat(bounds[1]) + safeAreaInsets.top,
                width: CGFloat(bounds[2]),
                height: CGFloat(bounds[3])
            )
            let role = node["role"] as? String ?? "generic"
            let label = node["label"] as? String ?? ""
            let value = node["value"] as? String ?? ""
            element.accessibilityLabel = role == "textbox" || role == "searchbox"
                ? [label, value].filter { !$0.isEmpty }.joined(separator: ": ")
                : (label.isEmpty ? value : label)
            element.accessibilityValue = role == "textbox" || role == "searchbox" ? value : nil
            var traits: UIAccessibilityTraits = []
            switch role {
            case "button": traits.insert(.button)
            case "link": traits.insert(.link)
            case "heading": traits.insert(.header)
            case "image": traits.insert(.image)
            case "text": traits.insert(.staticText)
            case "textbox", "searchbox": traits.insert(.updatesFrequently)
            default: break
            }
            if node["selected"] as? Bool == true { traits.insert(.selected) }
            if node["disabled"] as? Bool == true { traits.insert(.notEnabled) }
            element.accessibilityTraits = traits
            return element
        }
        UIAccessibility.post(notification: modalChanged ? .screenChanged : .layoutChanged,
                             argument: modalChanged ? semanticElements.first : nil)
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches {
            let local = contentPoint(touch.location(in: self))
            var sx: Float = 0, sy: Float = 0, sw: Float = 0, sh: Float = 0
            var ex: Float = 0, ey: Float = 0, ew: Float = 0, eh: Float = 0
            if bubble_field_at(Float(local.x), Float(local.y)) != 0,
               bubble_selection_handles(&sx, &sy, &sw, &sh, &ex, &ey, &ew, &eh) == 0 {
                var start: UInt32 = 0
                var end: UInt32 = 0
                var markStart: UInt32 = 0
                var markEnd: UInt32 = 0
                _ = bubble_selection(&start, &end, &markStart, &markEnd)
                let startHit = hypot(local.x - CGFloat(sx + sw / 2), local.y - CGFloat(sy + sh / 2)) <= 16
                let endHit = hypot(local.x - CGFloat(ex + ew / 2), local.y - CGFloat(ey + eh / 2)) <= 16
                if startHit || endHit {
                    selectionDrag = (ObjectIdentifier(touch), Int(startHit ? end : start))
                    return
                }
            }
            sendPointer(0, touch)
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches {
            if let drag = selectionDrag, drag.touch == ObjectIdentifier(touch),
               let snapshot = editSnapshot() {
                let local = contentPoint(touch.location(in: self))
                var unit: UInt32 = 0
                if bubble_unit_at(Float(local.x), Float(local.y), &unit) == 0 {
                    _ = bubble_edit_select(snapshot.session, UInt32(drag.anchor), unit)
                    refresh()
                }
                return
            }
            sendPointer(1, touch)
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        var delivered = false
        for touch in touches {
            let key = ObjectIdentifier(touch)
            if let drag = selectionDrag, drag.touch == key {
                selectionDrag = nil
                showEditMenu()
                continue
            }
            guard pointerIds[key] != nil else { continue }
            delivered = true
            sendPointer(2, touch)
            let point = touch.location(in: self)
            let handle = bubble_focus_at(
                Float(point.x - safeAreaInsets.left),
                Float(point.y - safeAreaInsets.top)
            )
            if handle != 0 {
                updateInputSession()
                _ = becomeFirstResponder()
            }
            pointerIds.removeValue(forKey: key)
        }
        if delivered { refresh() }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        selectionDrag = nil
        for touch in touches {
            let key = ObjectIdentifier(touch)
            guard let id = pointerIds.removeValue(forKey: key) else { continue }
            sendPointer(3, touch, id: id)
        }
    }

    private func showEditMenu() {
        guard bubble_focused() != 0 else { return }
        var start: UInt32 = 0
        var end: UInt32 = 0
        var markStart: UInt32 = 0
        var markEnd: UInt32 = 0
        _ = bubble_selection(&start, &end, &markStart, &markEnd)
        guard start != end else { return }
        var x: Float = 0, y: Float = 0, w: Float = 0, h: Float = 0
        _ = bubble_selection_rect(0, &x, &y, &w, &h)
        _ = becomeFirstResponder()
        UIMenuController.shared.showMenu(
            from: self,
            rect: CGRect(
                x: CGFloat(x) + safeAreaInsets.left,
                y: CGFloat(y) + safeAreaInsets.top,
                width: max(1, CGFloat(w)),
                height: max(1, CGFloat(h))
            )
        )
    }

    private func runEditMenu(_ action: UInt32) {
        var clip = ""
        if action == 2 {
            clip = UIPasteboard.general.string ?? ""
        }
        let bytes = Array(clip.utf8)
        var out = [CChar](repeating: 0, count: 65536)
        let changed = bytes.withUnsafeBufferPointer { buffer in
            bubble_edit_menu(action, buffer.baseAddress, UInt32(bytes.count), &out, UInt32(out.count))
        }
        if changed >= 0 && (action == 0 || action == 1) {
            UIPasteboard.general.string = String(decoding: out.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
        }
        UIMenuController.shared.hideMenu()
        refresh()
        reloadInputViews()
    }

    override func cut(_ sender: Any?) { runEditMenu(0) }
    override func copy(_ sender: Any?) { runEditMenu(1) }
    override func paste(_ sender: Any?) { runEditMenu(2) }
    override func selectAll(_ sender: Any?) { runEditMenu(3) }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if bubble_focused() != 0 {
            if action == #selector(cut(_:)) || action == #selector(copy(_:)) ||
                action == #selector(paste(_:)) || action == #selector(selectAll(_:)) {
                return true
            }
        }
        return super.canPerformAction(action, withSender: sender)
    }

    fileprivate func afterNavigationKey() {
        refresh()
        synchronizeResponder()
    }

    fileprivate func synchronizeResponder() {
        if !isFirstResponder { _ = becomeFirstResponder() }
    }

    override var inputView: UIView? { editSnapshot() == nil ? inactiveInputView : nil }

    private func updateInputSession() {
        let session = editSnapshot()?.session ?? 0
        guard session != inputSession else { return }
        inputSession = session
        let flags = bubble_field_flags(bubble_focused())
        secureEntry = flags & 2 != 0
        switch (flags >> 8) & 0xff {
        case 1: keyboardKind = .numberPad
        case 2: keyboardKind = .emailAddress
        case 3: keyboardKind = .URL
        case 4: keyboardKind = .phonePad
        default: keyboardKind = .default
        }
        switch (flags >> 16) & 0xff {
        case 1: contentKind = .username
        case 2: contentKind = .password
        case 3: contentKind = .newPassword
        case 4: contentKind = .emailAddress
        case 5: contentKind = .telephoneNumber
        case 6: contentKind = .oneTimeCode
        default: contentKind = nil
        }
        if isFirstResponder { reloadInputViews() }
    }
}

final class BubbleA11yElement: UIAccessibilityElement {
    let handle: UInt32
    weak var view: BubbleView?

    init(accessibilityContainer: Any, handle: UInt32, view: BubbleView) {
        self.handle = handle
        self.view = view
        super.init(accessibilityContainer: accessibilityContainer)
    }

    override func accessibilityActivate() -> Bool {
        guard bubble_a11y_action(handle, 1, nil, 0) == 0 else { return false }
        view?.afterNavigationKey()
        return true
    }

    override func accessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
        let action: UInt32
        switch direction {
        case .left: action = 5
        case .right: action = 6
        case .up: action = 7
        case .down: action = 8
        case .next: action = 3
        case .previous: action = 4
        @unknown default: return false
        }
        guard bubble_a11y_action(handle, action, nil, 0) == 0 else { return false }
        view?.refresh()
        return true
    }

    override func accessibilityElementDidBecomeFocused() {
        if bubble_a11y_action(handle, 0, nil, 0) == 0 { view?.afterNavigationKey() }
    }
}

final class IndexedPosition: UITextPosition {
    let index: Int
    init(_ index: Int) {
        self.index = max(0, index)
        super.init()
    }
}

final class IndexedRange: UITextRange {
    let startIndex: Int
    let endIndex: Int
    init(_ start: Int, _ end: Int) {
        startIndex = min(start, end)
        endIndex = max(start, end)
        super.init()
    }
    override var start: UITextPosition { IndexedPosition(startIndex) }
    override var end: UITextPosition { IndexedPosition(endIndex) }
    override var isEmpty: Bool { startIndex == endIndex }
}

final class IndexedSelection: UITextSelectionRect {
    let rectValue: CGRect
    let starts: Bool
    let ends: Bool
    init(_ rect: CGRect, starts: Bool, ends: Bool) {
        rectValue = rect
        self.starts = starts
        self.ends = ends
        super.init()
    }
    override var rect: CGRect { rectValue }
    override var writingDirection: NSWritingDirection { .leftToRight }
    override var containsStart: Bool { starts }
    override var containsEnd: Bool { ends }
    override var isVertical: Bool { false }
}

struct EditSnapshot {
    let session: UInt64
    let revision: UInt64
    let anchor: Int
    let active: Int
    let compositionStart: Int
    let compositionEnd: Int
    let composing: Bool

    var start: Int { min(anchor, active) }
    var end: Int { max(anchor, active) }
    var hasComposition: Bool { compositionEnd != Int(UInt32.max) }
}

extension BubbleView: UITextInput {
    override var canBecomeFirstResponder: Bool { true }
    override var keyCommands: [UIKeyCommand]? {
        let commands = [
            UIKeyCommand(input: "z", modifierFlags: .command, action: #selector(undoEditing)),
            UIKeyCommand(input: "z", modifierFlags: [.command, .shift], action: #selector(redoEditing)),
        ]
        for command in commands { command.wantsPriorityOverSystemBehavior = true }
        return commands
    }

    @objc func undoEditing() {
        changeText {
            guard let snapshot = editSnapshot() else { return }
            _ = bubble_edit_undo(snapshot.session)
        }
    }

    @objc func redoEditing() {
        changeText {
            guard let snapshot = editSnapshot() else { return }
            _ = bubble_edit_redo(snapshot.session)
        }
    }

    var keyboardType: UIKeyboardType {
        get { keyboardKind }
        set { _ = newValue }
    }
    var isSecureTextEntry: Bool {
        get { secureEntry }
        set { _ = newValue }
    }
    var textContentType: UITextContentType! {
        get { contentKind }
        set { _ = newValue }
    }
    var autocorrectionType: UITextAutocorrectionType {
        get { secureEntry ? .no : .default }
        set { _ = newValue }
    }
    var spellCheckingType: UITextSpellCheckingType {
        get { secureEntry ? .no : .default }
        set { _ = newValue }
    }
    var hasText: Bool { !focusedFieldText().isEmpty }
    var tokenizer: UITextInputTokenizer { textTokenizer }
    var markedTextStyle: [NSAttributedString.Key: Any]? {
        get { nil }
        set {}
    }

    func insertText(_ text: String) {
        if editSnapshot() == nil {
            let action: UInt32
            switch text {
            case "\n", "\r": action = 2
            case " ": action = 3
            default: return
            }
            if bubble_key(action, CACurrentMediaTime() * 1000) > 0 { afterNavigationKey() }
            return
        }
        changeText {
            guard let snapshot = editSnapshot() else { return }
            editCommit(snapshot.session, text: text)
        }
    }

    func deleteBackward() {
        changeText {
            guard let snapshot = editSnapshot() else { return }
            if snapshot.hasComposition || snapshot.start != snapshot.end {
                editCommit(snapshot.session, text: "")
            } else {
                _ = bubble_edit_delete_surrounding(snapshot.session, 1, 0)
            }
        }
    }

    var selectedTextRange: UITextRange? {
        get {
            guard let (start, end, _, _) = selectionUnits() else { return nil }
            return IndexedRange(start, end)
        }
        set {
            guard let range = newValue as? IndexedRange, let snapshot = editSnapshot() else { return }
            changeText {
                _ = bubble_edit_select(
                    snapshot.session,
                    UInt32(range.startIndex),
                    UInt32(range.endIndex),
                )
            }
        }
    }

    var markedTextRange: UITextRange? {
        guard let snapshot = editSnapshot(), snapshot.hasComposition else { return nil }
        return IndexedRange(snapshot.compositionStart, snapshot.compositionEnd)
    }

    func text(in range: UITextRange) -> String? {
        guard let range = range as? IndexedRange else { return nil }
        return utf16Slice(focusedFieldText(), range.startIndex, range.endIndex)
    }

    func replace(_ range: UITextRange, withText text: String) {
        guard let range = range as? IndexedRange, let snapshot = editSnapshot() else { return }
        changeText {
            _ = editReplace(
                snapshot.session,
                start: range.startIndex,
                end: range.endIndex,
                text: text,
                composing: false,
            )
        }
    }

    func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        guard let snapshot = editSnapshot() else { return }
        let text = markedText ?? ""
        let location = selectedRange.location == NSNotFound ? text.utf16.count : selectedRange.location
        let length = selectedRange.length
        changeText {
            _ = bubble_edit_batch_begin(snapshot.session)
            editCompose(snapshot.session, text: text)
            if let current = editSnapshot() {
                let base = current.compositionStart
                let anchor = min(current.compositionEnd, base + location)
                let active = min(current.compositionEnd, anchor + length)
                _ = bubble_edit_select(
                    snapshot.session,
                    UInt32(anchor),
                    UInt32(active),
                )
            }
            _ = bubble_edit_batch_end(snapshot.session)
        }
    }

    func unmarkText() {
        guard let snapshot = editSnapshot(), snapshot.composing else { return }
        changeText {
            _ = bubble_edit_finish_composition(snapshot.session)
        }
    }

    var beginningOfDocument: UITextPosition { IndexedPosition(0) }
    var endOfDocument: UITextPosition { IndexedPosition(focusedFieldText().utf16.count) }

    func textRange(from fromPosition: UITextPosition, to toPosition: UITextPosition) -> UITextRange? {
        guard let start = fromPosition as? IndexedPosition, let end = toPosition as? IndexedPosition else { return nil }
        return IndexedRange(start.index, end.index)
    }

    func position(from position: UITextPosition, offset: Int) -> UITextPosition? {
        guard let position = position as? IndexedPosition else { return nil }
        let next = position.index + offset
        if next < 0 || next > focusedFieldText().utf16.count { return nil }
        return IndexedPosition(next)
    }

    func position(from position: UITextPosition, in direction: UITextLayoutDirection, offset: Int) -> UITextPosition? {
        switch direction {
        case .left, .right:
            let delta = direction == .left ? -offset : offset
            return self.position(from: position, offset: delta)
        default:
            return nil
        }
    }

    func compare(_ position: UITextPosition, to other: UITextPosition) -> ComparisonResult {
        let a = (position as? IndexedPosition)?.index ?? 0
        let b = (other as? IndexedPosition)?.index ?? 0
        if a < b { return .orderedAscending }
        if a > b { return .orderedDescending }
        return .orderedSame
    }

    func offset(from: UITextPosition, to toPosition: UITextPosition) -> Int {
        ((toPosition as? IndexedPosition)?.index ?? 0) - ((from as? IndexedPosition)?.index ?? 0)
    }

    func position(within range: UITextRange, farthestIn direction: UITextLayoutDirection) -> UITextPosition? {
        guard let range = range as? IndexedRange else { return nil }
        switch direction {
        case .left, .up: return IndexedPosition(range.startIndex)
        default: return IndexedPosition(range.endIndex)
        }
    }

    func characterRange(byExtending position: UITextPosition, in direction: UITextLayoutDirection) -> UITextRange? {
        guard let position = position as? IndexedPosition,
              let next = self.position(from: position, in: direction, offset: 1) as? IndexedPosition else { return nil }
        return IndexedRange(position.index, next.index)
    }

    func baseWritingDirection(for position: UITextPosition, in direction: UITextStorageDirection) -> NSWritingDirection {
        _ = position
        _ = direction
        return .leftToRight
    }

    func setBaseWritingDirection(_ writingDirection: NSWritingDirection, for range: UITextRange) {
        _ = writingDirection
        _ = range
    }

    func firstRect(for range: UITextRange) -> CGRect {
        guard let range = range as? IndexedRange else { return .zero }
        if let selected = selectedTextRange as? IndexedRange,
           selected.startIndex == range.startIndex && selected.endIndex == range.endIndex,
           bubble_selection_count() > 0 {
            return selectionRect(0)
        }
        return caretRect(for: IndexedPosition(range.startIndex))
    }

    func caretRect(for position: UITextPosition) -> CGRect {
        guard let position = position as? IndexedPosition else { return .zero }
        var x: Float = 0, y: Float = 0, w: Float = 0, h: Float = 0
        if bubble_caret_at(UInt32(position.index), &x, &y, &w, &h) != 0 { return .zero }
        return viewRect(x, y, w, h)
    }

    func selectionRects(for range: UITextRange) -> [UITextSelectionRect] {
        guard let range = range as? IndexedRange else { return [] }
        let count = Int(bubble_selection_count())
        let selected = selectedTextRange as? IndexedRange
        if let selected, selected.startIndex == range.startIndex && selected.endIndex == range.endIndex && count > 0 {
            return (0..<count).map { index in
                IndexedSelection(selectionRect(UInt32(index)), starts: index == 0, ends: index == count - 1)
            }
        }
        let rect = caretRect(for: IndexedPosition(range.startIndex))
        return [IndexedSelection(rect, starts: true, ends: true)]
    }

    func closestPosition(to point: CGPoint) -> UITextPosition? {
        let local = contentPoint(point)
        var unit: UInt32 = 0
        if bubble_unit_at(Float(local.x), Float(local.y), &unit) != 0 { return nil }
        return IndexedPosition(Int(unit))
    }

    func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? {
        guard let range = range as? IndexedRange, let position = closestPosition(to: point) as? IndexedPosition else { return nil }
        return IndexedPosition(min(range.endIndex, max(range.startIndex, position.index)))
    }

    func characterRange(at point: CGPoint) -> UITextRange? {
        guard let position = closestPosition(to: point) as? IndexedPosition else { return nil }
        let end = min(focusedFieldText().utf16.count, position.index + 1)
        return IndexedRange(position.index, end)
    }

    private func selectionUnits() -> (Int, Int, Int, Int)? {
        guard let snapshot = editSnapshot() else { return nil }
        let markStart = snapshot.hasComposition ? snapshot.compositionStart : 0
        let markEnd = snapshot.hasComposition ? snapshot.compositionEnd : Int(UInt32.max)
        return (snapshot.start, snapshot.end, markStart, markEnd)
    }

    private func focusedFieldText() -> String {
        let handle = bubble_focused()
        if handle == 0 { return "" }
        var buf = [CChar](repeating: 0, count: 65536)
        guard bubble_node_text(handle, &buf, UInt32(buf.count)) == 0 else { return "" }
        return String(decoding: buf.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
    }

    private func utf16Slice(_ text: String, _ start: Int, _ end: Int) -> String {
        let units = text.utf16
        let lower = units.index(units.startIndex, offsetBy: start, limitedBy: units.endIndex) ?? units.endIndex
        let upper = units.index(units.startIndex, offsetBy: end, limitedBy: units.endIndex) ?? units.endIndex
        return String(decoding: units[lower..<upper], as: UTF16.self)
    }

    private func editSnapshot() -> EditSnapshot? {
        var session: UInt64 = 0
        var revision: UInt64 = 0
        var anchor: UInt32 = 0
        var active: UInt32 = 0
        var compositionStart: UInt32 = 0
        var compositionEnd: UInt32 = 0
        var composing: UInt32 = 0
        guard bubble_edit_snapshot(
            &session,
            &revision,
            &anchor,
            &active,
            &compositionStart,
            &compositionEnd,
            &composing,
        ) == 0 else { return nil }
        return EditSnapshot(
            session: session,
            revision: revision,
            anchor: Int(anchor),
            active: Int(active),
            compositionStart: Int(compositionStart),
            compositionEnd: Int(compositionEnd),
            composing: composing != 0,
        )
    }

    private func editCommit(_ session: UInt64, text: String, newCursor: Int32 = 1) {
        let bytes = Array(text.utf8)
        bytes.withUnsafeBufferPointer { buffer in
            _ = bubble_edit_commit(session, buffer.baseAddress, UInt32(bytes.count), newCursor)
        }
    }

    private func editCompose(_ session: UInt64, text: String, newCursor: Int32 = 1) {
        let bytes = Array(text.utf8)
        bytes.withUnsafeBufferPointer { buffer in
            _ = bubble_edit_compose(session, buffer.baseAddress, UInt32(bytes.count), newCursor)
        }
    }

    private func editReplace(
        _ session: UInt64,
        start: Int,
        end: Int,
        text: String,
        composing: Bool,
    ) -> Int32 {
        let bytes = Array(text.utf8)
        return bytes.withUnsafeBufferPointer { buffer in
            bubble_edit_replace(
                session,
                UInt32(start),
                UInt32(end),
                buffer.baseAddress,
                UInt32(bytes.count),
                1,
                composing ? 1 : 0,
            )
        }
    }

    private func changeText(_ body: () -> Void) {
        inputDelegate?.selectionWillChange(self)
        inputDelegate?.textWillChange(self)
        body()
        refresh()
        inputDelegate?.textDidChange(self)
        inputDelegate?.selectionDidChange(self)
    }

    private func contentPoint(_ point: CGPoint) -> CGPoint {
        CGPoint(x: point.x - safeAreaInsets.left, y: point.y - safeAreaInsets.top)
    }

    private func viewRect(_ x: Float, _ y: Float, _ w: Float, _ h: Float) -> CGRect {
        CGRect(
            x: CGFloat(x) + safeAreaInsets.left,
            y: CGFloat(y) + safeAreaInsets.top,
            width: CGFloat(w),
            height: CGFloat(h)
        )
    }

    private func selectionRect(_ index: UInt32) -> CGRect {
        var x: Float = 0, y: Float = 0, w: Float = 0, h: Float = 0
        if bubble_selection_rect(index, &x, &y, &w, &h) != 0 { return .zero }
        return viewRect(x, y, w, h)
    }
}

// Extended nonlinear sRGB: components may exceed 1 for wide-gamut colors.
func colorOf(_ red: CGFloat, _ green: CGFloat, _ blue: CGFloat, _ alpha: CGFloat) -> UIColor {
    UIColor(red: red, green: green, blue: blue, alpha: min(1, max(0, alpha)))
}

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
