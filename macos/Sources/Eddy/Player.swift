import AppKit

/// Reads the song that Tidal or Music is playing (or has paused), for "Copy Link for Current Song".
enum Player {
    enum Reading {
        case song(NowPlaying, Service)
        case nothingPlaying
        /// Tidal is read through the Accessibility API.
        case needsAccessibility
        /// Music is read through AppleScript; the user denied the Automation prompt.
        case needsAutomation
        /// Tidal's window is closed or on another desktop (Space), where the
        /// Accessibility API can't see it.
        case tidalWindowNotVisible
    }

    /// The preferred service's app first, then the other one. Blocking (AX calls and
    /// AppleScript); call it off the main thread. Without `mayPrompt`, Music is skipped
    /// until the user has answered the Automation prompt.
    static func read(preferring preferred: Service?, mayPrompt: Bool = true) -> Reading {
        let order: [Service] = preferred == .appleMusic ? [.appleMusic, .tidal] : [.tidal, .appleMusic]
        var blocked: Reading?
        for service in order where isRunning(service) {
            switch service == .tidal ? readTidal() : readMusic(mayPrompt: mayPrompt) {
            case let .song(song, service): return .song(song, service)
            case .nothingPlaying: continue
            case let other: blocked = blocked ?? other
            }
        }
        return blocked ?? .nothingPlaying
    }

    private static func isRunning(_ service: Service) -> Bool {
        !NSRunningApplication.runningApplications(withBundleIdentifier: TargetApps.bundleID(for: service)).isEmpty
    }

    // MARK: - Tidal

    /// The player bar (see `TidalPage` for the tree), verified on TIDAL 2.43.2:
    ///
    ///     AXGroup dom#=footerPlayer
    ///       AXGroup class=_trackContent_…
    ///         AXLink desc=<title>
    ///         AXGroup class=_artistRow_… → AXLink desc=<artist> (one per artist)
    ///         AXLink desc=<album>
    ///       AXGroup class=_scrubberTime_… (twice: position, then duration "4:10")
    ///
    /// Falls back to the window title, "<title> - <artist>, <artist>", while the web
    /// content's tree isn't built yet (right after the permission is first used).
    private static func readTidal() -> Reading {
        guard AX.isTrusted else { return .needsAccessibility }
        guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: TargetApps.bundleID(for: .tidal)).first
        else { return .nothingPlaying }
        let root = AX.application(pid: app.processIdentifier)
        AX.enableWebAccessibility(root)
        if let song = TidalPage.nowPlaying(in: root) {
            DebugCommands.log("player bar: \(song.artists.joined(separator: ", ")) – \(song.title) (\(song.album ?? "?"), \(song.durationMs.map { "\($0) ms" } ?? "?"))")
            return .song(song, .tidal)
        }
        let windows: [AXUIElement] = AX.attribute(root, kAXWindowsAttribute) ?? []
        let titles = windows.compactMap { AX.attribute($0, kAXTitleAttribute) as String? }
        for title in titles {
            if let song = songFromWindowTitle(title) {
                DebugCommands.log("player bar not readable (\(TidalPage.lastMiss)); window title: \(title)")
                return .song(song, .tidal)
            }
        }
        DebugCommands.log("Tidal: no player bar (\(TidalPage.lastMiss)), window titles \(titles)")
        return windows.isEmpty ? .tidalWindowNotVisible : .nothingPlaying
    }

    /// "Yeah! (feat. Lil Jon & Ludacris) - USHER, Lil Jon, Ludacris". Just "TIDAL" when
    /// nothing is playing.
    static func songFromWindowTitle(_ title: String) -> NowPlaying? {
        guard let separator = title.range(of: " - ", options: .backwards) else { return nil }
        let song = title[..<separator.lowerBound].trimmingCharacters(in: .whitespaces)
        let artists = title[separator.upperBound...]
            .components(separatedBy: ", ")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
        guard !song.isEmpty, !artists.isEmpty else { return nil }
        return NowPlaying(title: song, artists: artists, album: nil, durationMs: nil)
    }

    // MARK: - Music

    /// `tell application "Music"` would launch Music, so this only runs when it's running.
    private static let musicScript = """
    tell application id "com.apple.Music"
        if player state is stopped then return ""
        set t to current track
        set d to ""
        try
            set d to (round ((duration of t) * 1000)) as text
        end try
        return (name of t) & tab & (artist of t) & tab & (album of t) & tab & d
    end tell
    """

    private static func readMusic(mayPrompt: Bool) -> Reading {
        if !mayPrompt && !musicAutomationAllowed() { return .needsAutomation }
        var result: NSAppleEventDescriptor?
        var error: NSDictionary?
        // NSAppleScript isn't thread-safe; the first run may show the Automation prompt.
        DispatchQueue.main.sync {
            result = NSAppleScript(source: musicScript)?.executeAndReturnError(&error)
        }
        if let error {
            let code = error[NSAppleScript.errorNumber] as? Int
            DebugCommands.log("Music AppleScript failed: \(error)")
            // -1743: not authorized to send Apple Events (Automation denied).
            return code == -1743 ? .needsAutomation : .nothingPlaying
        }
        let fields = (result?.stringValue ?? "").components(separatedBy: "\t")
        guard fields.count == 4, !fields[0].isEmpty, !fields[1].isEmpty else { return .nothingPlaying }
        return .song(NowPlaying(
            title: fields[0],
            artists: splitArtists(fields[1]),
            album: fields[2].isEmpty ? nil : fields[2],
            durationMs: UInt64(fields[3])
        ), .appleMusic)
    }

    /// Whether Eddy may already send Apple Events to Music, without asking.
    private static func musicAutomationAllowed() -> Bool {
        let target = NSAppleEventDescriptor(bundleIdentifier: TargetApps.bundleID(for: .appleMusic))
        guard let desc = target.aeDesc else { return false }
        return AEDeterminePermissionToAutomateTarget(desc, typeWildCard, typeWildCard, false) == noErr
    }

    /// Music lists all artists in one string: "USHER feat. Lil Jon & Ludacris".
    static func splitArtists(_ artist: String) -> [String] {
        var parts = [artist]
        for separator in [" feat. ", " ft. ", " featuring ", ", ", " & "] {
            parts = parts.flatMap { $0.components(separatedBy: separator) }
        }
        let artists = parts.map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        return artists.isEmpty ? [artist] : artists
    }
}

extension TidalPage {
    static func nowPlaying(in root: AXUIElement) -> NowPlaying? {
        guard let footer = footerPlayer(in: root),
              let content = AX.find(in: footer, maxDepth: 2, where: { $0.domClasses.contains(prefix: "_trackContent") }).first
        else { return nil }
        let links = AX.find(in: content.element, maxDepth: 1) { $0.role == "AXLink" }
        guard let title = links.first?.description, !title.isEmpty else { return nil }
        let artists = AX.find(in: content.element, maxDepth: 3) { $0.role == "AXGroup" && $0.domClasses.contains(prefix: "_artistRow") }
            .first
            .map { row in AX.find(in: row.element, maxDepth: 1) { $0.role == "AXLink" }.compactMap(\.description) } ?? []
        guard !artists.isEmpty else { return nil }
        // The second time label is the length ("4:10"); the first is the position.
        let duration = AX.find(in: footer, maxDepth: 4) { $0.domClasses.contains(prefix: "_scrubberTimeText") }
            .dropFirst()
            .first
            .flatMap { AX.find(in: $0.element, maxDepth: 1) { $0.role == "AXStaticText" }.first?.value }
            .flatMap(milliseconds(fromClock:))
        return NowPlaying(
            title: title,
            artists: artists,
            album: links.count > 1 ? links.last?.description : nil,
            durationMs: duration
        )
    }

    /// "4:10" or "1:02:03".
    static func milliseconds(fromClock text: String) -> UInt64? {
        let parts = text.split(separator: ":").map { UInt64($0) }
        guard !parts.isEmpty, parts.allSatisfy({ $0 != nil }) else { return nil }
        return parts.reduce(0) { $0 * 60 + $1! } * 1000
    }
}
