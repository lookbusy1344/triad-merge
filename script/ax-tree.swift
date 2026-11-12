// Print a running app's accessibility tree, as VoiceOver reads it, after pressing the named
// elements in order (DESIGN.md §A6.7, docs/release-checklist.md). Each line is
// role/subrole | title | value. Menus are left out.
// Usage: swift script/ax-tree.swift PID [NAME...]
// The terminal needs Accessibility permission (System Settings → Privacy & Security).
import ApplicationServices
import Foundation

let maxValueLength = 60
let pollInterval = 0.2
let maxPolls = 50
let settleSeconds = 0.5

func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
    var value: AnyObject?
    return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
}

func text(_ element: AXUIElement, _ name: String) -> String {
    attribute(element, name).map { "\($0)".replacingOccurrences(of: "\n", with: "⏎") } ?? ""
}

func children(_ element: AXUIElement) -> [AXUIElement] {
    (attribute(element, "AXChildren") as? [AXUIElement]) ?? []
}

func find(_ element: AXUIElement, titled title: String) -> AXUIElement? {
    if text(element, "AXTitle") == title { return element }
    return children(element).lazy.compactMap { find($0, titled: title) }.first
}

func printTree(_ element: AXUIElement, depth: Int) {
    let role = text(element, "AXRole")
    guard !role.hasPrefix("AXMenu") else { return }
    let value = text(element, "AXValue")
    let shown = value.count > maxValueLength ? value.prefix(maxValueLength) + "…" : Substring(value)
    print(String(repeating: "  ", count: depth)
        + "\(role)/\(text(element, "AXSubrole")) | \(text(element, "AXTitle")) | \(shown)")
    children(element).forEach { printTree($0, depth: depth + 1) }
}

let arguments = CommandLine.arguments.dropFirst()
guard let first = arguments.first, let pid = pid_t(first) else {
    FileHandle.standardError.write(Data("usage: swift script/ax-tree.swift PID [NAME...]\n".utf8))
    exit(2)
}
guard AXIsProcessTrusted() else {
    FileHandle.standardError.write(Data("grant this terminal Accessibility permission\n".utf8))
    exit(1)
}
let app = AXUIElementCreateApplication(pid)
// GPUI builds its tree on the first query from an assistive client; wait until the window
// has content.
func windowHasContent() -> Bool {
    ((attribute(app, "AXWindows") as? [AXUIElement]) ?? []).contains { window in
        children(window).contains { !children($0).isEmpty }
    }
}
for _ in 0..<maxPolls where !windowHasContent() {
    Thread.sleep(forTimeInterval: pollInterval)
}
for name in arguments.dropFirst() {
    guard let element = find(app, titled: name) else {
        FileHandle.standardError.write(Data("no element titled \(name)\n".utf8))
        exit(1)
    }
    let result = AXUIElementPerformAction(element, kAXPressAction as CFString)
    guard result == .success else {
        FileHandle.standardError.write(Data("pressing \(name) failed: \(result.rawValue)\n".utf8))
        exit(1)
    }
    Thread.sleep(forTimeInterval: settleSeconds)
}
printTree(app, depth: 0)
