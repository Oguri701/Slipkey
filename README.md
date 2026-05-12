# Slipkey

Slipkey 是一款面向多语言用户的输入法切换工具。它让 macOS 和 Windows 使用同一套“输入式快捷键”：在任意文本框里输入短前缀，Slipkey 会在当前输入法转换这些按键之前完成切换。

Slipkey is a typed input-method switcher for multilingual users on macOS and Windows. Type a short trigger in any text field, and Slipkey switches the active input method before the current IME converts those keystrokes.

```text
;en  ->  English / ABC
;ja  ->  Japanese
;zh  ->  Chinese
```

[Download latest release](https://github.com/Oguri701/Slipkey/releases/latest)

## 中文

### 为什么做 Slipkey

如果你经常在中文、英文、日文之间切换，又同时使用 macOS 和 Windows，就很容易遇到这些问题：

- 每个系统、每台机器的输入法快捷键都不一样。
- 系统快捷键容易和应用快捷键冲突。
- 切错输入法以后，需要删除、重打，写作思路会被打断。
- CJK 输入法有组词和候选状态，等文本进入应用后再判断通常已经太晚。

Slipkey 把“切换输入法”变成打字流程的一部分。你不需要离开文本框，也不需要记每个平台不同的系统快捷键。

| 方案 | 常见问题 | Slipkey 的做法 |
| --- | --- | --- |
| 系统输入法快捷键 | 平台不统一，容易误触或冲突 | macOS / Windows 使用同样的输入式触发 |
| 文本替换或应用快捷键 | 发生在输入法处理之后，CJK 场景不稳定 | 在输入法转换之前读取按键事件 |
| 鼠标点菜单/工具栏 | 慢，并且打断输入节奏 | 保持在当前文本流里完成切换 |

### 下载安装

在 [GitHub Releases](https://github.com/Oguri701/Slipkey/releases/latest) 下载最新版本：

- macOS: `Slipkey-*-macos-arm64.zip`
- Windows: `Slipkey-*-windows-x64.zip`

### 平台支持

- **macOS 13+ / Apple Silicon**：Swift 原生应用，菜单栏图标，SwiftUI 设置界面，需要 Accessibility 权限。
- **Windows 10/11 x64**：Rust 桌面托盘应用，egui 设置界面，支持开机启动，Windows 包为单文件 `Slipkey.exe`。

### macOS 使用

1. 下载 `Slipkey-*-macos-arm64.zip`。
2. 解压后把 `Slipkey.app` 移到 `/Applications`。
3. 打开 `Slipkey.app`。
4. 到 **System Settings -> Privacy & Security -> Accessibility** 允许 Slipkey。
5. 打开 Slipkey 设置页，在 **Shortcuts** 里点击 **Detect**，确认输入源后点击 **Save**。

配置文件位置：

```text
~/.config/imeswitch/config.toml
```

卸载：

```bash
pkill -x Slipkey
rm -rf /Applications/Slipkey.app ~/.config/imeswitch
```

### Windows 使用

1. 下载 `Slipkey-*-windows-x64.zip`。
2. 解压后运行里面唯一的 `Slipkey.exe`。
3. 右键托盘图标，打开 **Settings**。
4. 在 **Shortcuts** 里配置引导键、前缀和输入源。
5. 如需开机启动，在 **General** 里打开 **Launch at login**。

首次运行时，Slipkey 会自动写出一个 helper DLL：

```text
%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll
```

这个 DLL 已经嵌入在 `Slipkey.exe` 里，用于 Windows 上 Microsoft Japanese IME 的 TSF 模式切换。你不需要手动管理它。

如果 Windows SmartScreen 提示 **Windows protected your PC**，这是因为当前版本尚未代码签名。点击 **More info -> Run anyway** 即可运行。

配置文件位置：

```text
%APPDATA%\imeswitch\config.toml
```

示例：

```toml
leader = ";"

[[mappings]]
language = "en"
prefix = "en"

[[mappings]]
language = "ja"
prefix = "ja"
source = "04110411"

[[mappings]]
language = "zh"
prefix = "zh"
source = "08040804"
```

建议优先使用设置页里的 **Detect** 自动填充本机输入源 ID。不同 Windows 机器上的输入源 ID 可能不同。

卸载：

1. 从托盘菜单退出 Slipkey。
2. 删除 `Slipkey.exe`。
3. 删除 `%APPDATA%\imeswitch\`。
4. 删除 `%LOCALAPPDATA%\Slipkey\`。
5. 如开启过开机启动，可删除注册表项 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Slipkey`。

### 从源码构建

Windows：

```powershell
cargo xtask build-windows
```

macOS：

```bash
bash scripts/package-macos.sh
```

更完整的新机器配置、测试和发版流程见 [DEVELOPMENT.md](DEVELOPMENT.md)。

### 架构

```text
bins/
  slipkey-app/          macOS 原生应用，Swift / SwiftPM
  slipkey-windows/      Windows 原生应用，Rust / egui

crates/
  imeswitch-core/         平台共享的纯状态机
  imeswitch-windows/      Windows hook、HKL 切换、TSF dispatch
  imeswitch-tsf-protocol/ Slipkey.exe 与 helper DLL 的共享 ABI
  slipkey-tsf-helper/     短生命周期 TSF helper DLL

xtask/                  项目自动化命令
scripts/                macOS 打包脚本
```

Slipkey 在按键事件层工作，而不是在文本层工作：

- macOS: `CGEventTap(HIDEventTap)`
- Windows: `WH_KEYBOARD_LL`

Windows CJK 输入法需要同时处理两个状态：

1. 通过 `WM_INPUTLANGCHANGEREQUEST` 切换焦点窗口的 HKL。
2. 通过短暂注入的 `slipkey_tsf.dll` 在目标 GUI 线程内写入 TSF Compartment。

某些 UWP 或受保护窗口可能拒绝 helper 注入。Slipkey 会记录日志并继续，不打扰当前应用。

## English

### Why Slipkey

If you switch between English, Chinese, and Japanese across macOS and Windows, input-method switching quickly becomes friction:

- System shortcuts differ across operating systems and machines.
- Global shortcuts can conflict with applications.
- A wrong input source forces deletion and retyping.
- CJK IMEs have composition states, so text-level detection often happens too late.

Slipkey makes input-method switching part of the typing flow. You stay in the text field and use the same trigger everywhere.

| Approach | Common problem | Slipkey difference |
| --- | --- | --- |
| OS input-source shortcuts | Different per platform; easy to mis-hit or conflict | Same typed trigger on macOS and Windows |
| Text expansion or app shortcuts | Runs after the IME may have handled the text | Reads key events before IME conversion |
| Toolbar or menu selection | Slow and interrupts typing | Switches inside the current writing flow |

### Download

Download the latest build from [GitHub Releases](https://github.com/Oguri701/Slipkey/releases/latest):

- macOS: `Slipkey-*-macos-arm64.zip`
- Windows: `Slipkey-*-windows-x64.zip`

### Platform Support

- **macOS 13+ / Apple Silicon**: native Swift app, status-bar menu, SwiftUI settings, Accessibility permission.
- **Windows 10/11 x64**: Rust tray app, egui settings, launch-at-login support, single-file `Slipkey.exe` distribution.

### macOS Usage

1. Download `Slipkey-*-macos-arm64.zip`.
2. Unzip it and move `Slipkey.app` to `/Applications`.
3. Open `Slipkey.app`.
4. Enable Slipkey in **System Settings -> Privacy & Security -> Accessibility**.
5. Open **Shortcuts**, click **Detect**, confirm the input sources, then click **Save**.

Config path:

```text
~/.config/imeswitch/config.toml
```

Uninstall:

```bash
pkill -x Slipkey
rm -rf /Applications/Slipkey.app ~/.config/imeswitch
```

### Windows Usage

1. Download `Slipkey-*-windows-x64.zip`.
2. Unzip it and run the single `Slipkey.exe`.
3. Right-click the tray icon and open **Settings**.
4. Configure the leader key, prefixes, and input sources in **Shortcuts**.
5. Enable **Launch at login** in **General** if desired.

On first run, Slipkey automatically provisions a helper DLL:

```text
%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll
```

The DLL is embedded in `Slipkey.exe` and is used for TSF-level Microsoft Japanese IME mode switching. Slipkey manages it automatically.

Windows SmartScreen may show **Windows protected your PC** because Slipkey is not yet code-signed. Click **More info -> Run anyway**.

Config path:

```text
%APPDATA%\imeswitch\config.toml
```

Example:

```toml
leader = ";"

[[mappings]]
language = "en"
prefix = "en"

[[mappings]]
language = "ja"
prefix = "ja"
source = "04110411"

[[mappings]]
language = "zh"
prefix = "zh"
source = "08040804"
```

Prefer **Detect** in Settings to populate source IDs for the current machine. Windows source IDs may differ between machines.

Uninstall:

1. Quit Slipkey from the tray menu.
2. Delete `Slipkey.exe`.
3. Delete `%APPDATA%\imeswitch\`.
4. Delete `%LOCALAPPDATA%\Slipkey\`.
5. Optionally remove `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Slipkey`.

### Build from Source

Windows:

```powershell
cargo xtask build-windows
```

macOS:

```bash
bash scripts/package-macos.sh
```

For full fresh-machine setup, tests, and release workflow, see [DEVELOPMENT.md](DEVELOPMENT.md).

### Architecture

```text
bins/
  slipkey-app/          macOS native app, Swift / SwiftPM
  slipkey-windows/      Windows native app, Rust / egui

crates/
  imeswitch-core/         Shared pure state machine
  imeswitch-windows/      Windows hook, HKL switching, TSF dispatch
  imeswitch-tsf-protocol/ Shared ABI between Slipkey.exe and helper DLL
  slipkey-tsf-helper/     Short-lived TSF helper DLL

xtask/                  Project automation commands
scripts/                macOS packaging script
```

Slipkey works at the key-event layer instead of the text layer:

- macOS: `CGEventTap(HIDEventTap)`
- Windows: `WH_KEYBOARD_LL`

Windows CJK IMEs require two operations:

1. `WM_INPUTLANGCHANGEREQUEST` switches the focused window's HKL.
2. `slipkey_tsf.dll` writes TSF compartments inside the target GUI thread.

Some UWP or protected windows may refuse helper injection. Slipkey logs and continues without disturbing the foreground app.

## License

MIT. See [LICENSE](LICENSE).
