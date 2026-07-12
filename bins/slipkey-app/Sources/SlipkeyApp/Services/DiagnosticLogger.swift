import Foundation

final class DiagnosticLogger {
    static let shared = DiagnosticLogger()

    static let logURL = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Logs/Slipkey/diagnostics.log")

    private static let maxLogBytes: UInt64 = 1_000_000
    private let queue = DispatchQueue(label: "dev.zlb.imeswitch.diagnostics", qos: .utility)
    private let formatter: ISO8601DateFormatter

    private init() {
        formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    }

    func log(_ event: String, fields: [String: String] = [:]) {
        queue.async { [formatter] in
            let line = Self.formatLine(
                timestamp: formatter.string(from: Date()),
                event: event,
                fields: fields
            )
            Self.append(line)
        }
    }

    static func formatLine(timestamp: String, event: String, fields: [String: String]) -> String {
        let values = fields.keys.sorted().map { key in
            "\(sanitize(key))=\(quoted(fields[key] ?? ""))"
        }
        return ([timestamp, sanitize(event)] + values).joined(separator: " ") + "\n"
    }

    private static func append(_ line: String) {
        let fileManager = FileManager.default
        let directory = logURL.deletingLastPathComponent()
        do {
            try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
            rotateIfNeeded(incomingBytes: line.utf8.count, fileManager: fileManager)
            if !fileManager.fileExists(atPath: logURL.path) {
                try Data().write(to: logURL, options: .atomic)
            }
            let handle = try FileHandle(forWritingTo: logURL)
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: Data(line.utf8))
        } catch {
            NSLog("Slipkey diagnostics: %@", String(describing: error))
        }
    }

    private static func rotateIfNeeded(incomingBytes: Int, fileManager: FileManager) {
        guard let attributes = try? fileManager.attributesOfItem(atPath: logURL.path),
              let size = attributes[.size] as? NSNumber,
              size.uint64Value + UInt64(incomingBytes) > maxLogBytes
        else { return }

        let previous = logURL.deletingPathExtension().appendingPathExtension("previous.log")
        try? fileManager.removeItem(at: previous)
        try? fileManager.moveItem(at: logURL, to: previous)
    }

    private static func sanitize(_ value: String) -> String {
        value.replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: " ", with: "_")
    }

    private static func quoted(_ value: String) -> String {
        let clean = value.replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        return "\"\(clean)\""
    }
}
