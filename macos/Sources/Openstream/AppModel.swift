import AppKit
import Foundation
import os

let log = Logger(subsystem: "app.openstream.mac", category: "router")

/// App state and the link-routing flow: every clicked URL comes through `handle(_:)`.
@MainActor
final class AppModel: ObservableObject {
    @Published var service: Service? { didSet { defaults.set(service?.key, forKey: Keys.service) } }
    @Published var browserBundleID: String? { didSet { defaults.set(browserBundleID, forKey: Keys.browser) } }
    @Published var isPaused: Bool { didSet { defaults.set(isPaused, forKey: Keys.paused) } }
    @Published var hasCompletedOnboarding: Bool { didSet { defaults.set(hasCompletedOnboarding, forKey: Keys.onboarded) } }
    @Published var tidalClientID: String { didSet { defaults.set(tidalClientID, forKey: Keys.tidalID); resetResolver() } }
    @Published var tidalClientSecret: String { didSet { defaults.set(tidalClientSecret, forKey: Keys.tidalSecret); resetResolver() } }
    @Published private(set) var recents: [RecentConversion]
    @Published private(set) var isDefaultHandler = false

    private let defaults = UserDefaults.standard
    private var cachedResolver: Resolver?

    private enum Keys {
        static let service = "service"
        static let browser = "browserBundleID"
        static let paused = "paused"
        static let onboarded = "hasCompletedOnboarding"
        static let tidalID = "tidalClientID"
        static let tidalSecret = "tidalClientSecret"
        static let recents = "recents"
    }

    init() {
        service = defaults.string(forKey: Keys.service).flatMap(Service.init(key:))
        browserBundleID = defaults.string(forKey: Keys.browser)
        isPaused = defaults.bool(forKey: Keys.paused)
        hasCompletedOnboarding = defaults.bool(forKey: Keys.onboarded)
        tidalClientID = defaults.string(forKey: Keys.tidalID) ?? ""
        tidalClientSecret = defaults.string(forKey: Keys.tidalSecret) ?? ""
        recents = (defaults.data(forKey: Keys.recents))
            .flatMap { try? JSONDecoder().decode([RecentConversion].self, from: $0) } ?? []
        if browserBundleID == nil {
            browserBundleID = Browsers.currentDefault() ?? Browsers.safari
        }
    }

    var statusLine: String {
        guard let service else { return "Not set up" }
        if !isDefaultHandler { return "Not active" }
        return isPaused ? "Paused" : "Active: Spotify → \(service.displayName)"
    }

    func refreshDefaultHandlerStatus() {
        isDefaultHandler = Browsers.isOpenstreamDefault()
    }

    // MARK: - Routing

    func handle(_ url: URL) {
        if url.isFileURL || isPaused || service == nil {
            passThrough(url)
            return
        }
        // Fast path: sync Rust parse, no network. Ordinary links leave immediately.
        switch parseLink(url: url.absoluteString) {
        case .notMusic:
            Browsers.forward(url, to: browserBundleID)
        case let .spotify(_, _, canonicalUrl, convertible):
            if convertible { convert(url) } else { passThrough(url, canonical: canonicalUrl) }
        case .spotifyShortLink:
            convert(url)
        }
    }

    /// Opens a link as if openstream weren't installed.
    private func passThrough(_ url: URL, canonical: String? = nil) {
        guard url.scheme?.lowercased() == "spotify" else {
            Browsers.forward(url, to: browserBundleID)
            return
        }
        // `spotify:` URIs: the Spotify app if installed, otherwise the web page.
        if let spotify = NSWorkspace.shared.urlForApplication(withBundleIdentifier: TargetApps.spotify) {
            NSWorkspace.shared.open([url], withApplicationAt: spotify, configuration: NSWorkspace.OpenConfiguration())
        } else if let web = canonical.flatMap(URL.init(string:)) ?? webURL(forSpotifyURI: url) {
            Browsers.forward(web, to: browserBundleID)
        }
    }

    private func webURL(forSpotifyURI url: URL) -> URL? {
        if case let .spotify(_, _, canonicalUrl, _) = parseLink(url: url.absoluteString) {
            return URL(string: canonicalUrl)
        }
        return nil
    }

    private func convert(_ url: URL) {
        guard let service else { return }
        let resolver = resolver()
        let country = Locale.current.region?.identifier ?? "US"
        log.info("resolving \(url.absoluteString, privacy: .public)")
        Task.detached(priority: .userInitiated) {
            let result = Result { try resolver.resolve(url: url.absoluteString, service: service, country: country) }
            await MainActor.run { self.finish(url, service: service, result: result) }
        }
    }

    private func finish(_ original: URL, service: Service, result: Result<Conversion, Error>) {
        switch result {
        case let .success(conversion):
            let opened = TargetApps.open(nativeURL: conversion.nativeUrl, webURL: conversion.webUrl, service: service, browser: browserBundleID)
            remember(RecentConversion(conversion, service: service, original: original))
            if opened == .web {
                Notifier.post(
                    title: "Opened \(conversion.title) in the browser",
                    body: "\(service.displayName) isn't installed, so the web player was used."
                )
            }
        case let .failure(error as ResolveError):
            handleFailure(error, original: original, service: service)
        case let .failure(error):
            log.error("resolve failed: \(error.localizedDescription, privacy: .public)")
            Browsers.forward(original, to: browserBundleID)
        }
    }

    private func handleFailure(_ error: ResolveError, original: URL, service: Service) {
        switch error {
        case .Unsupported, .NotSpotify:
            passThrough(original)
        case let .NotFound(reason, title, artist, searchNativeUrl, searchWebUrl):
            let name = [artist, title].compactMap { $0 }.joined(separator: " – ")
            guard let searchNativeUrl, let searchWebUrl else {
                Browsers.forward(original, to: browserBundleID)
                Notifier.post(title: "Couldn't read this Spotify link", body: "Opened the original link instead.")
                return
            }
            TargetApps.open(nativeURL: searchNativeUrl, webURL: searchWebUrl, service: service, browser: browserBundleID)
            let body = reason == .noCredentials
                ? "Exact Tidal matching needs Tidal API credentials (Settings). Showing search results instead."
                : "Not found on \(service.displayName). Showing search results instead."
            Notifier.post(title: name.isEmpty ? "No exact match" : name, body: body)
        case let .RateLimited(api):
            Browsers.forward(original, to: browserBundleID)
            Notifier.post(title: "Lookup limit reached", body: "\(api) is rate limiting requests. Opened the original link.")
        case let .Network(message):
            log.error("network error: \(message, privacy: .public)")
            Browsers.forward(original, to: browserBundleID)
            Notifier.post(title: "Couldn't convert the link", body: "Opened the original link instead. (\(message))")
        }
    }

    // MARK: - Recents

    private func remember(_ item: RecentConversion) {
        recents = Array(([item] + recents.filter { $0.originalURL != item.originalURL }).prefix(10))
        defaults.set(try? JSONEncoder().encode(recents), forKey: Keys.recents)
    }

    func reopen(_ item: RecentConversion) {
        TargetApps.open(nativeURL: item.nativeURL, webURL: item.webURL, service: item.service, browser: browserBundleID)
    }

    func openOriginal(_ item: RecentConversion) {
        guard let url = URL(string: item.originalURL) else { return }
        passThrough(url)
    }

    // MARK: - Resolver

    private func resolver() -> Resolver {
        if let cachedResolver { return cachedResolver }
        let made = Resolver(
            cachePath: Self.cacheURL.path,
            tidalClientId: tidalClientID.isEmpty ? nil : tidalClientID,
            tidalClientSecret: tidalClientSecret.isEmpty ? nil : tidalClientSecret
        )
        cachedResolver = made
        return made
    }

    private func resetResolver() {
        cachedResolver = nil
    }

    private static var cacheURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("openstream/cache.sqlite")
    }
}

struct RecentConversion: Codable, Identifiable, Equatable {
    var id = UUID()
    let title: String
    let artist: String
    let serviceKey: String
    let nativeURL: String
    let webURL: String
    let originalURL: String
    let date: Date

    var service: Service { Service(key: serviceKey) ?? .appleMusic }

    init(_ conversion: Conversion, service: Service, original: URL) {
        title = conversion.title
        artist = conversion.artist
        serviceKey = service.key
        nativeURL = conversion.nativeUrl
        webURL = conversion.webUrl
        originalURL = original.absoluteString
        date = Date()
    }
}

extension Service: CaseIterable, Identifiable {
    public static var allCases: [Service] { [.appleMusic, .tidal] }
    public var id: String { key }

    var key: String {
        switch self {
        case .appleMusic: "apple-music"
        case .tidal: "tidal"
        }
    }

    init?(key: String) {
        guard let match = Service.allCases.first(where: { $0.key == key }) else { return nil }
        self = match
    }

    var displayName: String {
        switch self {
        case .appleMusic: "Apple Music"
        case .tidal: "Tidal"
        }
    }
}
