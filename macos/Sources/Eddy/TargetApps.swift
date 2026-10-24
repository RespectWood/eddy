import AppKit

/// Opens converted items: in the native desktop app whenever it's installed,
/// in the web player only when it isn't.
enum TargetApps {
    static let spotify = "com.spotify.client"

    enum Outcome { case native, web, failed }

    static func bundleID(for service: Service) -> String {
        switch service {
        case .appleMusic: "com.apple.Music"
        case .tidal: "com.tidal.desktop"
        }
    }

    static func appURL(for service: Service) -> URL? {
        NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID(for: service))
    }

    static func isInstalled(_ service: Service) -> Bool {
        appURL(for: service) != nil
    }

    /// The only scheme each app's links may have. The URLs come from lookups, the cache
    /// and Recents (stored on disk), so they're checked before anything is opened.
    static func nativeScheme(for service: Service) -> String {
        switch service {
        case .appleMusic: "music"
        case .tidal: "tidal"
        }
    }

    @discardableResult
    static func open(nativeURL: String, webURL: String, service: Service, browser: String?) -> Outcome {
        guard let appURL = appURL(for: service) else {
            guard let web = URL(string: webURL), web.scheme?.lowercased() == "https" else { return .failed }
            Browsers.forward(web, to: browser)
            return .web
        }
        guard let native = URL(string: nativeURL),
              native.scheme?.lowercased() == nativeScheme(for: service) else { return .failed }
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = true
        // Send the link exactly once, also on a cold start: Tidal queues a link that
        // launched it and opens it when its window is ready. A second copy sent during
        // startup froze Tidal.
        NSWorkspace.shared.open([native], withApplicationAt: appURL, configuration: configuration) { _, error in
            if let error { log.error("opening \(nativeURL, privacy: .private) failed: \(error.localizedDescription, privacy: .public)") }
        }
        return .native
    }
}
