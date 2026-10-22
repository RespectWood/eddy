import ServiceManagement
import SwiftUI

struct MenuBarIcon: View {
    @ObservedObject var model: AppModel

    var body: some View {
        // SF Symbols are template images, so macOS tints them for light and dark menu bars.
        Image(systemName: symbol)
            .accessibilityLabel("openstream: \(model.statusLine)")
    }

    private var symbol: String {
        if model.isDisabled { return "nosign" }
        if model.service == nil || !model.isDefaultHandler { return "exclamationmark.circle" }
        return "music.note"
    }
}

struct MenuContent: View {
    @ObservedObject var model: AppModel
    let settingsWindow: SettingsWindowController
    @State private var launchAtLogin = LoginItem.isEnabled

    var body: some View {
        Text(model.statusLine)
        if model.service == nil || !model.isDefaultHandler {
            Button("Not active: set up openstream…") { settingsWindow.show() }
        }
        Divider()

        Button(model.isDisabled ? "Enable openstream" : "Disable openstream") { model.isDisabled.toggle() }
            .keyboardShortcut("p")

        Menu("Recent Conversions") {
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

        Picker("Preferred Service", selection: $model.service) {
            ForEach(Service.allCases) { service in
                Text(TargetApps.isInstalled(service) ? service.displayName : "\(service.displayName) (not installed)")
                    .tag(Optional(service))
            }
        }
        if model.service == .tidal {
            Picker("Open Tidal Tracks On", selection: $model.tidalLandingPage) {
                ForEach(TidalLandingPage.allCases) { page in
                    Text(page.displayName).tag(page)
                }
            }
        }
        Toggle("Start Playing Automatically", isOn: $model.autoPlay)
        if model.autoPlayNeedsPermission {
            Button("Autoplay needs Accessibility permission…") { AX.requestTrust() }
        }
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
        Button("About openstream") {
            NSApp.activate(ignoringOtherApps: true)
            NSApp.orderFrontStandardAboutPanel(nil)
        }
        Button("Quit openstream") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
            .onAppear {
                // The menu is rebuilt each time it opens: a cheap moment to re-check status.
                model.refreshDefaultHandlerStatus()
                launchAtLogin = LoginItem.isEnabled
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
