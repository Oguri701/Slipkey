# Slipkey

Slipkey 是一个面向多语言、多平台用户的输入法切换工具。它把 macOS 和 Windows PC 上原本不一致的输入法切换方式，统一成一套可记忆、可自定义的键入式快捷键。

如果你经常在中文、英文、日文等多语言之间切换，或者每天在 Mac 和 Windows 之间来回工作，你可能已经习惯了这些小打断：

- 系统默认快捷键不一致，Mac 和 Windows 的肌肉记忆互相打架
- 输入法切错后才发现，删掉、切回去、重打，思路被打断
- 全局快捷键容易和 App 自己的快捷键冲突
- CJK 输入法有组合/候选状态，普通“监听文字输入”的方案经常太晚才看到字符

Slipkey 的思路很简单：直接在任意文本框里输入短代码来切换输入法。

```text
;en  ->  English / ABC
;ja  ->  Japanese
;zh  ->  Chinese
```

这是一种“键入式切换”。它不要求你离开当前输入流，也不需要想起不同系统上的不同快捷键。你像打字一样切换输入法，切换完成后继续写，尽量保持输入时的沉浸感。

## 下载

最新版在 GitHub Releases：

[Download Slipkey](https://github.com/Oguri701/Slipkey/releases/latest)

- macOS: `Slipkey-*-macos-arm64.zip`
- Windows: `Slipkey-*-windows-x64.zip`

> macOS 版本目前是 ad-hoc signed，首次运行可能需要在系统设置里允许打开，并授予 Accessibility 权限。

## 平台支持

- **macOS**：原生 SwiftPM app，状态栏图标，设置窗口，单个 Accessibility 授权
- **Windows**：Rust tray app，egui 设置窗口，支持开机启动

## macOS 使用

### 要求

- macOS 13+
- Apple Silicon arm64

### 安装

1. 从 [Releases](https://github.com/Oguri701/Slipkey/releases/latest) 下载 `Slipkey-*-macos-arm64.zip`
2. 解压后把 `Slipkey.app` 放到 `/Applications`
3. 打开 `Slipkey.app`
4. 到 **System Settings -> Privacy & Security -> Accessibility** 里启用 Slipkey
5. 打开设置里的 **Shortcuts** 页，点 **Detect** 检测本机输入源，确认后点 **Save**

如果重装或重新构建后快捷键失效，可以重置授权后重新授予：

```bash
tccutil reset Accessibility dev.zlb.imeswitch
```

### 从源码构建

```bash
bash scripts/package-macos.sh
```

脚本会运行测试、构建 `Slipkey.app`、安装到 `/Applications/Slipkey.app`，并进行 ad-hoc 签名。

### 配置文件

`~/.config/imeswitch/config.toml`

```toml
leader = ";"

[[mappings]]
language = "en"
prefix = "en"
source = "com.apple.keylayout.ABC"

[[mappings]]
language = "ja"
prefix = "ja"
source = "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese"

[[mappings]]
language = "zh"
prefix = "zh"
source = "com.apple.inputmethod.SCIM.Shuangpin"
```

### 卸载

```bash
pkill -x Slipkey
rm -rf /Applications/Slipkey.app ~/.config/imeswitch
```

## Windows 使用

### 要求

- Windows 10/11 x64

### 安装

1. 从 [Releases](https://github.com/Oguri701/Slipkey/releases/latest) 下载 `Slipkey-*-windows-x64.zip`
2. 解压后运行 `Slipkey.exe`
3. 托盘区域会出现 Slipkey 图标
4. 右键托盘图标，打开 **Settings** 配置快捷键
5. 在 **General** 页可以启用开机启动

### 从源码构建

```bash
cargo build --release -p slipkey-windows --target x86_64-pc-windows-msvc
```

构建产物：

```text
target/x86_64-pc-windows-msvc/release/Slipkey.exe
```

### 配置文件

`%APPDATA%\imeswitch\config.toml`

配置格式和 macOS 相同。Windows 下的 `source` 是 HKL ID，例如：

- `00000409`：US English
- `00000411`：Japanese
- `00000804`：Chinese Simplified

### 卸载

1. 通过托盘菜单退出 Slipkey
2. 删除 `Slipkey.exe`
3. 删除 `%APPDATA%\imeswitch\`
4. 如启用了开机启动，可删除注册表项：`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Slipkey`

## 为什么不是普通快捷键

传统输入法切换通常依赖系统快捷键，但系统之间不统一，且很容易和应用快捷键冲突。更麻烦的是，多语言用户在写作、编程、聊天时经常需要快速切换输入语言；一旦切错输入法，往往要删除已经输入的内容，再切回正确输入源，节奏会被打断。

Slipkey 把“切换输入法”变成一种可输入的动作。你不需要离开键盘，也不需要记住每个平台不同的组合键，只需要输入同一套短代码。

## 为什么在 keycode 层工作

读取已输入的字符对 CJK 输入法不可靠。例如你想输入 `;en`，在日文或中文输入法里，应用层看到的可能已经是组合中的 `；えん` 或候选文本。此时再判断触发词就太晚了。

Slipkey 在更底层的 keycode 事件上工作：

- macOS：`CGEventTap(HIDEventTap)`
- Windows：`WH_KEYBOARD_LL`

这样可以在输入法转换字符之前识别触发序列，并消费这些按键。

## 架构

```text
bins/
  slipkey-app/          macOS native app (Swift, SwiftPM)
    Sources/SlipkeyApp/
      Hook/             CGEventTap, state machine, Carbon TIS IME switching
      Services/         Accessibility, login item, input source discovery
      App/              AppDelegate, AppState, WindowManager, StatusItemManager
      Views/            SwiftUI settings UI
      Stores/           Config persistence, L10n, UserDefaults
    Tests/              State machine, keycode, composition, config validation

  slipkey-windows/      Windows native app (Rust, egui)
    src/
      hook_thread.rs    WH_KEYBOARD_LL + PeekMessageW loop
      startup.rs        Registry launch-at-login
      tray.rs           System tray icon + menu
      ui/               egui settings window

crates/
  imeswitch-core/       Pure-Rust state machine shared by platform apps
  imeswitch-windows/    Windows hook + IME switching

scripts/
  package-macos.sh      macOS build pipeline
```

## 自动发布

推送 `v*` tag 会触发 GitHub Actions：

```bash
git tag -a v0.1.1 -m "Slipkey v0.1.1"
git push origin v0.1.1
```

Workflow 会构建并发布：

- `Slipkey-<version>-macos-arm64.zip`
- `Slipkey-<version>-windows-x64.zip`

---

# Slipkey

Slipkey is an input-method switcher for multilingual, cross-platform users. It gives macOS and Windows PC the same typed switching shortcuts, so you do not have to carry two different sets of input-source muscle memory.

If you move between English, Chinese, Japanese, macOS, and Windows, the friction is familiar:

- system input-source shortcuts differ across platforms
- switching to the wrong input method interrupts writing
- global shortcuts can conflict with app shortcuts
- CJK IMEs have composition states, so character-level detection often happens too late

Slipkey solves this with typed shortcuts. Type a short code in any text field, and Slipkey switches the OS input method before the active IME converts those keystrokes.

```text
;en  ->  English / ABC
;ja  ->  Japanese
;zh  ->  Chinese
```

The goal is immersion: switch input methods the same way you type, stay in the same text flow, and avoid losing your train of thought because the wrong IME was active.

## Download

Get the latest build from GitHub Releases:

[Download Slipkey](https://github.com/Oguri701/Slipkey/releases/latest)

- macOS: `Slipkey-*-macos-arm64.zip`
- Windows: `Slipkey-*-windows-x64.zip`

## Platform Support

- **macOS**: native SwiftPM app, status-bar icon, settings UI, single Accessibility grant
- **Windows**: Rust tray app, egui settings UI, launch-at-login support

## macOS

### Requirements

- macOS 13+
- Apple Silicon arm64

### Install

1. Download `Slipkey-*-macos-arm64.zip` from [Releases](https://github.com/Oguri701/Slipkey/releases/latest)
2. Unzip it and move `Slipkey.app` to `/Applications`
3. Open `Slipkey.app`
4. Enable Slipkey in **System Settings -> Privacy & Security -> Accessibility**
5. Open **Shortcuts**, click **Detect**, confirm the detected input sources, then click **Save**

### Build from source

```bash
bash scripts/package-macos.sh
```

This runs tests, builds `Slipkey.app`, installs it to `/Applications/Slipkey.app`, and ad-hoc signs it.

### Config

`~/.config/imeswitch/config.toml`

Use **Detect** in Settings to populate source IDs for your machine.

### Uninstall

```bash
pkill -x Slipkey
rm -rf /Applications/Slipkey.app ~/.config/imeswitch
```

## Windows

### Requirements

- Windows 10/11 x64

### Install

1. Download `Slipkey-*-windows-x64.zip` from [Releases](https://github.com/Oguri701/Slipkey/releases/latest)
2. Unzip it and run `Slipkey.exe`
3. Right-click the tray icon and open **Settings**
4. Configure shortcuts in **Shortcuts**
5. Enable **Launch at login** in **General** if desired

### Build from source

```bash
cargo build --release -p slipkey-windows --target x86_64-pc-windows-msvc
```

### Config

`%APPDATA%\imeswitch\config.toml`

The schema matches macOS. On Windows, `source` values are HKL IDs such as `00000409`, `00000411`, and `00000804`.

### Uninstall

1. Quit Slipkey from the tray menu
2. Delete `Slipkey.exe`
3. Delete `%APPDATA%\imeswitch\`
4. Optionally remove `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Slipkey`

## Why Typed Switching

System input-method shortcuts are easy to forget, easy to mis-hit, and different across platforms. For multilingual users, a wrong input source is not a small mistake: it breaks the sentence, forces deletion and retyping, and interrupts the thought you were trying to capture.

Typed switching makes input-source changes part of the writing flow. The trigger is visible, memorable, and consistent across macOS and Windows.

## Why Keycode-Level Detection

Watching typed characters is too late for CJK IMEs. The IME may turn `;en` into composed text before the app sees it.

Slipkey works at the keycode-event layer instead:

- macOS: `CGEventTap(HIDEventTap)`
- Windows: `WH_KEYBOARD_LL`

This lets Slipkey detect and consume trigger sequences before the active IME converts them.

## Release Automation

Push a `v*` tag to create a GitHub Release:

```bash
git tag -a v0.1.1 -m "Slipkey v0.1.1"
git push origin v0.1.1
```

GitHub Actions builds and uploads:

- `Slipkey-<version>-macos-arm64.zip`
- `Slipkey-<version>-windows-x64.zip`
