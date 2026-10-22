import AppKit
import ApplicationServices

/// Thin wrapper over the macOS Accessibility (AX) API, used to read another app's UI
/// and press its buttons (autoplay). Needs the Accessibility permission.
enum AX {
    static var isTrusted: Bool { AXIsProcessTrusted() }

    /// Shows the system prompt that sends the user to Privacy & Security → Accessibility.
    static func requestTrust() {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        AXIsProcessTrustedWithOptions([key: true] as CFDictionary)
    }

    static func application(pid: pid_t) -> AXUIElement {
        let app = AXUIElementCreateApplication(pid)
        // Don't let a hung target app block us for the default 6 s per call.
        AXUIElementSetMessagingTimeout(app, 1.0)
        return app
    }

    /// Electron/Chromium apps (Tidal) only build the accessibility tree of their web
    /// content once an assistive app asks for it.
    @discardableResult
    static func enableWebAccessibility(_ app: AXUIElement) -> String {
        let manual = AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
        let enhanced = AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
        return "AXManualAccessibility=\(manual.rawValue) AXEnhancedUserInterface=\(enhanced.rawValue)"
    }

    static func attribute<T>(_ element: AXUIElement, _ name: String) -> T? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
        return value as? T
    }

    static func children(_ element: AXUIElement) -> [AXUIElement] {
        attribute(element, kAXChildrenAttribute) ?? []
    }

    static func actions(_ element: AXUIElement) -> [String] {
        var names: CFArray?
        guard AXUIElementCopyActionNames(element, &names) == .success else { return [] }
        return (names as? [String]) ?? []
    }

    @discardableResult
    static func press(_ element: AXUIElement) -> Bool {
        AXUIElementPerformAction(element, kAXPressAction as CFString) == .success
    }

    /// Sends a key press straight to an app's process, without focusing its window.
    static func sendKey(_ keyCode: CGKeyCode, to pid: pid_t) {
        for down in [true, false] {
            CGEvent(keyboardEventSource: nil, virtualKey: keyCode, keyDown: down)?.postToPid(pid)
        }
    }

    /// Double-clicks at a screen point (top-left origin) through the system event
    /// stream, then puts the pointer back where it was. The target window must be in
    /// front at that point.
    static func doubleClick(at point: CGPoint) {
        let original = CGEvent(source: nil)?.location
        let source = CGEventSource(stateID: .hidSystemState)
        for clickState in [1, 2] {
            for type in [CGEventType.leftMouseDown, .leftMouseUp] {
                guard let event = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: point, mouseButton: .left) else { continue }
                event.setIntegerValueField(.mouseEventClickState, value: Int64(clickState))
                event.post(tap: .cghidEventTap)
            }
        }
        if let original {
            CGWarpMouseCursorPosition(original)
            CGAssociateMouseAndMouseCursorPosition(1)
        }
    }

    static func frame(_ element: AXUIElement) -> CGRect? {
        guard let posValue: AXValue = attribute(element, kAXPositionAttribute),
              let sizeValue: AXValue = attribute(element, kAXSizeAttribute) else { return nil }
        var position = CGPoint.zero
        var size = CGSize.zero
        AXValueGetValue(posValue, .cgPoint, &position)
        AXValueGetValue(sizeValue, .cgSize, &size)
        return CGRect(origin: position, size: size)
    }

    /// The attributes autoplay matching looks at.
    struct Node {
        let element: AXUIElement
        let depth: Int
        let role: String
        let subrole: String?
        let title: String?
        let description: String?
        let value: String?
        let help: String?
        let identifier: String?
        /// Chromium only: the DOM element's id and class list.
        let domID: String?
        let domClasses: [String]
        let isSelected: Bool

        init(_ element: AXUIElement, depth: Int) {
            self.element = element
            self.depth = depth
            role = AX.attribute(element, kAXRoleAttribute) ?? "?"
            subrole = AX.attribute(element, kAXSubroleAttribute)
            title = AX.attribute(element, kAXTitleAttribute)
            description = AX.attribute(element, kAXDescriptionAttribute)
            value = AX.attribute(element, kAXValueAttribute) as String?
            help = AX.attribute(element, kAXHelpAttribute)
            identifier = AX.attribute(element, kAXIdentifierAttribute)
            domID = AX.attribute(element, "AXDOMIdentifier")
            domClasses = AX.attribute(element, "AXDOMClassList") ?? []
            isSelected = (AX.attribute(element, kAXSelectedAttribute) as Bool?) ?? false
        }

        /// Every human-readable label of the element, for text matching.
        var labels: [String] {
            [title, description, value, help].compactMap { $0 }.filter { !$0.isEmpty }
        }

        var line: String {
            var parts = [String(repeating: "  ", count: depth) + role]
            if let subrole { parts.append("(\(subrole))") }
            for (name, text) in [("title", title), ("desc", description), ("value", value), ("help", help), ("id", identifier), ("dom#", domID)] {
                if let text, !text.isEmpty { parts.append("\(name)=\(text.prefix(80).debugDescription)") }
            }
            if !domClasses.isEmpty { parts.append("class=\(domClasses.joined(separator: "."))") }
            if isSelected { parts.append("SELECTED") }
            if let frame = AX.frame(element) {
                parts.append("@\(Int(frame.minX)),\(Int(frame.minY)) \(Int(frame.width))x\(Int(frame.height))")
            }
            let actions = AX.actions(element).filter { $0 != "AXShowMenu" && $0 != "AXScrollToVisible" }
            if !actions.isEmpty { parts.append("actions=\(actions.joined(separator: ","))") }
            return parts.joined(separator: " ")
        }
    }

    /// Visits the tree breadth-first. Return `false` from `visit` to stop.
    static func walk(_ root: AXUIElement, maxDepth: Int = 60, maxNodes: Int = 20_000, visit: (Node) -> Bool) {
        var queue: [(AXUIElement, Int)] = [(root, 0)]
        var index = 0
        while index < queue.count, index < maxNodes {
            let (element, depth) = queue[index]
            index += 1
            if !visit(Node(element, depth: depth)) { return }
            if depth < maxDepth {
                queue.append(contentsOf: children(element).map { ($0, depth + 1) })
            }
        }
    }

    static func find(in root: AXUIElement, maxDepth: Int = 60, where matches: (Node) -> Bool) -> [Node] {
        var found: [Node] = []
        walk(root, maxDepth: maxDepth) { node in
            if matches(node) { found.append(node) }
            return true
        }
        return found
    }

    /// The tree as indented text, depth-first, for debugging.
    static func dump(_ root: AXUIElement, maxDepth: Int = 60) -> String {
        var lines: [String] = []
        func visit(_ element: AXUIElement, _ depth: Int) {
            guard lines.count < 20_000 else { return }
            lines.append(Node(element, depth: depth).line)
            guard depth < maxDepth else { return }
            for child in children(element) { visit(child, depth + 1) }
        }
        visit(root, 0)
        return lines.joined(separator: "\n")
    }
}
