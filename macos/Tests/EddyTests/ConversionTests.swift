import Foundation
import Testing
@testable import Eddy

struct LandingPageTests {
    private func conversion(kind: SpotifyKind, artistPage: Bool = true) -> Conversion {
        Conversion(
            kind: kind,
            spotifyUrl: "https://open.spotify.com/track/5rb9QrpfcKFHM1EUbSIurX",
            title: "Yeah!",
            artist: "USHER",
            album: "Confessions",
            albumArtist: "Usher",
            nativeUrl: "tidal://track/11337915",
            webUrl: "https://tidal.com/browse/track/11337915",
            artistNativeUrl: artistPage ? "tidal://artist/1518" : nil,
            artistWebUrl: artistPage ? "https://tidal.com/browse/artist/1518" : nil,
            fromCache: false
        )
    }

    @Test func opensTheSongByDefault() {
        let urls = AppModel.landingURLs(for: conversion(kind: .track), service: .tidal, page: .album)
        #expect(urls == ("tidal://track/11337915", "https://tidal.com/browse/track/11337915"))
    }

    /// "Show Artist Page Instead of Song" (manual test E10).
    @Test func opensTheArtistPageForTidalTracks() {
        let urls = AppModel.landingURLs(for: conversion(kind: .track), service: .tidal, page: .artist)
        #expect(urls == ("tidal://artist/1518", "https://tidal.com/browse/artist/1518"))
    }

    @Test func artistPageOnlyAppliesWhereItExists() {
        let album = AppModel.landingURLs(for: conversion(kind: .album), service: .tidal, page: .artist)
        #expect(album.0 == "tidal://track/11337915")
        let noArtist = AppModel.landingURLs(for: conversion(kind: .track, artistPage: false), service: .tidal, page: .artist)
        #expect(noArtist.0 == "tidal://track/11337915")
        let appleMusic = AppModel.landingURLs(for: conversion(kind: .track), service: .appleMusic, page: .artist)
        #expect(appleMusic.0 == "tidal://track/11337915")
    }
}

struct RecentConversionTests {
    /// Recents as saved by 0.1.0-beta.1 must still load after an update.
    @Test func decodesRecentsSavedByBeta1() throws {
        let saved = """
        [{"id":"6F9619FF-8B86-D011-B42D-00C04FC964FF","title":"Yeah!","artist":"USHER",
          "serviceKey":"tidal","nativeURL":"tidal://track/11337915",
          "webURL":"https://tidal.com/browse/track/11337915",
          "originalURL":"https://open.spotify.com/track/5rb9QrpfcKFHM1EUbSIurX","date":783000000}]
        """
        let recents = try JSONDecoder().decode([RecentConversion].self, from: Data(saved.utf8))
        #expect(recents.count == 1)
        #expect(recents[0].service == .tidal)
        #expect(recents[0].nativeURL == "tidal://track/11337915")
    }

    @Test func roundTrips() throws {
        let item = RecentConversion(
            Conversion(
                kind: .track, spotifyUrl: "", title: "Never Gonna Give You Up", artist: "Rick Astley",
                album: nil, albumArtist: nil, nativeUrl: "", webUrl: "", artistNativeUrl: nil,
                artistWebUrl: nil, fromCache: false
            ),
            service: .appleMusic,
            original: URL(string: "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC")!,
            nativeURL: "music://music.apple.com/de/album/x/1?i=2",
            webURL: "https://music.apple.com/de/album/x/1?i=2"
        )
        let decoded = try JSONDecoder().decode(RecentConversion.self, from: JSONEncoder().encode(item))
        #expect(decoded == item)
    }

    /// A service key from a newer or older build falls back instead of failing.
    @Test func unknownServiceFallsBack() throws {
        let saved = """
        {"id":"6F9619FF-8B86-D011-B42D-00C04FC964FF","title":"x","artist":"y","serviceKey":"deezer",
         "nativeURL":"","webURL":"","originalURL":"","date":0}
        """
        let item = try JSONDecoder().decode(RecentConversion.self, from: Data(saved.utf8))
        #expect(item.service == .appleMusic)
    }
}

struct ServiceTests {
    @Test func keysRoundTrip() {
        for service in Service.allCases {
            #expect(Service(key: service.key) == service)
        }
        #expect(Service(key: "spotify") == nil)
    }

    /// The Copy Link submenu order (manual test O11).
    @Test func shareTargetOrder() {
        #expect(ShareTarget.allCases == [.tidal, .appleMusic, .spotify])
        #expect(ShareTarget.allCases.map(\.displayName) == ["TIDAL", "Apple Music", "Spotify"])
    }
}

/// Calls through the generated bindings into the Rust library, so a mismatch between
/// the Swift bindings and the linked library shows up here.
struct BridgeTests {
    @Test func parsesThroughRust() {
        #expect(isMusicLink(url: "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC"))
        #expect(!isMusicLink(url: "https://example.com"))
        guard case let .spotify(kind, id, canonicalUrl, convertible) =
            parseLink(url: "https://open.spotify.com/intl-de/album/4LH4d3cOWNNsVw41Gqt2kv?si=x")
        else {
            Issue.record("not parsed as a Spotify link")
            return
        }
        #expect(kind == .album)
        #expect(id == "4LH4d3cOWNNsVw41Gqt2kv")
        #expect(canonicalUrl == "https://open.spotify.com/album/4LH4d3cOWNNsVw41Gqt2kv")
        #expect(convertible)
    }

    /// No network: unsupported links fail before any lookup.
    @Test func resolverErrorsCrossTheBridge() {
        let resolver = Resolver(cachePath: "", tidalClientId: nil, tidalClientSecret: nil, tidalUserStore: nil)
        #expect(throws: ResolveError.self) {
            try resolver.resolve(url: "https://example.com", service: .tidal, country: "DE")
        }
        do {
            _ = try resolver.resolve(url: "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M", service: .tidal, country: "DE")
            Issue.record("expected an error")
        } catch let ResolveError.Unsupported(kind) {
            #expect(kind == .playlist)
        } catch {
            Issue.record("unexpected \(error)")
        }
    }
}
