import ServiceManagement
import SwiftUI

struct MenuBarIcon: View {
    @ObservedObject var model: AppModel

    var body: some View {
        // Template images, so macOS tints them for light and dark menu bars.
        Group {
            if model.service == nil || !model.isDefaultHandler {
                Image(systemName: "exclamationmark.circle")
            } else {
                Image(nsImage: EddyGlyph.menuBarImage(dimmed: model.isDisabled))
            }
        }
        .accessibilityLabel("Eddy: \(model.statusLine)")
    }
}

struct MenuContent: View {
    @ObservedObject var model: AppModel
    let settingsWindow: SettingsWindowController
    @State private var launchAtLogin = LoginItem.isEnabled

    var body: some View {
        Text(model.statusLine)
        if model.service == nil || !model.isDefaultHandler {
            Button("Not active: set up Eddy…") { settingsWindow.show() }
        }
        Button(model.isDisabled ? "Enable Eddy" : "Disable Eddy") { model.isDisabled.toggle() }
            .keyboardShortcut("p")
        Divider()

        Button("Copy Spotify Link for Current Song") { model.copySpotifyLink() }
            .keyboardShortcut("c", modifiers: [.command, .shift])

        Menu("Recently Opened") {
            if model.recents.isEmpty {
                Text("None yet")
            }
            ForEach(model.recents) { item in
                Menu("\(item.artist) – \(item.title)") {
                    Button("Open in \(item.service.displayName)") { model.reopen(item) }
                    Button("Open Original Link") { model.openOriginal(item) }
                }
            }
        }
        Divider()

        Picker("Open Spotify Links In", selection: $model.service) {
            ForEach(Service.allCases) { service in
                Text(TargetApps.isInstalled(service) ? service.displayName : "\(service.displayName) (not installed)")
                    .tag(Optional(service))
            }
        }
        if model.service == .tidal {
            Toggle("Show Artist Page Instead of Song", isOn: $model.showsTidalArtistPage)
        }
        Toggle("Start Playing Automatically", isOn: $model.autoPlay)
        // Hints come after the settings, so they don't split them.
        if model.autoPlayNeedsPermission {
            Button("Autoplay needs Accessibility permission…") { AX.requestTrust() }
        }
        if model.service == .tidal {
            switch model.tidalAccount.status {
            case .notConnected, .failed:
                Button("Connect Tidal for Exact Matches…") { settingsWindow.show() }
            case .needsReconnect:
                Button("Tidal login expired: Reconnect…") { settingsWindow.show() }
            default:
                EmptyView()
            }
        }
        Divider()

        Picker("Open Other Links In", selection: $model.browserBundleID) {
            ForEach(Browsers.available()) { browser in
                Text(browser.name).tag(Optional(browser.bundleID))
            }
        }
        Toggle("Launch at Login", isOn: $launchAtLogin)
            .onChange(of: launchAtLogin) { enabled in
                LoginItem.set(enabled)
                launchAtLogin = LoginItem.isEnabled
            }
        Divider()

        Button("Settings…") { settingsWindow.show() }
            .keyboardShortcut(",")
        Button("About Eddy") {
            NSApp.activate(ignoringOtherApps: true)
            NSApp.orderFrontStandardAboutPanel(nil)
        }
        Button("Quit Eddy") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
            .onAppear {
                // The menu is rebuilt each time it opens: a cheap moment to re-check status.
                model.refreshDefaultHandlerStatus()
                launchAtLogin = LoginItem.isEnabled
                // Start looking up the Spotify link now, so "Copy Spotify Link" is instant.
                model.prefetchSpotifyLink()
            }
    }
}

enum LoginItem {
    static var isEnabled: Bool { SMAppService.mainApp.status == .enabled }

    static func set(_ enabled: Bool) {
        do {
            if enabled { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
        } catch {
            log.error("launch at login: \(error.localizedDescription, privacy: .public)")
        }
    }
}
