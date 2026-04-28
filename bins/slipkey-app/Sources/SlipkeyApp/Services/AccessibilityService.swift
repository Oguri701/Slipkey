import ApplicationServices
import AppKit

enum AccessibilityService {
    static var isTrusted: Bool {
        AXIsProcessTrusted()
    }

    static func request() {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue()
        let options = [key: true] as CFDictionary
        AXIsProcessTrustedWithOptions(options)
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!)
    }
}
