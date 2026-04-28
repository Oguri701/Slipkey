import Foundation

final class SettingsStore {
    @UserDefault("menuBarIconVisible", defaultValue: true) var menuBarIconVisible: Bool
    @UserDefault("launchAtLogin", defaultValue: false) var launchAtLogin: Bool
    @UserDefault("uiLanguage", defaultValue: "en") var uiLanguage: String
}

@propertyWrapper
struct UserDefault<Value> {
    let key: String
    let defaultValue: Value

    init(_ key: String, defaultValue: Value) {
        self.key = key
        self.defaultValue = defaultValue
    }

    var wrappedValue: Value {
        get { UserDefaults.standard.object(forKey: key) as? Value ?? defaultValue }
        set { UserDefaults.standard.set(newValue, forKey: key) }
    }
}
