import CoreGraphics
import CoreText
import UIKit

@_silgen_name("bubble_create")
func bubble_create(_ host: UnsafePointer<BubbleHost>?) -> Int32
@_silgen_name("bubble_destroy")
func bubble_destroy()
@_silgen_name("bubble_load")
func bubble_load(_ manifest: UnsafePointer<CChar>?, _ entry: UnsafePointer<UInt8>?, _ entryLen: UInt32) -> Int32
@_silgen_name("bubble_frame")
func bubble_frame(_ width: Float, _ height: Float, _ paint: UnsafeMutablePointer<UnsafePointer<UInt8>?>?, _ paintLen: UnsafeMutablePointer<UInt32>?) -> Int32
@_silgen_name("bubble_pointer_down")
func bubble_pointer_down(_ x: Float, _ y: Float)
@_silgen_name("bubble_scroll")
func bubble_scroll(_ x: Float, _ y: Float, _ dy: Float)
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
@_silgen_name("bubble_faults")
func bubble_faults() -> UInt32
@_silgen_name("bubble_generation")
func bubble_generation() -> UInt32
@_silgen_name("bubble_last_error")
func bubble_last_error() -> UnsafePointer<CChar>?

struct BubbleHost {
    var measure: (@convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, Float, Float, Int32, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?) -> Void)?
    var ctx: UnsafeMutableRawPointer?
}

var hostFamily = "Arial"

let measure: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, Float, Float, Int32, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?) -> Void = { _, text, len, fontSize, maxWidth, hasMax, outW, outH in
    let bytes = text.map { Array(UnsafeBufferPointer(start: $0, count: Int(len))) } ?? []
    let string = String(bytes: bytes, encoding: .utf8) ?? ""
    let size = hostFamily.isEmpty ? UIFont.systemFont(ofSize: CGFloat(fontSize)) : (UIFont(name: hostFamily, size: CGFloat(fontSize)) ?? UIFont.systemFont(ofSize: CGFloat(fontSize)))
    let limit = hasMax != 0 ? CGFloat(maxWidth) : CGFloat.greatestFiniteMagnitude
    let rect = (string as NSString).boundingRect(
        with: CGSize(width: limit, height: .greatestFiniteMagnitude),
        options: [.usesLineFragmentOrigin, .usesFontLeading],
        attributes: [.font: size],
        context: nil
    )
    outW?.pointee = Float(ceil(rect.width))
    outH?.pointee = Float(ceil(rect.height))
}

final class BubbleView: UIView {
    var paint = Data()
    var family = "Arial"

    override init(frame: CGRect) {
        super.init(frame: frame)
        isMultipleTouchEnabled = true
        contentMode = .redraw
        backgroundColor = .black
    }

    required init?(coder: NSCoder) { nil }
    func contentSize() -> CGSize {
        let inset = safeAreaInsets
        return CGSize(width: max(1, bounds.width - inset.left - inset.right), height: max(1, bounds.height - inset.top - inset.bottom))
    }

    func refresh() {
        var ptr: UnsafePointer<UInt8>?
        var len: UInt32 = 0
        let size = contentSize()
        let code = bubble_frame(Float(size.width), Float(size.height), &ptr, &len)
        if code == 0, let ptr, len > 0 {
            paint = Data(bytes: ptr, count: Int(len))
        }
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
        guard let count = u32() else { return }
        for _ in 0..<count {
            guard let tag = paint.indices.contains(cursor) ? paint[cursor] : nil else { return }
            cursor += 1
            switch tag {
            case 1:
                guard let x = f32(), let y = f32(), let w = f32(), let h = f32(), let radius = f32(), let color = u32() else { return }
                colorOf(color).setFill()
                UIBezierPath(roundedRect: CGRect(x: x, y: y, width: w, height: h), cornerRadius: radius).fill()
            case 2:
                guard let x = f32(), let y = f32(), let w = f32(), let font = f32(), let color = u32(), let nbytes = u32() else { return }
                guard let raw = take(Int(nbytes)) else { return }
                let text = String(data: raw, encoding: .utf8) ?? ""
                let uiFont = UIFont(name: family, size: font) ?? UIFont.systemFont(ofSize: font)
                (text as NSString).draw(in: CGRect(x: x, y: y, width: max(w, 1), height: font * 2), withAttributes: [
                    .font: uiFont,
                    .foregroundColor: colorOf(color),
                ])
            case 3:
                guard let x = f32(), let y = f32(), let w = f32(), let h = f32() else { return }
                ctx.saveGState()
                ctx.clip(to: CGRect(x: x, y: y, width: w, height: h))
            case 4:
                ctx.restoreGState()
            default:
                return
            }
        }
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let point = touches.first?.location(in: self) else { return }
        bubble_pointer_down(Float(point.x - safeAreaInsets.left), Float(point.y - safeAreaInsets.top))
    }

    func scroll(dy: CGFloat, at point: CGPoint) {
        bubble_scroll(Float(point.x), Float(point.y), Float(dy))
        refresh()
    }
}

func colorOf(_ rgba: UInt32) -> UIColor {
    let a = CGFloat((rgba >> 24) & 0xff) / 255
    let r = CGFloat((rgba >> 16) & 0xff) / 255
    let g = CGFloat((rgba >> 8) & 0xff) / 255
    let b = CGFloat(rgba & 0xff) / 255
    return UIColor(red: r, green: g, blue: b, alpha: a)
}

func nodeText(_ handle: UInt32) -> String {
    var buf = [CChar](repeating: 0, count: 256)
    guard bubble_node_text(handle, &buf, UInt32(buf.count)) == 0 else { return "" }
    return String(cString: buf)
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
}

final class AppDriver {
    let view = BubbleView()
    var frames = 0
    var frameMs = 0.0
    var last = CACurrentMediaTime()
    var phase = 0
    var hangFrames = 0
    let started = CACurrentMediaTime()
    var link: CADisplayLink?

    func start() {
        var host = BubbleHost(measure: measure, ctx: nil)
        guard bubble_create(&host) == 0 else { return }
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
        guard let manifestURL = Bundle.main.url(forResource: "manifest", withExtension: "json"),
              let entryURL = Bundle.main.url(forResource: "main", withExtension: "js"),
              let manifest = try? String(contentsOf: manifestURL, encoding: .utf8),
              let entry = try? Data(contentsOf: entryURL) else { return }
        let loaded: Int32 = manifest.withCString { manifest in
            entry.withUnsafeBytes { raw in
                bubble_load(manifest, raw.bindMemory(to: UInt8.self).baseAddress, UInt32(entry.count))
            }
        }
        if loaded != 0 { NSLog("bubble load failed") }
        NSLog("bubble bounds %@", NSCoder.string(for: view.bounds))
        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    @objc func tick() {
        let now = CACurrentMediaTime()
        let dt = (now - last) * 1000
        last = now
        view.refresh()
        frames += 1
        if frames > 5 { frameMs += dt }
        if phase == 0 && frames == 20 {
            var x: Float = 0, y: Float = 0, w: Float = 0, h: Float = 0
            _ = bubble_node_box(4, &x, &y, &w, &h)
            bubble_pointer_down(x + w / 2, y + h / 2)
            phase = 1
        } else if phase == 1 && nodeText(3) == "Count: 1" {
            bubble_post_eval("while (true) {}")
            phase = 2
            hangFrames = 0
        } else if phase == 2 {
            hangFrames += 1
            if bubble_faults() > 0 && nodeText(3) == "Count: 0" {
                finish()
                phase = 3
            }
        }
        if phase != 3 && CACurrentMediaTime() - started > 4 {
            finish()
            phase = 3
        }
    }

    func finish() {
        let samples = max(frames - 5, 1)
        let average = frameMs / Double(samples)
        let ok = nodeText(3) == "Count: 0" && bubble_faults() > 0 && hangFrames >= 10 && average < 20 && bubble_font_epoch() > 0
        let text = "ok=\(ok) avg_ms=\(String(format: "%.2f", average)) hang_frames=\(hangFrames) faults=\(bubble_faults()) epoch=\(bubble_font_epoch()) text=\(nodeText(3))\n"
        let url = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("m1-result.txt")
        try? text.data(using: .utf8)?.write(to: url)
        NSLog("bubble %@", text)
    }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))
