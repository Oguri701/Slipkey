import ApplicationServices

enum AccessibilityService {
    static var isTrusted: Bool {
        AXIsProcessTrusted()
    }

    static func request() {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue()
        let options = [key: true] as CFDictionary
        AXIsProcessTrustedWithOptions(options)
    }
}
