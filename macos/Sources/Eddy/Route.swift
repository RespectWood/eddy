import Foundation

/// What happens to a clicked URL. Decided without side effects, so it's unit-tested;
/// `AppModel.handle` carries it out.
enum Route: Equatable {
    /// `eddy://auth/tidal?…`: the Connect TIDAL login redirect.
    case tidalLogin
    /// Other `eddy://auth/…` URLs.
    case ignore
    /// Any other `eddy://` URL: debug commands (off unless enabled, see `DebugCommands`).
    case debug
    /// An ordinary link: straight to the browser, no network.
    case forward
    /// Opened as if eddy weren't installed: Spotify links eddy doesn't convert, and every
    /// link while eddy is off. `canonical` is the Spotify web link, when known.
    case passThrough(canonical: String?)
    /// A Spotify track or album (or short link): look it up.
    case convert

    static func decide(_ url: URL, isDisabled: Bool, hasService: Bool) -> Route {
        if url.scheme?.lowercased() == "eddy" {
            if url.host == "auth" { return url.path == "/tidal" ? .tidalLogin : .ignore }
            return .debug
        }
        if url.isFileURL || isDisabled || !hasService { return .passThrough(canonical: nil) }
        // Fast path: sync Rust parse, no network. Ordinary links leave immediately.
        switch parseLink(url: url.absoluteString) {
        case .notMusic:
            return .forward
        case let .spotify(_, _, canonicalUrl, convertible):
            return convertible ? .convert : .passThrough(canonical: canonicalUrl)
        case .spotifyShortLink:
            return .convert
        }
    }
}
