import CoreGraphics
import CoreText
import UIKit

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
    let view = BubbleView()
    var link: CADisplayLink?
    var engineReady = false
    var devURL: URL?
    var devStamp = ""
    var devBusy = false
    var lastDev = 0.0
    var appVisible = true

    var fixtureName: String {
        let requested = ProcessInfo.processInfo.environment["BUBBLE_FIXTURE"] ?? "counter"
        return requested.range(of: "^[a-z0-9-]+$", options: .regularExpression) == nil
            ? "counter"
            : requested
    }

    func setVisible(_ next: Bool) {
        guard next != appVisible else { return }
        appVisible = next
        if !next {
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
        view.refresh()
    }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))
