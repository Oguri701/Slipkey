import SwiftUI

struct SettingsView: View {
    @ObservedObject var appState: AppState
    @State private var selection: SettingsSection = .general

    var body: some View {
        VStack(spacing: 0) {
            PreferencesToolbar(selection: $selection, language: appState.uiLanguage)
            Divider()

            Group {
                switch selection {
                case .general:
                    GeneralSettingsView(appState: appState)
                case .shortcuts:
                    ShortcutSettingsView(appState: appState)
                case .about:
                    AboutSettingsView(appState: appState)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(Color(nsColor: .windowBackgroundColor))
        }
        .frame(width: 560, height: 380)
    }
}

enum SettingsSection: String, CaseIterable, Identifiable {
    case general
    case shortcuts
    case about

    var id: String { rawValue }

    var systemImage: String {
        switch self {
        case .general: "gearshape"
        case .shortcuts: "keyboard"
        case .about: "info.circle"
        }
    }

    func title(_ language: String) -> String {
        switch self {
        case .general: L10n.text("General", language)
        case .shortcuts: L10n.text("Shortcuts", language)
        case .about: L10n.text("About", language)
        }
    }
}

struct PreferencesToolbar: View {
    @Binding var selection: SettingsSection
    let language: String

    var body: some View {
        HStack(spacing: 14) {
            ForEach(SettingsSection.allCases) { section in
                Button {
                    selection = section
                } label: {
                    VStack(spacing: 5) {
                        Image(systemName: section.systemImage)
                            .font(.system(size: 20, weight: .regular))
                            .symbolRenderingMode(.hierarchical)
                        Text(section.title(language))
                            .font(.caption)
                            .lineLimit(1)
                    }
                    .frame(width: 68, height: 48)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(selection == section ? Color.accentColor : Color.primary)
                .background {
                    if selection == section {
                        RoundedRectangle(cornerRadius: 7, style: .continuous)
                            .fill(Color.accentColor.opacity(0.12))
                    }
                }
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 7)
        .padding(.bottom, 6)
        .background(.bar)
    }
}

struct GeneralSettingsView: View {
    @ObservedObject var appState: AppState

    var body: some View {
        PreferenceContent {
            PreferenceRow(label: L10n.text("General", appState.uiLanguage)) {
                VStack(alignment: .leading, spacing: 10) {
                    PreferenceToggleRow(
                        title: L10n.text("Open at startup", appState.uiLanguage),
                        detail: L10n.text("Start Slipkey automatically after login.", appState.uiLanguage),
                        isOn: Binding(get: { appState.launchAtLogin }, set: { appState.launchAtLogin = $0 })
                    )
                    PreferenceToggleRow(
                        title: L10n.text("Show menu bar icon", appState.uiLanguage),
                        detail: L10n.text("Keep a small menu bar entry for quick access.", appState.uiLanguage),
                        isOn: Binding(get: { appState.menuBarIconVisible }, set: { appState.menuBarIconVisible = $0 })
                    )
                }
            }

            Divider()

            PreferenceRow(label: L10n.text("Language", appState.uiLanguage)) {
                HStack {
                    Picker("", selection: Binding(get: { appState.uiLanguage }, set: { appState.uiLanguage = $0 })) {
                        Text("English").tag("en")
                        Text("中文").tag("zh")
                        Text("日本語").tag("ja")
                    }
                    .labelsHidden()
                    .frame(width: 128)
                    Spacer()
                }
            }

            Divider()

            PreferenceRow(label: L10n.text("Permissions", appState.uiLanguage)) {
                HStack(spacing: 10) {
                    Image(systemName: appState.accessibilityGranted ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                        .foregroundStyle(appState.accessibilityGranted ? .green : .orange)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(appState.accessibilityGranted ? L10n.text("Ready", appState.uiLanguage) : L10n.text("Accessibility permission required", appState.uiLanguage))
                        Text(L10n.text("Needed to listen for typed shortcuts before the active input method converts them.", appState.uiLanguage))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if !appState.accessibilityGranted {
                        Button(L10n.text("Grant Accessibility", appState.uiLanguage)) {
                            appState.requestAccessibility()
                        }
                        .controlSize(.small)
                    }
                }
            }

        }
    }
}

struct ShortcutSettingsView: View {
    @ObservedObject var appState: AppState

    var body: some View {
        PreferenceContent {
            PreferenceRow(label: L10n.text("Leader key", appState.uiLanguage)) {
                HStack(alignment: .center, spacing: 10) {
                    TextField("", text: Binding(
                        get: { appState.config.leader },
                        set: { appState.config.leader = String($0.prefix(1)) }
                    ))
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 18, weight: .semibold, design: .monospaced))
                    .multilineTextAlignment(.center)
                    .frame(width: 48)
                    Text(L10n.text("Type this first, then a prefix such as ;en. Pick a rarely used key to avoid accidental triggers.", appState.uiLanguage))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 0)
                }
            }

            Divider()

            PreferenceRow(label: L10n.text("Input sources", appState.uiLanguage)) {
                VStack(alignment: .leading, spacing: 8) {
                    ShortcutTable(appState: appState)

                    HStack(spacing: 8) {
                        if !appState.statusMessage.isEmpty {
                            Text(appState.statusMessage)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                        Spacer()
                        Button(L10n.text("Reset to defaults", appState.uiLanguage)) {
                            appState.resetShortcutsToDefaults()
                        }
                        .controlSize(.small)
                        Button(L10n.text("Detect", appState.uiLanguage)) {
                            appState.refreshDetectedSources()
                        }
                        .controlSize(.small)
                        Button(L10n.text("Save", appState.uiLanguage)) {
                            appState.saveAndRestart()
                        }
                        .controlSize(.small)
                        .keyboardShortcut("s", modifiers: [.command])
                    }
                    .padding(.top, 1)
                }
                .frame(width: 388, alignment: .leading)
            }
        }
        .onAppear {
            appState.refreshDetectedSources()
        }
    }
}

struct ShortcutTable: View {
    @ObservedObject var appState: AppState
    private let columns = [
        GridItem(.fixed(82), spacing: 10, alignment: .leading),
        GridItem(.fixed(70), spacing: 10, alignment: .leading),
        GridItem(.fixed(190), spacing: 0, alignment: .leading)
    ]

    var body: some View {
        VStack(spacing: 0) {
            LazyVGrid(columns: columns, alignment: .leading, spacing: 0) {
                TableHeader(L10n.text("Language", appState.uiLanguage))
                TableHeader(L10n.text("Prefix", appState.uiLanguage))
                TableHeader(L10n.text("Input source", appState.uiLanguage))
            }
            .padding(.horizontal, 12)
            .padding(.bottom, 5)

            ForEach($appState.config.mappings) { $mapping in
                LazyVGrid(columns: columns, alignment: .leading, spacing: 0) {
                    Text(languageName(mapping.language))
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    TextField("", text: $mapping.prefix)
                        .textFieldStyle(.roundedBorder)
                        .controlSize(.small)
                        .frame(width: 58)
                    Picker("", selection: $mapping.source) {
                        ForEach(appState.detectedSources.filter { $0.language == mapping.language }) { source in
                            Text(source.name).tag(source.sourceID)
                        }
                        if !appState.detectedSources.contains(where: { $0.sourceID == mapping.source }) {
                            Text(mapping.source).tag(mapping.source)
                        }
                    }
                    .labelsHidden()
                    .controlSize(.small)
                    .frame(width: 190, alignment: .leading)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 4)
                if mapping.id != appState.config.mappings.last?.id {
                    Divider().padding(.leading, 12)
                }
            }
        }
        .padding(.vertical, 6)
        .frame(width: 388, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 7, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .stroke(Color(nsColor: .separatorColor), lineWidth: 0.5)
        )
    }

    private func languageName(_ code: String) -> String {
        switch code {
        case "en": "English"
        case "zh": "中文"
        case "ja": "日本語"
        default: code.uppercased()
        }
    }
}

struct AboutSettingsView: View {
    @ObservedObject var appState: AppState

    var body: some View {
        PreferenceContent {
            VStack(spacing: 10) {
                Image(nsImage: NSApp.applicationIconImage)
                    .resizable()
                    .frame(width: 64, height: 64)
                    .cornerRadius(14)
                Text("Slipkey")
                    .font(.title2.weight(.semibold))
                Text("0.1.0")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(.top, 4)
            .padding(.bottom, 10)

            PreferenceSection(title: L10n.text("What it solves", appState.uiLanguage)) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(L10n.text("Slipkey gives multilingual Mac and Windows users one typed shortcut system for switching input methods.", appState.uiLanguage))
                    Text(L10n.text("Instead of reaching for different platform shortcuts and breaking your typing flow, type a short code like ;en, ;zh, or ;ja. Slipkey switches the system input source and removes the trigger text before it appears.", appState.uiLanguage))
                }
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

struct PreferenceContent<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                content
            }
            .frame(maxWidth: 510)
            .padding(.horizontal, 22)
            .padding(.vertical, 20)
            .frame(maxWidth: .infinity, alignment: .center)
        }
    }
}

struct PreferenceRow<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            Text(label)
                .font(.system(size: 14))
                .foregroundStyle(.secondary)
                .frame(width: 94, alignment: .trailing)
                .padding(.top, 4)
            content
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

struct PreferenceSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 10) {
                content
            }
        }
    }
}

struct PreferenceToggleRow: View {
    let title: String
    let detail: String
    @Binding var isOn: Bool

    var body: some View {
        Toggle(isOn: $isOn) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.system(size: 14))
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .toggleStyle(.checkbox)
    }
}

struct PreferencePickerRow<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            content
        }
    }
}

struct TableHeader: View {
    let title: String

    init(_ title: String) {
        self.title = title
    }

    var body: some View {
        Text(title)
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct KeyboardShortcutBadge: View {
    let label: String

    init(_ label: String) {
        self.label = label
    }

    var body: some View {
        Text(label)
            .font(.system(size: 12, weight: .medium, design: .monospaced))
            .frame(minWidth: 20, minHeight: 18)
            .background(.quaternary, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
    }
}
