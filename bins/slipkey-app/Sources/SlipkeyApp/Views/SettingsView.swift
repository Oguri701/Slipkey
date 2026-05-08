import AppKit
import SwiftUI

private let kGitHubURL = URL(string: "https://github.com/Oguri701/Slipkey")!

private struct ContentHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

struct SettingsContent: View {
    @ObservedObject var appState: AppState
    @ObservedObject var tabState: SettingsTabState

    var body: some View {
        Group {
            switch tabState.selection {
            case .general:
                GeneralSettingsView(appState: appState)
            case .shortcuts:
                ShortcutSettingsView(appState: appState)
            case .about:
                AboutSettingsView(appState: appState)
            }
        }
        // The order here matters. `GeometryReader` must wrap the bare
        // `Group` (the actual content) so it reports the fitting size of
        // whichever tab is rendered. Putting `.background(GeometryReader…)`
        // *after* `.frame(maxWidth: .infinity, alignment: .top)` made it
        // measure the outer frame layer instead — and because that frame
        // uses `alignment: .top`, SwiftUI is allowed to propose the full
        // NSHostingView height to it. That feedback loop is what stretched
        // the window vertically every time the user opened the Shortcuts
        // tab and `onAppear` triggered a re-layout.
        .background(
            GeometryReader { geo in
                Color.clear.preference(key: ContentHeightKey.self, value: geo.size.height)
            }
        )
        .frame(maxWidth: .infinity, alignment: .top)
        .onPreferenceChange(ContentHeightKey.self) { height in
            guard height > 10 else { return }
            tabState.onContentHeight?(height)
        }
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
                .frame(width: 330, alignment: .leading)
            }
        }
        .onAppear {
            appState.refreshDetectedSources()
        }
    }
}

struct ShortcutTable: View {
    @ObservedObject var appState: AppState

    // tableW - hPad*2 - langW - gap - prefW - gap = sourceW
    private let tableW:  CGFloat = 330
    private let hPad:    CGFloat = 10
    private let langW:   CGFloat = 70
    private let prefW:   CGFloat = 52
    private let gap:     CGFloat = 8
    private var sourceW: CGFloat { tableW - hPad * 2 - langW - gap - prefW - gap }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                TableHeader(L10n.text("Language", appState.uiLanguage))
                    .frame(width: langW, alignment: .leading)
                Spacer().frame(width: gap)
                TableHeader(L10n.text("Prefix", appState.uiLanguage))
                    .frame(width: prefW, alignment: .leading)
                Spacer().frame(width: gap)
                TableHeader(L10n.text("Input source", appState.uiLanguage))
                    .frame(width: sourceW, alignment: .leading)
            }
            .padding(.horizontal, hPad)
            .padding(.bottom, 4)

            Divider().padding(.leading, hPad)

            ForEach($appState.config.mappings) { $mapping in
                HStack(spacing: 0) {
                    Text(languageName(mapping.language))
                        .lineLimit(1)
                        .frame(width: langW, alignment: .leading)
                    Spacer().frame(width: gap)
                    TextField("", text: $mapping.prefix)
                        .textFieldStyle(.roundedBorder)
                        .controlSize(.small)
                        .frame(width: prefW)
                    Spacer().frame(width: gap)
                    SourceMenu(
                        mapping: $mapping,
                        sources: appState.detectedSources.filter { $0.language == mapping.language },
                        width: sourceW
                    )
                }
                .padding(.horizontal, hPad)
                .padding(.vertical, 3)
                if mapping.id != appState.config.mappings.last?.id {
                    Divider().padding(.leading, hPad)
                }
            }
        }
        .padding(.vertical, 5)
        .frame(width: tableW)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
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

struct SourceMenu: View {
    @Binding var mapping: MappingEntry
    let sources: [InputSource]
    let width: CGFloat

    var body: some View {
        Menu {
            ForEach(sources) { source in
                Button {
                    mapping.source = source.sourceID
                } label: {
                    menuItemLabel(source.name, isSelected: source.sourceID == mapping.source)
                }
            }
            if !sources.contains(where: { $0.sourceID == mapping.source }) {
                Button {} label: {
                    menuItemLabel(mapping.source, isSelected: true)
                }
                .disabled(true)
            }
        } label: {
            HStack(spacing: 6) {
                Text(selectedSourceName)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
            .font(.system(size: 12))
            .foregroundStyle(.primary)
            .padding(.horizontal, 8)
            .frame(width: width, height: 22, alignment: .leading)
            .background(Color(nsColor: .controlColor), in: RoundedRectangle(cornerRadius: 5, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Input source")
        .accessibilityValue(selectedSourceName)
    }

    private var selectedSourceName: String {
        sources.first { $0.sourceID == mapping.source }?.name ?? mapping.source
    }

    @ViewBuilder
    private func menuItemLabel(_ title: String, isSelected: Bool) -> some View {
        if isSelected {
            Label(title, systemImage: "checkmark")
        } else {
            Text(title)
        }
    }
}

struct AboutSettingsView: View {
    @ObservedObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 16) {
                Image(nsImage: NSApp.applicationIconImage)
                    .resizable()
                    .frame(width: 76, height: 76)

                VStack(alignment: .leading, spacing: 5) {
                    Text("Slipkey")
                        .font(.system(size: 38, weight: .ultraLight))
                    Text(L10n.text("Switch input methods by typing.", appState.uiLanguage))
                        .font(.system(size: 11, weight: .light))
                        .foregroundStyle(.primary)
                    Text("v\(appVersion())  ·  © 2026 oguri701")
                        .font(.system(size: 9))
                        .foregroundStyle(Color(nsColor: .disabledControlTextColor))
                        .padding(.top, 2)
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 16)
            .padding(.bottom, 16)

            Divider()
                .padding(.leading, 20)

            HStack(spacing: 8) {
                Button {
                    NSWorkspace.shared.open(kGitHubURL)
                } label: {
                    Text(L10n.text("View on GitHub", appState.uiLanguage))
                        .frame(minWidth: 110)
                }
                .controlSize(.regular)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func appVersion() -> String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.1.0"
    }
}

struct PreferenceContent<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            content
        }
        .frame(maxWidth: 420)
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .center)
    }
}

struct PreferenceRow<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Text(label)
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
                .frame(width: 70, alignment: .trailing)
                .padding(.top, 4)
            content
                .frame(maxWidth: .infinity, alignment: .leading)
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
                    .font(.system(size: 13))
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .toggleStyle(.checkbox)
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
