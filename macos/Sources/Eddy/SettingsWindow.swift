import SwiftUI

/// The onboarding / settings window. Shown on first launch, whenever Eddy isn't
/// the link handler, and from the menu.
@MainActor
final class SettingsWindowController {
    private let model: AppModel
    private var window: NSWindow?

    init(model: AppModel) {
        self.model = model
    }

    func show() {
        if window == nil {
            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 520, height: 560),
                styleMask: [.titled, .closable],
                backing: .buffered,
                defer: false
            )
            window.title = "Eddy"
            window.isReleasedWhenClosed = false
            window.contentView = NSHostingView(rootView: SettingsView(model: model) { [weak window] in window?.close() })
            window.center()
            self.window = window
        }
        model.refreshDefaultHandlerStatus()
        // Menu bar apps aren't active by default, so bring the window to the front explicitly.
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }
}

struct SettingsView: View {
    @ObservedObject var model: AppModel
    let close: () -> Void
    @State private var setupError: String?

    private static let privacyURL = URL(string: "https://github.com/RespectWood/eddy#privacy")!

    var body: some View {
        Form {
            Section {
                Text("Spotify links you click open in your own streaming app. Every other link goes straight to your normal browser.")
                    .foregroundStyle(.secondary)
            }

            Section("1. Your streaming service") {
                Picker("Open Spotify links in", selection: $model.service) {
                    Text("Choose…").tag(Service?.none)
                    ForEach(Service.allCases) { service in
                        Text(TargetApps.isInstalled(service) ? service.displayName : "\(service.displayName) (app not installed: web player)")
                            .tag(Optional(service))
                    }
                }
                if model.service == .tidal {
                    Toggle("Show artist page instead of song", isOn: $model.showsTidalArtistPage)
                    Text("Off: Tidal opens the song, highlighted on its album or single. On: the artist's page opens instead (autoplay doesn't work there).")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            if model.service == .tidal {
                Section("Accounts") {
                    TidalAccountRow(account: model.tidalAccount, browser: model.browserBundleID)
                    // Shown before signing in, as TIDAL's developer terms require.
                    Link("What Eddy does with your Tidal login…", destination: Self.privacyURL)
                        .font(.caption)
                    DisclosureGroup("Developer") {
                        Text("Client ID and secret of your own app from developer.tidal.com. Used only while Tidal isn't connected above.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        TextField("Client ID", text: $model.tidalClientID)
                        SecureField("Client secret", text: $model.tidalClientSecret)
                    }
                }
            }

            Section("Autoplay (optional)") {
                Toggle("Start playing automatically", isOn: $model.autoPlay)
                Text("Neither Tidal nor Apple Music can start playback from a link, so Eddy presses the Play button for you once the song's page has opened. This needs the Accessibility permission, and Eddy only uses it for that one button.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                if model.autoPlayNeedsPermission {
                    Button("Grant Accessibility permission…") { AX.requestTrust() }
                }
                if model.service == .tidal && model.tidalLandingPage == .artist {
                    Text("Autoplay doesn't work while \"Show artist page instead of song\" is on.")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
            }

            Section("2. Your normal browser") {
                Picker("Open other links in", selection: $model.browserBundleID) {
                    ForEach(Browsers.available()) { browser in
                        Text(browser.name).tag(Optional(browser.bundleID))
                    }
                }
            }

            Section("3. Let Eddy receive links") {
                Text("macOS sends clicked links to one app, which it calls the \"default web browser\". Eddy takes that slot, but it isn't a browser: it converts Spotify links and immediately passes every other link to the browser above. macOS will ask you to confirm.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                HStack {
                    if model.isDefaultHandler {
                        Label("Eddy receives links", systemImage: "checkmark.circle.fill")
                            .foregroundStyle(.green)
                    } else {
                        Button("Use Eddy for links") { makeDefault() }
                            .disabled(model.service == nil)
                    }
                    Spacer()
                }
                if let setupError {
                    Text(setupError).font(.caption).foregroundStyle(.red)
                }
            }

            Section {
                HStack {
                    Spacer()
                    Button("Done") {
                        if model.service != nil { model.hasCompletedOnboarding = true }
                        close()
                    }
                    .keyboardShortcut(.defaultAction)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 520)
        .fixedSize(horizontal: false, vertical: true)
        .onAppear { Notifier.requestAuthorization() }
    }

    private func makeDefault() {
        setupError = nil
        Task {
            do {
                try await Browsers.makeEddyDefault()
            } catch {
                setupError = "Couldn't change the default browser: \(error.localizedDescription). You can also set it in System Settings → Desktop & Dock → Default web browser."
            }
            model.refreshDefaultHandlerStatus()
        }
    }
}

/// Connect / Connected as … / Disconnect for the user's Tidal login.
struct TidalAccountRow: View {
    @ObservedObject var account: TidalAccount
    /// Where the login page opens: the user's normal browser.
    let browser: String?

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Tidal")
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(detailColor)
            }
            Spacer()
            switch account.status {
            case .notConfigured:
                EmptyView()
            case .connecting:
                ProgressView().controlSize(.small)
                Button("Cancel") { account.cancelConnect() }
            case .connected:
                Button("Disconnect") { account.disconnect() }
            case .needsReconnect:
                Button("Reconnect Tidal") { account.connect(in: browser) }
            case .notConnected, .failed:
                Button("Connect Tidal") { account.connect(in: browser) }
            }
        }
    }

    private var detail: String {
        switch account.status {
        case .notConfigured: "Not available in this build (no Tidal client ID)."
        case .notConnected: "Log in once for exact matches. Without it, Tidal shows search results."
        case .connecting: "Log in to Tidal in your browser. When it asks, allow it to open eddy."
        case let .connected(name, country):
            "Connected" + (name.map { " as \($0)" } ?? "") + (country.map { " · \($0)" } ?? "")
        case .needsReconnect: "Your Tidal login expired. Showing search results until you reconnect."
        case let .failed(message): message
        }
    }

    private var detailColor: Color {
        switch account.status {
        case .needsReconnect, .failed: .orange
        case .connected: .green
        default: .secondary
        }
    }
}
