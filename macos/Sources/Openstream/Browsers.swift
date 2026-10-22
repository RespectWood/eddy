import AppKit

/// The "real" browsers that ordinary links are forwarded to, and openstream's
/// registration as the system's web link handler ("default web browser").
enum Browsers {
    static let safari = "com.apple.Safari"
    private static let probe = URL(string: "https://example.com")!

    struct Info: Identifiable, Hashable {
        let bundleID: String
        let name: String
        var id: String { bundleID }
    }

    /// Installed apps that can open web links, excluding openstream itself.
    static func available() -> [Info] {
        var seen = Set<String>()
        return NSWorkspace.shared.urlsForApplications(toOpen: probe).compactMap { url in
            guard let bundle = Bundle(url: url), let id = bundle.bundleIdentifier,
                  id != Bundle.main.bundleIdentifier, seen.insert(id).inserted else { return nil }
            let name = FileManager.default.displayName(atPath: url.path).replacingOccurrences(of: ".app", with: "")
            return Info(bundleID: id, name: name)
        }
        .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    /// The current default browser, unless that's openstream.
    static func currentDefault() -> String? {
        guard let url = NSWorkspace.shared.urlForApplication(toOpen: probe),
              let id = Bundle(url: url)?.bundleIdentifier,
              id != Bundle.main.bundleIdentifier else { return nil }
        return id
    }

    static func isOpenstreamDefault() -> Bool {
        guard let url = NSWorkspace.shared.urlForApplication(toOpen: probe) else { return false }
        return Bundle(url: url)?.bundleIdentifier == Bundle.main.bundleIdentifier
    }

    /// Asks macOS to make openstream the web link handler. macOS shows a confirmation dialog.
    static func makeOpenstreamDefault() async throws {
        let app = Bundle.main.bundleURL
        try await NSWorkspace.shared.setDefaultApplication(at: app, toOpenURLsWithScheme: "http")
        try await NSWorkspace.shared.setDefaultApplication(at: app, toOpenURLsWithScheme: "https")
    }

    /// Hands the link to the real browser. Always names the app explicitly: a plain
    /// `NSWorkspace.open(url)` would send the link straight back to openstream.
    static func forward(_ url: URL, to bundleID: String?) {
        let workspace = NSWorkspace.shared
        let candidates = [bundleID, safari].compactMap { $0 }.filter { $0 != Bundle.main.bundleIdentifier }
        guard let appURL = candidates.lazy.compactMap({ workspace.urlForApplication(withBundleIdentifier: $0) }).first else {
            log.error("no browser to forward \(url.absoluteString, privacy: .public) to")
            return
        }
        workspace.open([url], withApplicationAt: appURL, configuration: NSWorkspace.OpenConfiguration())
    }
}
