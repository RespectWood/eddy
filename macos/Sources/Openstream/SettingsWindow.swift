import SwiftUI

/// The onboarding / settings window. Shown on first launch, whenever openstream isn't
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
            window.title = "openstream"
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
                    Picker("Open Tidal tracks on", selection: $model.tidalLandingPage) {
                        ForEach(TidalLandingPage.allCases) { page in
                            Text(page.displayName).tag(page)
                        }
                    }
                    DisclosureGroup("Tidal API credentials (optional)") {
                        Text("Without these, Tidal opens a search for the track. With a free app from developer.tidal.com, it opens the exact track.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        TextField("Client ID", text: $model.tidalClientID)
                        SecureField("Client secret", text: $model.tidalClientSecret)
                    }
                }
            }

            Section("2. Your normal browser") {
                Picker("Open other links in", selection: $model.browserBundleID) {
                    ForEach(Browsers.available()) { browser in
                        Text(browser.name).tag(Optional(browser.bundleID))
                    }
                }
            }

            Section("3. Let openstream receive links") {
                Text("macOS sends clicked links to one app, which it calls the \"default web browser\". openstream takes that slot, but it isn't a browser: it converts Spotify links and immediately passes every other link to the browser above. macOS will ask you to confirm.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                HStack {
                    if model.isDefaultHandler {
                        Label("openstream receives links", systemImage: "checkmark.circle.fill")
                            .foregroundStyle(.green)
                    } else {
                        Button("Use openstream for links") { makeDefault() }
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
                try await Browsers.makeOpenstreamDefault()
            } catch {
                setupError = "Couldn't change the default browser: \(error.localizedDescription). You can also set it in System Settings → Desktop & Dock → Default web browser."
            }
            model.refreshDefaultHandlerStatus()
        }
    }
}
