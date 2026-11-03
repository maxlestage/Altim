import XCTest

/// French texts of the app (iPhone, Watch, widgets, engines): no double space before a punctuation mark, which shows
/// as a gap on screen and is read as a pause by VoiceOver.
final class TextTypographyTests: XCTestCase {
    func testNoDoubleSpaceBeforePunctuation() throws {
        let ios = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let pattern = try NSRegularExpression(pattern: "[^ ]  [:;!?»]")
        var found: [String] = []
        var files = 0
        for dir in ["Altim", "AltimWatch", "AltimWidgets", "AltimKit/Sources"] {
            guard let e = FileManager.default.enumerator(at: ios.appendingPathComponent(dir), includingPropertiesForKeys: nil) else { continue }
            for case let url as URL in e where url.pathExtension == "swift" {
                files += 1
                for (n, line) in try String(contentsOf: url, encoding: .utf8).components(separatedBy: "\n").enumerated()
                where !line.trimmingCharacters(in: .whitespaces).hasPrefix("//") {
                    if pattern.firstMatch(in: line, range: NSRange(line.startIndex..., in: line)) != nil { found.append("\(url.lastPathComponent):\(n + 1)") }
                }
            }
        }
        XCTAssertGreaterThan(files, 40)
        XCTAssertEqual(found, [])
    }
}
