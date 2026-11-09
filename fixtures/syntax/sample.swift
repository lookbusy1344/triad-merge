// A sample that exercises the Swift grammar's highlight captures.
import Foundation

/// A point with two coordinates.
public struct Point<T: Numeric>: Equatable {
    public var x: T
    public var y: T

    public static let origin = Point(x: 0, y: 0)

    public func scaled(by factor: T) -> Point {
        Point(x: x * factor, y: y * factor)
    }
}

/* A block comment
   across lines. */
enum Mode: String, CaseIterable {
    case read = "r"
    case write = "w"
}

protocol Store: AnyObject {
    associatedtype Key: Hashable
    func load(_ keys: [Key]) async throws -> Int
}

@MainActor
final class Cache<Key: Hashable>: Store {
    private var items: [Key: String] = [:]
    private(set) var count = 0
    weak var delegate: AnyObject?

    init(items: [Key: String] = [:]) {
        self.items = items
    }

    func load(_ keys: [Key]) async throws -> Int {
        for key in keys where items[key] != nil {
            try await Task.sleep(nanoseconds: 1_000)
            count += 1
        }
        guard count > 0 else { throw CacheError.empty }
        defer { print("loaded \(count) of \(keys.count)\t") }
        switch count {
        case 0:
            return -1
        case 1...10:
            return count
        default:
            return 0x10
        }
    }
}

enum CacheError: Error {
    case empty
}

extension Point where T == Double {
    var length: Double { (x * x + y * y).squareRoot() }
}

let text = """
    multi-line "string"
    """
let regex = /[a-z]+\d?/
let closure: (Int) -> Int = { $0 * 2 }
var optional: String? = nil
if let value = optional ?? "fallback" as String? {
    print(value, true, false, closure(3), Mode.allCases, regex)
}
#if DEBUG
print(#file, #line)
#endif
