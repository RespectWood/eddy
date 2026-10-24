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
            window.title = "eddy"
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
                    Text("Off: TIDAL opens the song, highlighted on its album or single. On: the artist's page opens instead (autoplay doesn't work there).")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            if model.service == .tidal {
                Section("Accounts") {
                    TidalAccountRow(account: model.tidalAccount, browser: model.browserBundleID)
                    // Shown before signing in, as TIDAL's developer terms require.
                    Link(destination: Self.privacyURL) {
                        HStack {
                            Text("Privacy Policy")
                            Spacer()
                            Image(systemName: "arrow.up.right")
                                .foregroundStyle(.secondary)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    DisclosureGroup("Developer") {
                        Text("Client ID and secret of your own app from developer.tidal.com. Used only while TIDAL isn't connected above.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        TextField("Client ID", text: $model.tidalClientID)
                        SecureField("Client secret", text: $model.tidalClientSecret)
                    }
                }
            }

            Section("Autoplay (optional)") {
                Toggle("Start playing automatically", isOn: $model.autoPlay)
                Text("Neither TIDAL nor Apple Music can start playback from a link, so eddy presses the Play button for you once the song's page has opened. This needs the Accessibility permission, and eddy only uses it for that one button.")
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

            Section("3. Let eddy receive links") {
                Text("macOS sends clicked links to one app, which it calls the \"default web browser\". eddy takes that slot, but it isn't a browser: it converts Spotify links and immediately passes every other link to the browser above. macOS will ask you to confirm.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                HStack {
                    if model.isDefaultHandler {
                        Label("eddy receives links", systemImage: "checkmark.circle.fill")
                            .foregroundStyle(.green)
                    } else {
                        Button("Use eddy for links") { makeDefault() }
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

/// Connect / Disconnect for the user's TIDAL login.
struct TidalAccountRow: View {
    @ObservedObject var account: TidalAccount
    /// Where the login page opens: the user's normal browser.
    let browser: String?

    /// One line like an account row in System Settings: logo and action (Disconnect
    /// already says it's connected). A note goes below only when there's something to
    /// explain.
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                TidalBadge()
                Spacer()
                if account.status == .connecting {
                    ProgressView().controlSize(.small)
                }
                action
            }
            if let note {
                Text(note)
                    .font(.caption)
                    .foregroundStyle(noteColor)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    @ViewBuilder private var action: some View {
        switch account.status {
        case .notConfigured:
            EmptyView()
        case .connecting:
            Button("Cancel") { account.cancelConnect() }
        case .connected:
            Button("Disconnect") { account.disconnect() }
        case .needsReconnect:
            Button("Reconnect") { account.connect(in: browser) }
        case .notConnected, .failed:
            Button("Connect") { account.connect(in: browser) }
        }
    }

    private var note: String? {
        switch account.status {
        case .connected: nil
        case .notConfigured: "Not available in this build (no TIDAL client ID)."
        case .notConnected: "Log in once for exact matches. Without it, TIDAL shows search results."
        case .connecting: "Log in to TIDAL in your browser. When it asks, allow it to open eddy."
        case .needsReconnect: "Your TIDAL login expired. Showing search results until you reconnect."
        case let .failed(message): message
        }
    }

    private var noteColor: Color {
        switch account.status {
        case .needsReconnect, .failed: .orange
        default: .secondary
        }
    }
}

/// TIDAL's logo, as its Design Guidelines require wherever TIDAL is used: the official
/// horizontal logo, unaltered, in black (light mode) or white (dark mode) on the row's
/// own background, with its clear space, linking back to TIDAL. Only shown next to TIDAL,
/// never next to other services.
struct TidalBadge: View {
    @Environment(\.colorScheme) private var colorScheme

    /// `tidal-logo.png`: the logo from TIDAL's Design Guidelines, white on black, with its
    /// clear space around it (652 × 196 px; the logo starts 64 px in from each edge).
    private static let pixelSize = CGSize(width: 652, height: 196)
    private static let clearSpace: CGFloat = 64
    /// The whole image, clear space included; the logo itself is then ~14 pt tall
    /// (minimum 13 px).
    private static let height: CGFloat = 40

    var body: some View {
        Link(destination: URL(string: "https://tidal.com")!) {
            if let logo = NSImage(named: "tidal-logo") {
                let scale = Self.height / Self.pixelSize.height
                // The logo's shape (its white pixels) in plain black or white, so no box
                // of the image's background shows on the row.
                (colorScheme == .dark ? Color.white : Color.black)
                    .frame(width: Self.pixelSize.width * scale, height: Self.height)
                    .mask(
                        Image(nsImage: logo)
                            .resizable()
                            .interpolation(.high)
                            .luminanceToAlpha()
                    )
                    // The clear space stays empty but reaches into the row's margins,
                    // so the row is as tall as the logo and it lines up with the text.
                    .padding(-Self.clearSpace * scale)
            } else {
                // Not bundled (e.g. `swift run`): the name, still linking back.
                Text("TIDAL").bold()
            }
        }
        .buttonStyle(.plain)
        .help("tidal.com")
        .accessibilityLabel("TIDAL")
    }
}
