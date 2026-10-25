import Foundation
import Testing
@testable import Eddy

/// Every clicked link goes through `Route.decide`. The most important rule: ordinary
/// links always go straight to the browser (manual tests C1–C5, F1–F3).
struct RouteTests {
    private func route(_ string: String, disabled: Bool = false, service: Bool = true) -> Route {
        Route.decide(URL(string: string)!, isDisabled: disabled, hasService: service)
    }

    @Test(arguments: [
        "https://example.com",
        "http://example.com/page?q=1#top",
        "https://www.spotify.com/de/premium/",
        "https://open.spotify.com/",
        "https://open.spotify.com.example.org/track/4uLU6hMCjMI75M1A2tKUQC",
        "https://music.apple.com/de/album/x/1559523357?i=1559523359",
        "https://tidal.com/browse/track/1234567",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "mailto:someone@example.com",
    ])
    func ordinaryLinksGoToTheBrowser(link: String) {
        #expect(route(link) == .forward)
    }

    @Test(arguments: [
        "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC?si=abc",
        "https://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC",
        "https://open.spotify.com/album/4LH4d3cOWNNsVw41Gqt2kv",
        "spotify:track:4uLU6hMCjMI75M1A2tKUQC",
        "https://spotify.link/b8ZkKpRqRDb",
        "https://spoti.fi/3Hx9kQd",
    ])
    func tracksAlbumsAndShortLinksAreConverted(link: String) {
        #expect(route(link) == .convert)
    }

    @Test func otherSpotifyLinksOpenAsUsual() {
        #expect(route("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=1")
            == .passThrough(canonical: "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M"))
        #expect(route("https://open.spotify.com/episode/512ojhOuo1ktJprKbVcKyQ")
            == .passThrough(canonical: "https://open.spotify.com/episode/512ojhOuo1ktJprKbVcKyQ"))
        #expect(route("https://open.spotify.com/artist/0gxyHStUsqpMadRV0Di1Qt")
            == .passThrough(canonical: "https://open.spotify.com/artist/0gxyHStUsqpMadRV0Di1Qt"))
        #expect(route("spotify:show:2MAi0BvDc6GTFvKFPXnkCL")
            == .passThrough(canonical: "https://open.spotify.com/show/2MAi0BvDc6GTFvKFPXnkCL"))
    }

    /// "Disable eddy", and before setup: nothing is converted (manual test H1).
    @Test func disabledOrNotSetUpChangesNothing() {
        let track = "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC"
        #expect(route(track, disabled: true) == .passThrough(canonical: nil))
        #expect(route(track, service: false) == .passThrough(canonical: nil))
        #expect(route("https://example.com", disabled: true) == .passThrough(canonical: nil))
    }

    /// A local `.html` file opened from Finder (manual test C4).
    @Test func fileURLsOpenAsUsual() {
        #expect(Route.decide(URL(fileURLWithPath: "/tmp/page.html"), isDisabled: false, hasService: true)
            == .passThrough(canonical: nil))
    }

    @Test func eddyURLs() {
        #expect(route("eddy://auth/tidal?code=x&state=y") == .tidalLogin)
        #expect(route("EDDY://auth/tidal?code=x&state=y") == .tidalLogin)
        #expect(route("eddy://auth/other") == .ignore)
        #expect(route("eddy://debug/share") == .debug)
        // Even while disabled: the login redirect must still arrive.
        #expect(route("eddy://auth/tidal?code=x", disabled: true) == .tidalLogin)
    }
}

struct BrowsersTests {
    private let eddy = "com.respectwood.eddy"
    private let firefox = "org.mozilla.firefox"

    @Test func forwardsToTheChosenBrowserThenSafari() {
        #expect(Browsers.candidates(preferred: firefox, own: eddy) == [firefox, Browsers.safari])
        #expect(Browsers.candidates(preferred: nil, own: eddy) == [Browsers.safari])
        #expect(Browsers.candidates(preferred: Browsers.safari, own: eddy) == [Browsers.safari])
    }

    /// Forwarding to eddy would send the link straight back: an endless loop.
    @Test func neverForwardsToEddyItself() {
        #expect(Browsers.candidates(preferred: eddy, own: eddy) == [Browsers.safari])
    }
}
