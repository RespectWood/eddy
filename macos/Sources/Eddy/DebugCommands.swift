import AppKit

/// Developer commands, sent as `eddy://debug/…?token=<token>` URLs (`scripts/debug.sh`
/// adds the token and calls `open`). Off unless a token is set with
/// `defaults write com.respectwood.eddy debugToken <random>`, and only accepted with
/// that token: Eddy is the default browser, so any web page can open `eddy://` URLs,
/// but no web page can read the user's defaults.
///
///   eddy://debug/dump?app=tidal|music     → debug/ax-<app>.txt (the app's UI tree)
///   eddy://debug/autoplay?app=tidal|music&title=…&artist=…&album=…&kind=track|album&url=<opened url>
///   eddy://debug/click?url=<spotify link>  → handled exactly like a clicked link
///   eddy://debug/share[?to=spotify|music|tidal] → "Copy Link for Current Song" (default target without `to`)
///   eddy://debug/prefetch                  → what expanding the Copy Link submenu does (look up ahead)
///
/// Output and logs go to ~/Library/Application Support/eddy/debug/.
enum DebugCommands {
    /// Shorter tokens are ignored, so a guessable one can't enable the commands.
    private static let minTokenLength = 16

    private static var token: String? {
        UserDefaults.standard.string(forKey: "debugToken").flatMap { $0.count >= minTokenLength ? $0 : nil }
    }

    static var isEnabled: Bool { token != nil }

    /// Set by `AppModel`: routes a URL as if the user had clicked it.
    @MainActor static var simulateClick: ((URL) -> Void)?
    /// Set by `AppModel`: "Copy Link for Current Song" (`nil`: the default target).
    @MainActor static var copyLink: ((ShareTarget?) -> Void)?
    /// Set by `AppModel`: the lookup that starts when the menu opens.
    @MainActor static var prefetchShareLink: (() -> Void)?

    static var directory: URL {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("eddy/debug")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Appends to debug/autoplay.log when debug commands are enabled.
    static func log(_ message: String) {
        guard isEnabled else { return }
        // Messages hold song titles and URLs: line breaks are escaped, so one can't
        // fake another log entry.
        let flat = message.replacingOccurrences(of: "\r", with: "\\r").replacingOccurrences(of: "\n", with: "\\n")
        let line = "\(ISO8601DateFormatter().string(from: Date())) \(flat)\n"
        let url = directory.appendingPathComponent("autoplay.log")
        if let handle = try? FileHandle(forWritingTo: url) {
            handle.seekToEndOfFile()
            handle.write(Data(line.utf8))
            try? handle.close()
        } else {
            try? Data(line.utf8).write(to: url)
        }
    }

    @MainActor static func handle(_ url: URL) {
        let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        let param = { (name: String) in query.first { $0.name == name }?.value }
        // Without the right token, nothing happens and nothing is logged: the URL may
        // come from a web page.
        guard let token, let given = param("token"), constantTimeEqual(given, token) else { return }
        if url.path == "/click" {
            guard let link = param("url").flatMap(URL.init(string:)) else { return }
            log("simulated click: \(link)")
            simulateClick?(link)
            return
        }
        if url.path == "/prefetch" {
            log("simulated menu open (prefetch)")
            prefetchShareLink?()
            return
        }
        if url.path == "/share" {
            let target: ShareTarget? = switch param("to")?.lowercased() {
            case "spotify": .spotify
            case "tidal": .tidal
            case "music", "apple-music": .appleMusic
            default: nil
            }
            log("simulated Copy Link (\(target.map { "\($0)" } ?? "default"))")
            copyLink?(target)
            return
        }
        guard let service = param("app").flatMap(service(named:)) else {
            log("unknown app in \(url)")
            return
        }
        switch url.path {
        case "/dump":
            DispatchQueue.global(qos: .userInitiated).async { dump(service) }
        case "/autoplay":
            let target = AutoPlay.Target(
                service: service,
                kind: param("kind") == "album" ? .album : .track,
                title: param("title") ?? "",
                artist: param("artist") ?? "",
                album: param("album"),
                openedURL: param("url") ?? ""
            )
            AutoPlay.start(target)
        default:
            log("unknown command \(url.path)")
        }
    }

    /// Takes as long whether the tokens differ early or late.
    private static func constantTimeEqual(_ a: String, _ b: String) -> Bool {
        let (x, y) = (Array(a.utf8), Array(b.utf8))
        guard x.count == y.count else { return false }
        return zip(x, y).reduce(0) { $0 | ($1.0 ^ $1.1) } == 0
    }

    private static func service(named name: String) -> Service? {
        switch name.lowercased() {
        case "tidal": .tidal
        case "music", "apple-music", "applemusic": .appleMusic
        default: nil
        }
    }

    private static func dump(_ service: Service) {
        let name = service == .tidal ? "tidal" : "music"
        let file = directory.appendingPathComponent("ax-\(name).txt")
        guard AX.isTrusted else {
            try? "eddy has no Accessibility permission\n".write(to: file, atomically: true, encoding: .utf8)
            return
        }
        guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: TargetApps.bundleID(for: service)).first else {
            try? "\(name) isn't running\n".write(to: file, atomically: true, encoding: .utf8)
            return
        }
        let root = AX.application(pid: app.processIdentifier)
        log("enable web accessibility: \(AX.enableWebAccessibility(root))")
        // Chromium builds the web content's tree asynchronously after the first request.
        Thread.sleep(forTimeInterval: 3)
        // Only the windows: the menu bar includes the system's Recent Items.
        let windows: [AXUIElement] = AX.attribute(root, kAXWindowsAttribute) ?? []
        var text = windows.map { AX.dump($0) }.joined(separator: "\n")
        // Chromium may only expose web content through hit testing: probe a few points
        // and dump the highest web area found.
        if let window = windows.first, let frame = AX.frame(window) {
            text += "\n\n=== hit tests ===\n"
            var webArea: AXUIElement?
            for (fx, fy) in [(0.5, 0.5), (0.3, 0.3), (0.7, 0.4), (0.5, 0.9)] {
                var hit: AXUIElement?
                let point = (Float(frame.minX + frame.width * fx), Float(frame.minY + frame.height * fy))
                let err = AXUIElementCopyElementAtPosition(root, point.0, point.1, &hit)
                text += "at \(point): error \(err.rawValue)\n"
                var current = hit
                var depth = 0
                while let element = current, depth < 40 {
                    let node = AX.Node(element, depth: 1)
                    text += node.line + "\n"
                    if node.role == "AXWebArea" { webArea = element }
                    current = AX.attribute(element, kAXParentAttribute)
                    depth += 1
                }
            }
            if let webArea {
                text += "\n=== web area ===\n" + AX.dump(webArea)
            }
        }
        try? text.write(to: file, atomically: true, encoding: .utf8)
        log("dumped \(name): \(text.split(separator: "\n").count) lines → \(file.path)")
    }
}
