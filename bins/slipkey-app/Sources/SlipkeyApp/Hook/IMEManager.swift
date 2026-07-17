import Carbon
import Foundation

enum CurrentSourceKind {
    case keyboardLayout
    case inputMethod
    case other
}

struct TISSourceInfo {
    let id: String
    let name: String
    let category: String
    let type: String
    let languages: [String]
    let isEnabled: Bool
    let isSelectable: Bool
}

enum IMESwitchError: Error, CustomStringConvertible {
    case notInstalled(String)
    case notSelectable(String)
    case selectFailed(id: String, status: OSStatus)

    var description: String {
        switch self {
        case .notInstalled(let id):
            return "input source '\(id)' not installed — enable it in System Settings → Keyboard → Input Sources"
        case .notSelectable(let id):
            return "input source '\(id)' is installed but not selectable"
        case .selectFailed(let id, let status):
            return "TISSelectInputSource('\(id)') failed with OSStatus \(status)"
        }
    }
}

enum IMEManager {
    /// Synchronously selects the input source by its ID (e.g. "com.apple.keylayout.ABC").
    static func select(sourceID: String) -> Result<Void, IMESwitchError> {
        let filter = [kTISPropertyInputSourceID!: sourceID] as CFDictionary
        guard let listRef = TISCreateInputSourceList(filter, false) else {
            return .failure(.notInstalled(sourceID))
        }
        let list = listRef.takeRetainedValue() as NSArray
        guard let first = list.firstObject else {
            return .failure(.notInstalled(sourceID))
        }
        let source = first as! TISInputSource
        guard readBool(source, key: kTISPropertyInputSourceIsSelectCapable!) == true else {
            return .failure(.notSelectable(sourceID))
        }
        let status = TISSelectInputSource(source)
        if status != noErr {
            return .failure(.selectFailed(id: sourceID, status: status))
        }
        return .success(())
    }

    static func currentSourceID() -> String? {
        guard let srcRef = TISCopyCurrentKeyboardInputSource() else { return nil }
        let src = srcRef.takeRetainedValue()
        return readString(src, key: kTISPropertyInputSourceID!)
    }

    static func currentSourceKind() -> CurrentSourceKind {
        guard let srcRef = TISCopyCurrentKeyboardInputSource() else { return .other }
        let src = srcRef.takeRetainedValue()
        switch readString(src, key: kTISPropertyInputSourceType!) {
        case "TISTypeKeyboardLayout":
            return .keyboardLayout
        case "TISTypeKeyboardInputMode",
             "TISTypeKeyboardInputMethodWithoutModes",
             "TISTypeKeyboardInputMethodModeEnabled":
            return .inputMethod
        default:
            return .other
        }
    }

    /// Lists only input sources currently enabled in System Settings.
    /// Asking TIS for all installed sources and checking the `isEnabled`
    /// property is unreliable because that property can remain stale after a
    /// source is removed from the user's input menu.
    static func listEnabled() -> [TISSourceInfo] {
        guard let listRef = TISCreateInputSourceList(nil, false) else { return [] }
        let list = listRef.takeRetainedValue() as NSArray
        return list.compactMap { item -> TISSourceInfo? in
            let src = item as! TISInputSource
            return TISSourceInfo(
                id: readString(src, key: kTISPropertyInputSourceID!) ?? "",
                name: readString(src, key: kTISPropertyLocalizedName!) ?? "",
                category: readString(src, key: kTISPropertyInputSourceCategory!) ?? "",
                type: readString(src, key: kTISPropertyInputSourceType!) ?? "",
                languages: readStringArray(src, key: kTISPropertyInputSourceLanguages!) ?? [],
                isEnabled: readBool(src, key: kTISPropertyInputSourceIsEnabled!) ?? false,
                isSelectable: readBool(src, key: kTISPropertyInputSourceIsSelectCapable!) ?? false
            )
        }
    }

    // MARK: - Private CF readers

    private static func readString(_ source: TISInputSource, key: CFString) -> String? {
        guard let ptr = TISGetInputSourceProperty(source, key) else { return nil }
        return Unmanaged<CFString>.fromOpaque(ptr).takeUnretainedValue() as String
    }

    private static func readBool(_ source: TISInputSource, key: CFString) -> Bool? {
        guard let ptr = TISGetInputSourceProperty(source, key) else { return nil }
        return CFBooleanGetValue(Unmanaged<CFBoolean>.fromOpaque(ptr).takeUnretainedValue())
    }

    private static func readStringArray(_ source: TISInputSource, key: CFString) -> [String]? {
        guard let ptr = TISGetInputSourceProperty(source, key) else { return nil }
        let array = Unmanaged<CFArray>.fromOpaque(ptr).takeUnretainedValue() as NSArray
        return array.compactMap { $0 as? String }
    }
}
