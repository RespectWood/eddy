import AppKit

/// "Connect Tidal": the user logs in on Tidal's own login page (OAuth 2.1 + PKCE),
/// so exact matching works without developer credentials. Tokens live in the Keychain;
/// the Rust core refreshes them through `KeychainTidalTokenStore`.
///
/// The login page opens in the user's own browser (the one ordinary links go to), so an
/// existing Tidal login there is reused. Tidal then redirects to `eddy://auth/tidal`,
/// which the browser hands to Eddy (`handleRedirect`).
@MainActor
final class TidalAccount: ObservableObject {
    enum Status: Equatable {
        /// This build has no Tidal client ID (`EddyTidalClientID` in Info.plist).
        case notConfigured
        case notConnected
        /// The login page is open in the browser.
        case connecting
        case connected(name: String?, country: String?)
        /// The login expired or was revoked.
        case needsReconnect
        case failed(String)
    }

    static let redirectURI = "eddy://auth/tidal"

    @Published private(set) var status: Status = .notConnected
    /// Called when the login changes, so the resolver is rebuilt.
    var onChange: (() -> Void)?

    /// Public by design (PKCE): it's not a secret.
    let clientID: String? = {
        let id = Bundle.main.object(forInfoDictionaryKey: "EddyTidalClientID") as? String
        return id.flatMap { $0.isEmpty || $0.hasPrefix("__") ? nil : $0 }
    }()

    let store = KeychainTidalTokenStore()
    /// The login in progress; only its redirect is accepted (the core checks `state`).
    private var pending: TidalLoginRequest?

    init() {
        store.onClear = { [weak self] in
            Task { @MainActor in
                DebugCommands.log("Tidal login no longer valid; needs reconnect")
                self?.status = .needsReconnect
                self?.onChange?()
            }
        }
        refreshStatus()
    }

    var isConnected: Bool {
        if case .connected = status { return true }
        return false
    }

    /// The account's country, used as the lookup country while connected.
    var country: String? {
        if case let .connected(_, country) = status { return country }
        return nil
    }

    func refreshStatus() {
        guard clientID != nil else {
            status = .notConfigured
            return
        }
        if let tokens = store.load() {
            status = .connected(name: tokens.userName, country: tokens.country)
        } else if status != .needsReconnect && status != .connecting {
            status = .notConnected
        }
    }

    /// Opens Tidal's login page in `browser` (bundle ID).
    func connect(in browser: String?) {
        guard let clientID else { return }
        let request = tidalLoginRequest(clientId: clientID, redirectUri: Self.redirectURI)
        guard let url = URL(string: request.authorizeUrl) else { return }
        pending = request
        status = .connecting
        DebugCommands.log("Tidal login: opening login page in \(browser ?? "default browser")")
        Browsers.forward(url, to: browser)
    }

    func cancelConnect() {
        pending = nil
        refreshStatus()
        if status == .connecting { status = .notConnected }
    }

    func disconnect() {
        store.delete()
        status = .notConnected
        onChange?()
    }

    /// `eddy://auth/tidal?…` from the browser. Ignored unless it answers the login in
    /// progress: any web page can open this URL, and a forged one must not end the login.
    func handleRedirect(_ url: URL) {
        guard let request = pending else {
            DebugCommands.log("Tidal login: redirect ignored (no login in progress)")
            return
        }
        guard tidalCallbackMatches(request: request, callbackUrl: url.absoluteString) else {
            DebugCommands.log("Tidal login: redirect ignored (state doesn't match this login)")
            return
        }
        // Log the outcome, never the code.
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        let error = items.first { $0.name == "error" }?.value
        DebugCommands.log("Tidal login: redirect received (\(error.map { "error \($0)" } ?? "code"))")
        pending = nil
        NSApp.activate(ignoringOtherApps: true)
        Task.detached(priority: .userInitiated) {
            let result = Result { try tidalFinishLogin(request: request, callbackUrl: url.absoluteString) }
            await MainActor.run {
                switch result {
                case let .success(tokens):
                    self.store.save(tokens: tokens)
                    self.status = .connected(name: tokens.userName, country: tokens.country)
                    DebugCommands.log("Tidal login: connected (country \(tokens.country ?? "unknown"), name \(tokens.userName == nil ? "unknown" : "known"))")
                    self.onChange?()
                case let .failure(error):
                    DebugCommands.log("Tidal login failed: \(error)")
                    self.status = .failed("Tidal login failed: \(Self.describe(error))")
                }
            }
        }
    }

    private nonisolated static func describe(_ error: Error) -> String {
        switch error as? TidalLoginError {
        case let .Denied(message): message
        case .StateMismatch: "the response didn't belong to this login. Try again."
        case .MissingCode: "Tidal didn't send a login code."
        case let .Network(message): message
        case nil: error.localizedDescription
        }
    }
}

/// Tidal tokens in the login keychain. Called by the Rust core from background threads.
final class KeychainTidalTokenStore: TidalTokenStore, @unchecked Sendable {
    private static let service = "com.respectwood.eddy.tidal"
    private static let account = "tokens"

    /// Called when the core reports that the login is no longer valid.
    var onClear: (() -> Void)?

    func load() -> TidalTokens? {
        guard let data = Keychain.read(service: Self.service, account: Self.account),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let access = dict["accessToken"] as? String,
              let expiresAt = (dict["expiresAt"] as? NSNumber)?.uint64Value else { return nil }
        return TidalTokens(
            accessToken: access,
            refreshToken: dict["refreshToken"] as? String,
            expiresAt: expiresAt,
            userName: dict["userName"] as? String,
            country: dict["country"] as? String
        )
    }

    func save(tokens: TidalTokens) {
        var dict: [String: Any] = ["accessToken": tokens.accessToken, "expiresAt": tokens.expiresAt]
        dict["refreshToken"] = tokens.refreshToken
        dict["userName"] = tokens.userName
        dict["country"] = tokens.country
        guard let data = try? JSONSerialization.data(withJSONObject: dict) else { return }
        Keychain.write(data, service: Self.service, account: Self.account)
    }

    func clear() {
        delete()
        onClear?()
    }

    func delete() {
        Keychain.delete(service: Self.service, account: Self.account)
    }
}
