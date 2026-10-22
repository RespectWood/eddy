import AppKit

/// Starts playback after a link opened the target app on the right page, by pressing
/// that page's play control through the Accessibility API. Neither Tidal nor Music
/// can start playback from a link, so this is the only way.
enum AutoPlay {
    struct Target {
        enum Kind { case track, album }
        let service: Service
        let kind: Kind
        let title: String
        let artist: String
        let album: String?
        /// The URL that opened the item; Apple Music's contains the exact album/track IDs.
        var openedURL: String = ""
    }

    /// How long to wait for the app to launch (and log in) and show the page.
    private static let timeout: TimeInterval = 45
    private static let pollInterval: TimeInterval = 0.4
    private static let maxPresses = 3

    /// Bumped by every start, so a newer link cancels an older autoplay still waiting.
    private static let generation = ManagedAtomicCounter()

    /// Runs in the background; returns immediately.
    static func start(_ target: Target) {
        guard AX.isTrusted else {
            DebugCommands.log("autoplay skipped: no Accessibility permission")
            return
        }
        let run = generation.increment()
        DispatchQueue.global(qos: .userInitiated).async { self.run(target, generation: run) }
    }

    private static func run(_ target: Target, generation run: Int) {
        let started = Date()
        let bundleID = TargetApps.bundleID(for: target.service)
        DebugCommands.log("autoplay \(target.service) \(target.kind) \"\(target.artist) – \(target.title)\" (album \(target.album ?? "?"), \(target.openedURL))")
        if target.service == .tidal { TidalPage.lastMiss = "not checked yet" }
        var attempt = 0
        var presses = 0
        var checkedCurrentSong = false
        while Date().timeIntervalSince(started) < timeout {
            guard generation.value == run else {
                DebugCommands.log("autoplay for \"\(target.title)\" cancelled by a newer link")
                return
            }
            attempt += 1
            let app = NSRunningApplication.runningApplications(withBundleIdentifier: bundleID).first
            if attempt % 5 == 1 {
                let state = app.map { "running pid \($0.processIdentifier), finished launching \($0.isFinishedLaunching), active \($0.isActive)" } ?? "not running"
                DebugCommands.log("attempt \(attempt): \(state); last miss: \(target.service == .tidal ? TidalPage.lastMiss : "-")")
            }
            if let app, app.isFinishedLaunching || attempt > 10 {
                let root = AX.application(pid: app.processIdentifier)
                AX.enableWebAccessibility(root)
                // Hit tests only reach a window that isn't covered. If the page still
                // hasn't shown up after ~6 s, bring the app forward once (the link
                // opened it, so it's where the user is looking anyway).
                if attempt == 15 && !app.isActive {
                    DebugCommands.log("bringing \(target.service) to the front")
                    DispatchQueue.main.async { app.activate() }
                }
                // Already the current song: pressing its button again would pause it.
                // Checked once, as soon as the player is readable.
                if target.kind == .track, !checkedCurrentSong,
                   let playing = nowPlayingTitle(in: root, for: target) {
                    checkedCurrentSong = true
                    if Titles.same(playing, target.title) {
                        DebugCommands.log("already the current song; not pressing")
                        return
                    }
                }
                if let control = findPlayControl(in: root, for: target) {
                    let pressed = activate(control, target, pid: app.processIdentifier)
                    DebugCommands.log("attempt \(attempt): pressed \(pressed) → \(control.line)")
                    if pressed {
                        presses += 1
                        // Right after launch, Tidal restores the last session's song and
                        // that can override an early press, so press again (only while the
                        // player shows a *different* song, so the right one never pauses).
                        if verify(root, target, after: started) || presses >= maxPresses { return }
                        Thread.sleep(forTimeInterval: 1.0)
                        continue
                    }
                }
            }
            Thread.sleep(forTimeInterval: pollInterval)
        }
        DebugCommands.log("autoplay gave up after \(attempt) attempts: \(target.service == .tidal ? TidalPage.lastMiss : "control not found")")
    }

    /// Presses the control. Music track rows only get selected by a press; a double-click
    /// plays them, like in the app itself.
    private static func activate(_ control: AX.Node, _ target: Target, pid: pid_t) -> Bool {
        if target.service == .appleMusic && target.kind == .track {
            guard let frame = AX.frame(control.element) else { return false }
            // Click the empty space right of the title, away from the row's buttons.
            let point = CGPoint(x: frame.minX + frame.width * 0.6, y: frame.midY)
            // A real click lands on whatever is in front, so only click when that's Music
            // and the point is inside its window.
            let root = AX.application(pid: pid)
            let window: AXUIElement? = (AX.attribute(root, kAXWindowsAttribute) as [AXUIElement]?)?.first
            guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
                  let windowFrame = window.flatMap(AX.frame), windowFrame.contains(point) else {
                DebugCommands.log("Music isn't in front (or the row is off-window); not clicking")
                return false
            }
            AX.press(control.element)
            AX.doubleClick(at: point)
            return true
        }
        return AX.press(control.element)
    }

    /// Whether the player shows the linked track within ~3 s. Albums can't be checked
    /// by title, so they count as verified.
    private static func verify(_ root: AXUIElement, _ target: Target, after started: Date) -> Bool {
        guard target.kind == .track else { return true }
        for _ in 0..<10 {
            Thread.sleep(forTimeInterval: 0.3)
            let playing = nowPlayingTitle(in: root, for: target)
            if let playing, Titles.same(playing, target.title) {
                DebugCommands.log(String(format: "verified: now playing \"%@\" after %.1fs", playing, Date().timeIntervalSince(started)))
                return true
            }
        }
        DebugCommands.log("not verified: player shows \(nowPlayingTitle(in: root, for: target) ?? "nothing")")
        return false
    }

    private static func nowPlayingTitle(in root: AXUIElement, for target: Target) -> String? {
        switch target.service {
        case .tidal: TidalPage.nowPlayingTitle(in: root)
        case .appleMusic: MusicPage.nowPlayingTitle(in: root)
        }
    }

    /// The control that plays exactly the linked item. `nil` until the page has loaded.
    private static func findPlayControl(in root: AXUIElement, for target: Target) -> AX.Node? {
        switch target.service {
        case .tidal: TidalPage.playControl(in: root, for: target)
        case .appleMusic: MusicPage.playControl(in: root, for: target)
        }
    }
}

/// A thread-safe counter.
final class ManagedAtomicCounter: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0

    var value: Int { lock.withLock { count } }

    func increment() -> Int {
        lock.withLock {
            count += 1
            return count
        }
    }
}

/// Tidal desktop (Electron). The web content's accessibility tree (verified on TIDAL 2.43.2):
///
///     AXWebArea title="<album> / <artist> - TIDAL"
///       AXGroup dom#=main
///         AXButton class=_marketButton….primary…           ← album "Play"
///         AXTable desc=Tracklist
///           AXRow class=…_tableRowSelected_…              ← the linked track
///             AXCell → AXButton class=_playButton_…       ← that track's "Play"
///       AXGroup dom#=footerPlayer → _trackContent → AXLink desc=<now playing title>
///
/// CSS class names end in a build hash, so only their prefixes are matched. Button
/// labels are localized, so they aren't matched at all.
enum TidalPage {
    /// Why the last lookup found nothing, for the debug log.
    nonisolated(unsafe) static var lastMiss = ""

    static func playControl(in root: AXUIElement, for target: AutoPlay.Target) -> AX.Node? {
        guard let web = webArea(in: root) else { return nil }
        // Wait until the link's page has replaced whatever was shown before.
        let pageTitle: String = AX.attribute(web, kAXTitleAttribute) ?? ""
        if let album = target.album, !album.isEmpty, !pageTitle.hasPrefix(album + " / ") {
            return miss("page is \"\(pageTitle)\", waiting for \"\(album) / …\"")
        }
        guard let main = AX.find(in: web, maxDepth: 8, where: { $0.domID == "main" }).first else {
            return miss("no main content on \"\(pageTitle)\"")
        }
        switch target.kind {
        case .track:
            let rows = AX.find(in: main.element, maxDepth: 6) { $0.role == "AXRow" }
            let row = rows.first { $0.domClasses.contains(prefix: "_tableRowSelected") }
                ?? rows.first { rowTitle($0.element).map { Titles.same($0, target.title) } ?? false }
            guard let row else { return miss("\(rows.count) rows, none highlighted or titled \"\(target.title)\"") }
            let title = rowTitle(row.element)
            guard title.map({ Titles.same($0, target.title) }) ?? false else {
                return miss("highlighted row is \"\(title ?? "?")\", not \"\(target.title)\"")
            }
            return AX.find(in: row.element, maxDepth: 5) {
                $0.role == "AXButton" && $0.domClasses.contains(prefix: "_playButton")
            }.first ?? miss("row has no play button")
        case .album:
            return AX.find(in: main.element, maxDepth: 3) {
                $0.role == "AXButton" && $0.domClasses.contains(prefix: "_marketButton")
                    && $0.domClasses.contains(prefix: "_primary")
            }.first ?? miss("no primary play button on \"\(pageTitle)\"")
        }
    }

    private static func miss(_ reason: String) -> AX.Node? {
        lastMiss = reason
        return nil
    }

    /// Title of the track that's playing (or paused) in the player bar.
    static func nowPlayingTitle(in root: AXUIElement) -> String? {
        guard let web = webArea(in: root),
              let footer = AX.find(in: web, maxDepth: 6, where: { $0.domID == "footerPlayer" }).first,
              let content = AX.find(in: footer.element, maxDepth: 2, where: { $0.domClasses.contains(prefix: "_trackContent") }).first
        else { return nil }
        return AX.find(in: content.element, maxDepth: 1) { $0.role == "AXLink" }.first?.description
    }

    /// Chromium only builds the web content's tree after an assistive app asks for it
    /// (`AXManualAccessibility`). In a freshly launched Tidal the web area is then only
    /// reachable *upwards*: a hit test lands inside the page and its parents lead to the
    /// `AXWebArea`, while the window's children don't list it. So: children first (works
    /// once Tidal has been running a while), then hit test and walk up.
    private static func webArea(in root: AXUIElement) -> AXUIElement? {
        let windows: [AXUIElement] = AX.attribute(root, kAXWindowsAttribute) ?? []
        for window in windows {
            if let web = AX.find(in: window, maxDepth: 12, where: { $0.role == "AXWebArea" }).first {
                return web.element
            }
        }
        for window in windows {
            guard let frame = AX.frame(window) else { continue }
            for (fx, fy) in [(0.5, 0.5), (0.7, 0.4), (0.3, 0.3), (0.5, 0.85)] {
                var hit: AXUIElement?
                let x = Float(frame.minX + frame.width * fx), y = Float(frame.minY + frame.height * fy)
                guard AXUIElementCopyElementAtPosition(root, x, y, &hit) == .success else { continue }
                var current = hit
                for _ in 0..<40 {
                    guard let element = current else { break }
                    if (AX.attribute(element, kAXRoleAttribute) as String?) == "AXWebArea" { return element }
                    current = AX.attribute(element, kAXParentAttribute)
                }
            }
        }
        let titles = windows.compactMap { AX.attribute($0, kAXTitleAttribute) as String? }
        lastMiss = "no web area yet (\(windows.count) windows: \(titles))"
        return nil
    }

    private static func rowTitle(_ row: AXUIElement) -> String? {
        AX.find(in: row, maxDepth: 4) { $0.domClasses.contains(prefix: "_titleText") }
            .first
            .flatMap { group in AX.find(in: group.element, maxDepth: 1) { $0.role == "AXStaticText" }.first?.value }
    }
}

/// Loose title comparison, mirroring the Rust core's `normalize`: case, bracketed parts,
/// "- Remastered" suffixes and punctuation are ignored.
enum Titles {
    static func same(_ a: String, _ b: String) -> Bool {
        let (x, y) = (normalize(a), normalize(b))
        return !x.isEmpty && x == y
    }

    static func normalize(_ s: String) -> String {
        var base = s.lowercased()
        if let range = base.range(of: " - ") { base = String(base[..<range.lowerBound]) }
        var out = ""
        var depth = 0
        for c in base {
            switch c {
            case "(", "[": depth += 1
            case ")", "]": depth = max(0, depth - 1)
            case _ where depth > 0: continue
            case "&": out += " and "
            case _ where c.isLetter || c.isNumber: out.append(c)
            default: out.append(" ")
            }
        }
        return out.split(separator: " ").joined(separator: " ")
    }
}

extension [String] {
    func contains(prefix: String) -> Bool { contains { $0.hasPrefix(prefix) } }
}

/// Apple Music (verified on macOS 15). Album pages list each track as a group whose
/// identifier contains the catalog IDs from the link, so the exact track is found:
///
///     AXUnknown id="Music.shelfItem.AlbumDetailHeaderLockup[id=album-detail-header-<album>…"
///       AXButton title=desc="Play"                       ← album "Play" (labels localized)
///     AXGroup id="Music.shelfItem.AlbumTrackLockup[id=track-lockup-<album>-<track>…"  ← press plays it
enum MusicPage {
    static func playControl(in root: AXUIElement, for target: AutoPlay.Target) -> AX.Node? {
        guard let ids = catalogIDs(target.openedURL), let window = mainWindow(root) else { return nil }
        switch target.kind {
        case .track:
            guard let track = ids.track else { return nil }
            let needle = "track-lockup-\(ids.album)-\(track)"
            return AX.find(in: window, maxDepth: 12) { $0.identifier?.contains(needle) ?? false }.first
        case .album:
            let needle = "album-detail-header-\(ids.album)"
            guard let header = AX.find(in: window, maxDepth: 12, where: { $0.identifier?.contains(needle) ?? false }).first
            else { return nil }
            // The header's text buttons have the same title and description (Play, Shuffle,
            // Add); the artist link and "More" don't. Play comes first.
            return AX.find(in: header.element, maxDepth: 2) {
                $0.role == "AXButton" && $0.title != nil && $0.title == $0.description
            }.first
        }
    }

    static func nowPlayingTitle(in root: AXUIElement) -> String? {
        guard let window = mainWindow(root),
              let lcd = AX.find(in: window, maxDepth: 3, where: { $0.role == "AXGroup" && $0.description == "LCD section" }).first
        else { return nil }
        return AX.find(in: lcd.element, maxDepth: 4) { $0.role == "AXStaticText" && !($0.value ?? "").isEmpty }.first?.value
    }

    /// `music://music.apple.com/se/album/<slug>/<album>?i=<track>`
    static func catalogIDs(_ url: String) -> (album: String, track: String?)? {
        guard let components = URLComponents(string: url),
              let album = components.path.split(separator: "/").last.map(String.init),
              album.allSatisfy(\.isNumber) else { return nil }
        return (album, components.queryItems?.first { $0.name == "i" }?.value)
    }

    private static func mainWindow(_ root: AXUIElement) -> AXUIElement? {
        let windows: [AXUIElement] = AX.attribute(root, kAXWindowsAttribute) ?? []
        return windows.first
    }
}
