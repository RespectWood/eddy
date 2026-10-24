import AppKit
import Foundation
import os

let log = Logger(subsystem: "com.respectwood.eddy", category: "router")

/// App state and the link-routing flow: every clicked URL comes through `handle(_:)`.
@MainActor
final class AppModel: ObservableObject {
    @Published var service: Service? { didSet { defaults.set(service?.key, forKey: Keys.service) } }
    @Published var browserBundleID: String? { didSet { defaults.set(browserBundleID, forKey: Keys.browser) } }
    /// Disabled: Eddy leaves every link alone (Spotify links open as usual).
    @Published var isDisabled: Bool { didSet { defaults.set(isDisabled, forKey: Keys.disabled) } }
    @Published var hasCompletedOnboarding: Bool { didSet { defaults.set(hasCompletedOnboarding, forKey: Keys.onboarded) } }
    @Published var tidalClientID: String { didSet { defaults.set(tidalClientID, forKey: Keys.tidalID); resetResolver() } }
    /// In the Keychain, not UserDefaults: preferences are readable by every process.
    @Published var tidalClientSecret: String { didSet { Self.saveTidalSecret(tidalClientSecret); resetResolver() } }
    @Published var tidalLandingPage: TidalLandingPage { didSet { defaults.set(tidalLandingPage.rawValue, forKey: Keys.tidalLanding) } }
    /// Press play after opening the item. Needs the Accessibility permission.
    @Published var autoPlay: Bool {
        didSet {
            defaults.set(autoPlay, forKey: Keys.autoPlay)
            if autoPlay && !AX.isTrusted { AX.requestTrust() }
        }
    }
    @Published private(set) var recents: [RecentConversion]
    @Published private(set) var isDefaultHandler = false

    private let defaults = UserDefaults.standard
    private var cachedResolver: Resolver?
    /// "Connect Tidal" login (preferred over the developer credentials above).
    let tidalAccount = TidalAccount()

    private enum Keys {
        static let service = "service"
        static let browser = "browserBundleID"
        static let disabled = "disabled"
        static let onboarded = "hasCompletedOnboarding"
        static let tidalID = "tidalClientID"
        /// Only read to move a secret saved by older builds into the Keychain.
        static let legacyTidalSecret = "tidalClientSecret"
        static let recents = "recents"
        static let tidalLanding = "tidalLandingPage"
        static let autoPlay = "autoPlay"
    }

    init() {
        service = defaults.string(forKey: Keys.service).flatMap(Service.init(key:))
        browserBundleID = defaults.string(forKey: Keys.browser)
        isDisabled = defaults.bool(forKey: Keys.disabled)
        hasCompletedOnboarding = defaults.bool(forKey: Keys.onboarded)
        tidalClientID = defaults.string(forKey: Keys.tidalID) ?? ""
        tidalClientSecret = Self.loadTidalSecret()
        tidalLandingPage = defaults.string(forKey: Keys.tidalLanding).flatMap(TidalLandingPage.init(rawValue:)) ?? .album
        autoPlay = defaults.bool(forKey: Keys.autoPlay)
        recents = (defaults.data(forKey: Keys.recents))
            .flatMap { try? JSONDecoder().decode([RecentConversion].self, from: $0) } ?? []
        if browserBundleID == nil {
            browserBundleID = Browsers.currentDefault() ?? Browsers.safari
        }
        DebugCommands.simulateClick = { [weak self] url in self?.handle(url) }
        DebugCommands.copyLink = { [weak self] target in self?.copyLink(to: target) }
        DebugCommands.prefetchShareLink = { [weak self] in self?.prefetchShareLink() }
        tidalAccount.onChange = { [weak self] in
            self?.resetResolver()
            self?.objectWillChange.send()
        }
    }

    // MARK: - Tidal developer secret

    private static let secretService = "com.respectwood.eddy.tidal-dev"
    private static let secretAccount = "clientSecret"

    private static func loadTidalSecret() -> String {
        let defaults = UserDefaults.standard
        if let legacy = defaults.string(forKey: Keys.legacyTidalSecret) {
            // Only drop the plain-text copy once the Keychain has it.
            if legacy.isEmpty || saveTidalSecret(legacy) {
                defaults.removeObject(forKey: Keys.legacyTidalSecret)
            }
            return legacy
        }
        return Keychain.readString(service: secretService, account: secretAccount) ?? ""
    }

    @discardableResult
    private static func saveTidalSecret(_ secret: String) -> Bool {
        if secret.isEmpty {
            Keychain.delete(service: secretService, account: secretAccount)
            return true
        }
        return Keychain.write(Data(secret.utf8), service: secretService, account: secretAccount)
    }

    /// "Show Artist Page Instead of Song": Tidal tracks open on the artist's page.
    var showsTidalArtistPage: Bool {
        get { tidalLandingPage == .artist }
        set { tidalLandingPage = newValue ? .artist : .album }
    }

    /// Autoplay is on but macOS hasn't granted the permission (yet).
    var autoPlayNeedsPermission: Bool { autoPlay && !AX.isTrusted }

    var statusLine: String {
        guard let service else { return "Not set up" }
        if !isDefaultHandler { return "Not active" }
        return isDisabled ? "Disabled: links open as usual" : "Active: Spotify → \(service.displayName)"
    }

    func refreshDefaultHandlerStatus() {
        isDefaultHandler = Browsers.isEddyDefault()
    }

    // MARK: - Routing

    func handle(_ url: URL) {
        if url.scheme?.lowercased() == "eddy" {
            // Login redirect from the browser (`eddy://auth/tidal?code=…`).
            if url.host == "auth" {
                if url.path == "/tidal" { tidalAccount.handleRedirect(url) }
                return
            }
            DebugCommands.handle(url)
            return
        }
        if url.isFileURL || isDisabled || service == nil {
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

    /// Opens a link as if Eddy weren't installed.
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

    /// Counts converted clicks; only the latest one may autoplay.
    private var latestClick = 0

    private func convert(_ url: URL) {
        latestClick += 1
        let click = latestClick
        guard let service else { return }
        let resolver = resolver()
        let country = lookupCountry(for: service == .tidal ? .tidal : .appleMusic)
        // The URL is private: it shows what was played, and `si=` who shared it.
        log.info("resolving a link from \(url.host ?? "?", privacy: .public): \(url.absoluteString, privacy: .private)")
        Task.detached(priority: .userInitiated) {
            let result = Result { try resolver.resolve(url: url.absoluteString, service: service, country: country) }
            await MainActor.run { self.finish(url, service: service, result: result, click: click) }
        }
    }

    private func finish(_ original: URL, service: Service, result: Result<Conversion, Error>, click: Int) {
        switch result {
        case let .success(conversion):
            let (nativeURL, webURL) = landingURLs(for: conversion, service: service)
            let opened = TargetApps.open(nativeURL: nativeURL, webURL: webURL, service: service, browser: browserBundleID)
            remember(RecentConversion(conversion, service: service, original: original, nativeURL: nativeURL, webURL: webURL))
            // Lookups finish in any order (cache hits are instant), so only the most
            // recent click autoplays.
            if opened == .native && autoPlay && click == latestClick
                && !(service == .tidal && tidalLandingPage == .artist) {
                AutoPlay.start(AutoPlay.Target(
                    service: service,
                    kind: conversion.kind == .album ? .album : .track,
                    title: conversion.title,
                    artist: conversion.artist,
                    album: conversion.album,
                    openedURL: nativeURL
                ))
            }
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

    /// The page a conversion opens on: the item itself, or for Tidal tracks optionally
    /// the artist's page.
    private func landingURLs(for conversion: Conversion, service: Service) -> (String, String) {
        if service == .tidal, tidalLandingPage == .artist, conversion.kind == .track,
           let nativeURL = conversion.artistNativeUrl, let webURL = conversion.artistWebUrl {
            return (nativeURL, webURL)
        }
        return (conversion.nativeUrl, conversion.webUrl)
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
            let body = switch reason {
            case .noCredentials: "Connect TIDAL in Settings for exact matches. Showing search results instead."
            case .accountExpired: "Your TIDAL login expired. Reconnect TIDAL in Settings. Showing search results instead."
            default: "Not found on \(service.displayName). Showing search results instead."
            }
            Notifier.post(title: name.isEmpty ? "No exact match" : name, body: body)
        case .AccountExpired:
            // Normally turned into `NotFound(.accountExpired)` with a search; just in case.
            Browsers.forward(original, to: browserBundleID)
            Notifier.post(title: "TIDAL login expired", body: "Reconnect TIDAL in Settings.")
        case let .RateLimited(api):
            Browsers.forward(original, to: browserBundleID)
            Notifier.post(title: "Lookup limit reached", body: "\(api) is rate limiting requests. Opened the original link.")
        case let .Network(message):
            // Network errors include the request URL, i.e. the song.
            log.error("network error: \(message, privacy: .private)")
            Browsers.forward(original, to: browserBundleID)
            // Only the private log gets the details: notifications are kept and shown on screen.
            Notifier.post(title: "Couldn't convert the link", body: "Couldn't reach the music service, so the original link opened instead.")
        }
    }

    // MARK: - Copy Link for Current Song

    /// Where "Copy Link for Current Song" (⇧⌘C) points by default: the user's own
    /// service, so friends on it can open the song. Spotify until a service is chosen.
    var shareTarget: ShareTarget {
        switch service {
        case .tidal: .tidal
        case .appleMusic: .appleMusic
        case nil: .spotify
        }
    }

    /// A lookup started when the menu opened (for `shareTarget`), so the link is usually
    /// ready by the time the user clicks. Nothing runs in the background otherwise.
    private var prefetched: (task: Task<(Player.Reading, SongShare?), Never>, target: ShareTarget, started: Date)?
    /// Older lookups may be for a song that has ended since.
    private static let prefetchMaxAge: TimeInterval = 20

    /// Called when the menu opens. Never prompts for a permission (that waits for the click).
    func prefetchShareLink() {
        let target = shareTarget
        // Menu reopened right away: the running lookup is still current.
        if let prefetched, prefetched.target == target, Date().timeIntervalSince(prefetched.started) < 3 { return }
        let resolver = resolver()
        let preferred = service
        let country = lookupCountry(for: target)
        let task = Task.detached(priority: .utility) {
            Self.lookUpLink(resolver, preferring: preferred, to: target, country: country, mayPrompt: false)
        }
        prefetched = (task, target, Date())
    }

    /// Copies a link to the song playing in Tidal or Music, to share with friends:
    /// on `target`, or on `shareTarget` by default.
    func copyLink(to target: ShareTarget? = nil) {
        let target = target ?? shareTarget
        let resolver = resolver()
        let preferred = service
        let country = lookupCountry(for: target)
        let pending = prefetched.flatMap {
            $0.target == target && Date().timeIntervalSince($0.started) < Self.prefetchMaxAge ? $0.task : nil
        }
        prefetched = nil
        let clicked = Date()
        Task.detached(priority: .userInitiated) {
            var (reading, share) = await pending?.value ?? (.nothingPlaying, nil)
            // Without a usable prefetch (none, or it stopped at a permission), look up now.
            if share == nil {
                (reading, share) = Self.lookUpLink(resolver, preferring: preferred, to: target, country: country, mayPrompt: true)
            }
            guard case let .song(song, source) = reading, let share else {
                let unreadable = reading
                await MainActor.run { self.reportUnreadable(unreadable) }
                return
            }
            DebugCommands.log(String(format: "copy %@ link from %@: %@ %@ (%@, ready %.2fs after the click)",
                                     target.displayName, "\(source)", share.exact ? "exact" : "search", share.url,
                                     pending == nil ? "no prefetch" : "prefetched", Date().timeIntervalSince(clicked)))
            await MainActor.run {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(share.url, forType: .string)
                let name = "\(song.artists.first ?? share.artist) – \(share.title)"
                if share.exact {
                    Notifier.post(title: "Copied \(target.displayName) link", body: name)
                } else {
                    Notifier.post(title: "Copied \(target.displayName) search link",
                                  body: "\(name) wasn't found exactly, so the link searches \(target.displayName) for it.")
                }
            }
        }
    }

    /// Blocking: reads the player, then looks up the link.
    nonisolated private static func lookUpLink(
        _ resolver: Resolver, preferring preferred: Service?, to target: ShareTarget, country: String, mayPrompt: Bool
    ) -> (Player.Reading, SongShare?) {
        let reading = Player.read(preferring: preferred, mayPrompt: mayPrompt)
        guard case let .song(song, _) = reading else { return (reading, nil) }
        return (reading, resolver.shareLink(nowPlaying: song, to: target, country: country))
    }

    private func reportUnreadable(_ reading: Player.Reading) {
        DebugCommands.log("copy link: \(reading)")
        switch reading {
        case .needsAccessibility:
            AX.requestTrust()
            Notifier.post(title: "Can't see what TIDAL is playing", body: "Eddy needs the Accessibility permission to read TIDAL's player.")
        case .tidalWindowNotVisible:
            Notifier.post(title: "Can't see TIDAL's player", body: "TIDAL's window is closed or on another desktop. Show it and try again.")
        case .needsAutomation:
            Notifier.post(title: "Can't see what Music is playing", body: "Allow Eddy to control Music in System Settings → Privacy & Security → Automation.")
        default:
            Notifier.post(title: "Nothing is playing", body: "Play a song in TIDAL or Music first.")
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
        // Read back from disk, so only the link kinds Eddy converts are opened.
        guard let url = URL(string: item.originalURL),
              ["http", "https", "spotify"].contains(url.scheme?.lowercased() ?? "") else { return }
        passThrough(url)
    }

    /// The catalog country for lookups: while Tidal is connected, Tidal lookups use the
    /// account's country; everything else the Mac's region.
    private func lookupCountry(for target: ShareTarget) -> String {
        (target == .tidal ? tidalAccount.country : nil) ?? Locale.current.region?.identifier ?? "US"
    }

    // MARK: - Resolver

    private func resolver() -> Resolver {
        if let cachedResolver { return cachedResolver }
        // The user's login wins; developer credentials are the fallback.
        let made = tidalAccount.isConnected
            ? Resolver(cachePath: Self.cacheURL.path, tidalClientId: tidalAccount.clientID, tidalClientSecret: nil, tidalUserStore: tidalAccount.store)
            : Resolver(
                cachePath: Self.cacheURL.path,
                tidalClientId: tidalClientID.isEmpty ? nil : tidalClientID,
                tidalClientSecret: tidalClientSecret.isEmpty ? nil : tidalClientSecret,
                tidalUserStore: nil
            )
        cachedResolver = made
        return made
    }

    private func resetResolver() {
        cachedResolver = nil
    }

    private static var cacheURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Eddy/cache.sqlite")
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

    init(_ conversion: Conversion, service: Service, original: URL, nativeURL: String, webURL: String) {
        title = conversion.title
        artist = conversion.artist
        serviceKey = service.key
        self.nativeURL = nativeURL
        self.webURL = webURL
        originalURL = original.absoluteString
        date = Date()
    }
}

/// Where Tidal track links land. Tidal links can only navigate, not start playback.
/// Shown as the "Show Artist Page Instead of Song" toggle.
enum TidalLandingPage: String {
    /// The track's album, with the song highlighted.
    case album
    case artist
}

extension ShareTarget: CaseIterable, Identifiable {
    public static var allCases: [ShareTarget] { [.tidal, .appleMusic, .spotify] }
    public var id: String { displayName }

    var displayName: String {
        switch self {
        case .spotify: "Spotify"
        case .appleMusic: "Apple Music"
        case .tidal: "TIDAL"
        }
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
        case .tidal: "TIDAL"
        }
    }
}
