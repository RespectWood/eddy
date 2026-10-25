import Foundation
import Testing
@testable import Eddy

/// Any web page can open `eddy://auth/tidal`. A forged redirect must neither end a
/// login in progress nor show text of its choosing (security ticket 0010, finding 4).
@MainActor
struct TidalAccountTests {
    /// Its own Keychain item, never the app's login.
    private func account(open: @escaping (URL, String?) -> Void = { _, _ in }) -> TidalAccount {
        TidalAccount(
            clientID: "test-client",
            store: KeychainTidalTokenStore(service: "com.respectwood.eddy.tests.tidal"),
            openLoginPage: open
        )
    }

    @Test func connectOpensTheLoginPage() {
        var opened: [(URL, String?)] = []
        let account = account { opened.append(($0, $1)) }
        account.connect(in: "org.mozilla.firefox")
        #expect(account.status == .connecting)
        #expect(opened.count == 1)
        #expect(opened.first?.0.host == "login.tidal.com")
        #expect(opened.first?.1 == "org.mozilla.firefox")
    }

    @Test(arguments: [
        "eddy://auth/tidal?error=access_denied&error_description=Call%201-800-SCAM",
        "eddy://auth/tidal?code=attacker-code&state=forged",
        "eddy://auth/tidal?code=attacker-code",
        "eddy://auth/tidal",
    ])
    func forgedRedirectsLeaveTheLoginPending(redirect: String) {
        let account = account()
        account.connect(in: nil)
        let pending = account.pending
        account.handleRedirect(URL(string: redirect)!)
        #expect(account.status == .connecting)
        #expect(account.pending == pending)
    }

    @Test func redirectsWithoutALoginAreIgnored() {
        let account = account()
        let before = account.status
        account.handleRedirect(URL(string: "eddy://auth/tidal?code=x&state=y")!)
        #expect(account.status == before)
        #expect(account.pending == nil)
    }

    @Test func withoutAClientIDThereIsNothingToConnect() {
        var opened = 0
        let account = TidalAccount(
            clientID: nil,
            store: KeychainTidalTokenStore(service: "com.respectwood.eddy.tests.tidal"),
            openLoginPage: { _, _ in opened += 1 }
        )
        #expect(account.status == .notConfigured)
        account.connect(in: nil)
        #expect(opened == 0)
    }
}
