import Foundation

/// Incremental Server-Sent Events parser: feed it bytes as they arrive, get the complete `data:` payloads.
/// Handles chunks cut anywhere (even inside a UTF-8 character), CRLF, comments (`: ok` heartbeats) and `retry:`.
public struct SSEParser: Sendable {
    private var buffer = Data()
    private var data: [String] = []
    public private(set) var retryMs: Int?

    public init() {}

    public mutating func feed(_ chunk: Data) -> [String] {
        buffer.append(chunk)
        var events: [String] = []
        while let nl = buffer.firstIndex(of: 0x0A) {
            var lineData = buffer[buffer.startIndex..<nl]
            buffer.removeSubrange(buffer.startIndex...nl)
            if lineData.last == 0x0D { lineData = lineData.dropLast() }
            let line = String(decoding: lineData, as: UTF8.self)
            if line.isEmpty {
                if !data.isEmpty { events.append(data.joined(separator: "\n")) }
                data.removeAll()
            } else if line.hasPrefix(":") {
                continue
            } else if line.hasPrefix("data:") {
                data.append(String(line.dropFirst(5)).trimmingLeadingSpace())
            } else if line.hasPrefix("retry:") {
                retryMs = Int(String(line.dropFirst(6)).trimmingLeadingSpace())
            }
        }
        return events
    }

    /// Parses the live price ticks, ignoring any malformed event.
    public mutating func ticks(_ chunk: Data) -> [LiveTick] {
        let decoder = JSONDecoder()
        return feed(chunk).compactMap { try? decoder.decode(LiveTick.self, from: Data($0.utf8)) }
    }
}

private extension String {
    func trimmingLeadingSpace() -> String { hasPrefix(" ") ? String(dropFirst()) : self }
}
