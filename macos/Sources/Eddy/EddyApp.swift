import SwiftUI
import UniformTypeIdentifiers

@main
struct EddyApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        // The menu bar icon is the app's only visible surface (LSUIElement: no Dock icon).
        MenuBarExtra {
            MenuContent(model: appDelegate.model, settingsWindow: appDelegate.settingsWindow)
        } label: {
            MenuBarIcon(model: appDelegate.model)
        }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()
    lazy var settingsWindow = SettingsWindowController(model: model)

    func applicationWillFinishLaunching(_ notification: Notification) {
        // Clicked links arrive as "get URL" Apple Events. Installed before launch finishes
        // so a link that launched the app isn't lost.
        NSAppleEventManager.shared().setEventHandler(
            self,
            andSelector: #selector(handleGetURL(_:withReplyEvent:)),
            forEventClass: AEEventClass(kInternetEventClass),
            andEventID: AEEventID(kAEGetURL)
        )
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        if terminateIfAlreadyRunning() { return }
        model.refreshDefaultHandlerStatus()
        if !model.hasCompletedOnboarding || !model.isDefaultHandler {
            settingsWindow.show()
        }
    }

    /// The schemes Eddy is registered for (Info.plist). Any app can still send it other
    /// URLs (`open -a Eddy <url>`); those are dropped, so Eddy doesn't pass on, with its
    /// own rights, what the sender couldn't open itself.
    private static let acceptedSchemes: Set<String> = ["http", "https", "spotify", "eddy"]

    /// The document types Eddy is registered for (Info.plist), as a browser.
    private static let acceptedFileTypes: [UTType] = [.html, UTType("public.xhtml")].compactMap { $0 }

    @objc private func handleGetURL(_ event: NSAppleEventDescriptor, withReplyEvent reply: NSAppleEventDescriptor) {
        guard let string = event.paramDescriptor(forKeyword: keyDirectObject)?.stringValue,
              let url = URL(string: string),
              let scheme = url.scheme?.lowercased(), Self.acceptedSchemes.contains(scheme) else { return }
        model.handle(url)
    }

    /// Local HTML files (Eddy is registered as a browser) go straight to the real browser.
    /// Other files (apps, scripts, …) are refused rather than handed to the browser.
    func application(_ sender: NSApplication, openFile filename: String) -> Bool {
        let url = URL(fileURLWithPath: filename)
        guard let type = try? url.resourceValues(forKeys: [.contentTypeKey]).contentType,
              Self.acceptedFileTypes.contains(where: type.conforms(to:)) else {
            log.error("refused to open a file that isn't HTML")
            return false
        }
        model.handle(url)
        return true
    }

    /// Opening the app again (e.g. from Finder) shows settings instead of doing nothing.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        settingsWindow.show()
        return false
    }

    /// Only one menu bar icon, even if a second copy of the app is launched.
    private func terminateIfAlreadyRunning() -> Bool {
        guard let id = Bundle.main.bundleIdentifier else { return false }
        let others = NSRunningApplication.runningApplications(withBundleIdentifier: id)
            .filter { $0 != NSRunningApplication.current }
        guard !others.isEmpty else { return false }
        NSApp.terminate(nil)
        return true
    }
}
