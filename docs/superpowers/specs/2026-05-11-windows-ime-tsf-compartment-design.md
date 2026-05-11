# Windows IME 切换重构：TSF Compartment 短命注入

- **日期**：2026-05-11
- **状态**：Approved (brainstorming)
- **作者**：Slipkey 维护团队
- **目标分支**：`design/windows-ime-tsf-compartment`

## 一句话设计

`;en` 在日语 IME 下保持 IME 激活、仅切内部 conversion mode 到 Alphanumeric；做法是用**短命** `SetWindowsHookEx` 把一个 ~30KB 的 Rust cdylib 临时映射到焦点进程的 GUI 线程，**在该线程内**通过 TSF `ITfCompartment` 设置权威的 `GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION`，几毫秒后卸载。旧的 IMM32 + `WM_IME_CONTROL` + DBE 虚拟键三路兜底全部删除。UWP / 系统进程注入失败时静默、仅写日志。

## 动机

当前 `crates/imeswitch-windows/src/ime/mode.rs` 在切换"日语 → 英文"时表现脆弱：

- 在日版 Windows 与中版 Windows 上行为不一致
- 依赖 30ms sleep + 重复 `VK_DBE_ALPHANUMERIC` 模拟键来"赌"IME 响应
- 三条路径（IMM32 / `WM_IME_CONTROL` / DBE 模拟键）并行兜底，没有任何一条是权威

根本原因：Microsoft 日语 IME 是 **TSF (Text Services Framework) IME**，其 conversion mode 状态由 TSF Compartment 持有，**只在目标进程内部可被权威写入**。IMM32 与 `WM_IME_CONTROL` 都是 TSF 之上的兼容 shim，对纯 TSF IME 是"发请求、不保证生效"。在外部进程里"调 API 设模式"本质就是脆弱的。

## 关键决策

| # | 决策 | 选定 |
|---|------|------|
| D1 | `;en` 在日语 IME 下的目标状态 | **保持日语 IME 激活，仅切内部 conversion mode 到 Alphanumeric** |
| D2 | 技术路线 | **DLL 注入 + TSF Compartment** |
| D3 | 注入策略 | **短命 hook 借道**：`SetWindowsHookEx` 注入 → 触发执行 → `UnhookWindowsHookEx`，整个过程 ~5–10ms |
| D4 | DLL 产出 | **新增 `crates/slipkey-tsf-helper`**，独立 cdylib |
| D5 | 旧 IMM32 / WM_IME_CONTROL / DBE 路径 | **完全删除**，不保留 fallback |
| D6 | UWP / 系统进程注入失败的行为 | **始终静默、只记入日志** |

## 现状诊断

当前 `switch_entry` 在 `WinImeMode::Alphanumeric` / `Native` 分支里：

1. 同步切 HKL（`WM_INPUTLANGCHANGEREQUEST`）— 这一步**稳定**，保留
2. `std::thread::spawn` 一个 worker 线程
3. worker 内 `sleep(30ms)`
4. 调用 `imm32_set_mode` (`ImmSetOpenStatus` + `ImmSetConversionStatus`)
5. 调用 `ime_window_set_mode` (`SendMessage(ImmGetDefaultIMEWnd, WM_IME_CONTROL, ...)`)
6. 对日语，额外 `SendInput(VK_DBE_HIRAGANA)` 或 `SendInput(VK_DBE_ALPHANUMERIC)`，再 sleep 30ms，再发一次

步骤 4–6 在 TSF IME 上不是权威操作，时序敏感、跨地区不一致。

## 架构总览

```
┌────────────────────────────────────────────────────────────────────┐
│                       Slipkey.exe (tray app)                        │
│                                                                     │
│   hook_thread.rs (WH_KEYBOARD_LL)                                   │
│         │                                                           │
│         ▼ (state machine → Language::Ja / Language::En)             │
│   imeswitch-windows::WindowsImeSwitcher::switch_to(lang)            │
│         │                                                           │
│         ▼                                                           │
│   ime::switch_entry(entry)                                          │
│         ├── Step 1: HKL 切换（保留）                                │
│         │       layout::switch_layout_sync(...)                     │
│         │       layout::broadcast_layout_change(...)                │
│         │                                                           │
│         └── Step 2: TSF Compartment 写入                            │
│                 tsf_dispatch::global().dispatch(target)             │
│                     │                                               │
│                     ├── 拿到 focused HWND / TID                     │
│                     ├── 写共享内存 TsfCommand                       │
│                     ├── SetWindowsHookEx(WH_CALLWNDPROC,           │
│                     │                  slipkey_tsf.dll, TID)        │
│                     ├── PostThreadMessageW(TID, WM_NULL, 0, 0)      │
│                     ├── WaitForSingleObject(done, 200ms)            │
│                     └── UnhookWindowsHookEx                         │
└────────────────────────────────────────────────────────────────────┘
                                  │
                  瞬态注入（持续 ~5ms，做完即卸）
                                  ▼
┌────────────────────────────────────────────────────────────────────┐
│  目标 GUI 进程（Notepad / VSCode / Chrome / ...）                   │
│                                                                     │
│   slipkey_tsf.dll (cdylib, ~30KB)                                   │
│       DllMain DLL_PROCESS_ATTACH                                    │
│         │                                                           │
│         ▼ call_wnd_hook 被回调（GUI 线程上下文）                    │
│   execute_tsf_command():                                            │
│         ├── 读共享内存 TsfCommand                                   │
│         ├── 校验 abi_version                                        │
│         ├── CoInitializeEx(APARTMENTTHREADED)                       │
│         ├── CoCreateInstance(CLSID_TF_ThreadMgr)                    │
│         ├── mgr.Activate(&client_id)                                │
│         ├── mgr -> ITfCompartmentMgr                                │
│         ├── cmp_mgr.GetCompartment(                                 │
│         │       GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)                │
│         │   .SetValue(client_id, open_status)                       │
│         ├── cmp_mgr.GetCompartment(                                 │
│         │       GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION)     │
│         │   .SetValue(client_id, target_conversion_mode)            │
│         ├── 写回 TsfCommand.result                                  │
│         ├── SetEvent(完成事件)                                      │
│         └── mgr.Deactivate() / CoUninitialize()                     │
│                                                                     │
│       DLL_PROCESS_DETACH 自动 cleanup                               │
└────────────────────────────────────────────────────────────────────┘
```

### 架构原则

1. **一条路径，无分支**：切换只有一个入口 `tsf_dispatch::dispatch`，内部一条流程，没有"如果...就...否则..."的兜底层。
2. **同步语义**：从 Rust 上层看 `dispatch()` 是阻塞的 ≤200ms 调用，返回 `Result`。失败立刻知道，不留时序悬念。
3. **DLL 无状态**：每次注入都是冷启动；卸载后目标进程零痕迹（没有窗口、没有线程、没有 hook）。
4. **HKL 切换保留**：`WM_INPUTLANGCHANGEREQUEST` 路径稳定，不动。问题只在 conversion mode，那部分换 TSF。

### 文件结构变化

```
crates/
  imeswitch-windows/
    src/
      ime/
        mod.rs              ← 精简：删 mode.rs 大部分内容
        layout.rs           ← 保留（HKL 切换稳定）
        detect.rs           ← 保留
        tsf_dispatch.rs     ← 新增（替代 mode.rs 的角色）
        tsf_protocol.rs     ← 新增（共享内存结构体）
    Cargo.toml

  slipkey-tsf-helper/       ← 新增 crate
    src/
      lib.rs                ← DllMain + hook proc
      compartment.rs        ← TSF Compartment 操作
    Cargo.toml              ← crate-type = ["cdylib"]
```

### 删除清单

- `crates/imeswitch-windows/src/ime/mode.rs` 整个文件
- `mod.rs::switch_entry` 中所有 `std::thread::spawn` + `sleep(30ms)` + `set_ime_*_mode` 分支
- 所有 `VK_DBE_*` 模拟键代码
- `REPLAY_MAGIC` 常量及 `hook.rs` 中对应的过滤逻辑
- `keep_ime_open_for_alphanumeric` 这类语言相关的特例函数

## 组件职责

### 1. `imeswitch-windows::ime::tsf_protocol`

**职责**：定义主进程与 helper DLL 之间的共享内存协议。纯类型定义，无任何 Windows API 调用。

**为什么独立成模块**：两边（EXE 和 DLL）都要 `#[repr(C)]` 严格对齐。放在一个文件里避免漂移。

**关键类型**：

```rust
#[repr(C)]
pub struct TsfCommand {
    pub abi_version: u32,
    pub sequence: u32,
    pub target_conversion_mode: u32,   // TF_CONVERSIONMODE_* bitfield
    pub target_open_status: u32,       // 0 = close, 1 = open
    pub result: AtomicU32,             // TsfResult
    pub error_hresult: u32,
}

#[repr(u32)]
pub enum TsfResult {
    Pending = 0,
    Ok = 1,
    Failed = 2,
    AbiMismatch = 3,
}

pub const ABI_VERSION: u32 = 1;

pub fn shared_memory_name(host_pid: u32) -> String {
    format!(r"Local\Slipkey_TSF_v{}_{}", ABI_VERSION, host_pid)
}

pub fn completion_event_name(host_pid: u32, sequence: u32) -> String {
    format!(r"Local\Slipkey_TSF_Done_{}_{}", host_pid, sequence)
}
```

**依赖**：仅 `core::sync::atomic`。

### 2. `imeswitch-windows::ime::tsf_dispatch`

**职责**：从主进程视角把"切到这个语言"翻译为"在目标进程内执行一次 TSF Compartment 写入"。**唯一切换入口**。

**关键接口**：

```rust
pub struct TsfDispatcher {
    next_sequence: AtomicU32,
    helper_dll_path: PathBuf,
}

impl TsfDispatcher {
    pub fn new() -> Result<Self, InitError>;
    pub fn dispatch(&self, target: TsfTarget) -> Result<(), DispatchError>;
}

#[derive(Debug, Clone, Copy)]
pub struct TsfTarget {
    pub conversion_mode: u32,
    pub open_status: bool,
}

#[derive(Debug)]
pub enum DispatchError {
    NoFocusWindow,
    InjectionRefused,
    Timeout,
    HelperFailed { hresult: u32 },
    AbiMismatch,
}
```

**依赖**：`windows-sys`（`SetWindowsHookEx`、`PostThreadMessageW`、`WaitForSingleObject`、`CreateFileMappingW`、`CreateEventW`、`MapViewOfFile`）、`tsf_protocol`。

### 3. `slipkey-tsf-helper`（cdylib）

**职责**：被注入到目标进程后，**在目标 GUI 线程上下文**里执行一次 TSF Compartment 写入。

**导出符号**：仅 `call_wnd_hook`（`extern "system"` CallWndProc）。

**执行流程**（hook proc 首次被调用时）：

1. 从环境变量 `SLIPKEY_TSF_HOST_PID` 取主进程 PID
2. `OpenFileMappingW(shared_memory_name(host_pid))` + `MapViewOfFile`
3. 校验 `abi_version`，不匹配则写 `AbiMismatch` 并 SetEvent
4. `CoInitializeEx(COINIT_APARTMENTTHREADED)`
5. `CoCreateInstance(CLSID_TF_ThreadMgr, IID_ITfThreadMgr)`
6. `mgr.Activate(&mut client_id)`
7. `QueryInterface::<ITfCompartmentMgr>`
8. `GetCompartment(GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)` → `SetValue(open_status)`
9. `GetCompartment(GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION)` → `SetValue(target_conversion_mode)`
10. 写 `cmd.result`，`SetEvent` 完成句柄
11. `mgr.Deactivate()` / `CoUninitialize()`

**依赖**：
- `windows`（COM TSF 接口的 idiomatic 绑定）
- `imeswitch-windows::ime::tsf_protocol`（仅类型）

**为什么用 `windows` 而不是 `windows-sys`**：TSF COM 接口在 `windows` crate 里有自动 IUnknown 引用计数和 `Result<>`。DLL cdylib 会 dead-strip，体积影响 < 5KB。

### 4. `imeswitch-windows::ime::mod`（重写后的 `switch_entry`）

```rust
fn switch_entry(entry: &WinEntry) -> Result<(), SwitchError> {
    // Step 1: HKL 切换（如需要）—— 稳定，不动
    if let Some(hkl_id) = &entry.hkl_id {
        let hwnd = layout::focused_window();
        let hkl = layout::load_or_find_layout(hkl_id)?;
        layout::switch_layout_sync(hwnd, hkl)?;
        layout::broadcast_layout_change(hkl);
    }

    // Step 2: TSF Compartment 写入（如需要）
    match entry.mode {
        WinImeMode::Native | WinImeMode::Alphanumeric => {
            let target = tsf_dispatch::TsfTarget::for_mode(entry.mode, entry.language.as_str());
            if let Err(e) = tsf_dispatch::global().dispatch(target) {
                log::warn!("TSF dispatch failed: {:?} (silent by design)", e);
            }
        }
        WinImeMode::LayoutOnly => {}
    }
    Ok(())
}
```

## 数据流时序（`;en` 从日语切英文）

| 时间 | 主进程 | 目标进程 | OS / TSF |
|------|--------|----------|---------|
| T+0ms | `dispatch(Alphanumeric)` 入口 | — | — |
| T+0ms | `GetFocus` → HWND, `GetWindowThreadProcessId` → TID | — | — |
| T+0ms | 申请 sequence, 写 `TsfCommand` 到共享内存 | — | — |
| T+0ms | `CreateEventW(完成事件)` | — | — |
| T+0ms | `SetEnvironmentVariable("SLIPKEY_TSF_HOST_PID", ...)` | — | — |
| T+0ms | `SetWindowsHookEx(WH_CALLWNDPROC, dll, TID)` | — | OS 把 DLL 映射到目标进程 |
| T+1ms | `PostThreadMessageW(TID, WM_NULL)` | DLL `DllMain` 完成 | — |
| T+2ms | `WaitForSingleObject(done, 200ms)` | hook proc 被回调 | — |
| T+2ms | — | 读共享内存 / 校验 ABI | — |
| T+3ms | — | `CoInitialize` + `ITfThreadMgr::Activate` | — |
| T+4ms | — | `ITfCompartment::SetValue(ALPHANUMERIC)` | TSF 通知 IME 模式变更 |
| T+5ms | — | 写 `result = Ok` / `SetEvent` | — |
| T+5ms | 收到完成事件 / `UnhookWindowsHookEx` | — | — |
| T+6ms | `dispatch` 返回 `Ok(())` | OS 在线程下次空闲时卸载 DLL | — |

**关键不变量**：

- `dispatch` 调用是同步的，调用方拿到结果时 conversion mode 已经在目标 IME 内权威生效
- DLL 在目标进程内的可观察存在时间 < 100ms
- 主进程在等待时只占用一个事件句柄，无任何后台线程

## 错误处理

**两类实际会发生的错误**：

1. **UWP / 系统进程拒绝注入**：`SetWindowsHookEx` 返回 NULL
   - `GetLastError()` 通常是 `ERROR_ACCESS_DENIED` 或 `ERROR_INVALID_HOOK_HANDLE`
   - `DispatchError::InjectionRefused`
   - 行为：静默，写日志 `injection refused for pid={pid} tid={tid} (likely UWP/protected)`

2. **TSF 调用失败**：DLL 写回 `Failed { hresult: 0x... }`
   - 行为：静默，写日志 `tsf failed: hresult=0x{:08X}`

**严格不做**：自动回退到 IMM32 / `WM_IME_CONTROL` / DBE 模拟键。这违反 D5。

**调试支持**：日志路径 `%LOCALAPPDATA%\Slipkey\slipkey.log`，可通过托盘菜单"打开日志目录"快速定位。

## 测试策略

### 单元测试（CI 跑得动）

- `tsf_protocol`
  - `TsfCommand` 的大小和对齐稳定（防止字段顺序意外变动）
  - `ABI_VERSION` 常量等于 1
  - `shared_memory_name` / `completion_event_name` 的命名规则
- `tsf_dispatch`
  - `TsfTarget::for_mode(Native, "ja")` 返回 `NATIVE | FULLSHAPE | ROMAN`
  - `TsfTarget::for_mode(Alphanumeric, "ja")` 返回 `ALPHANUMERIC` + `open=true`
  - `TsfTarget::for_mode(Native, "zh")` 返回 `NATIVE`（不含 FULLSHAPE）
  - 序列号单调递增
- `slipkey-tsf-helper`
  - `call_wnd_hook` 在 `code < 0` 时透传到 `CallNextHookEx`，不执行任何操作
  - 同一进程内被回调多次时，TSF 操作只执行一次（`AtomicBool` 守护）

### 集成测试（Windows VM）

写一个最小 GUI 测试程序：

1. 启动 `slipkey-windows` 主程序
2. 启动测试 GUI 程序，焦点设到其编辑控件
3. 主程序 `dispatch(NATIVE | FULLSHAPE | ROMAN)`
4. 测试程序 100ms 后调用 `ImmGetConversionStatus` 验证
5. 期望：`conversion == NATIVE | FULLSHAPE | ROMAN`

### 真机回归矩阵

每次发布前手动跑，结果归档到 `docs/windows-ime-test-matrix.md`：

| 场景 | 日版 Win11 | 中版 Win11 | 美版 Win10 |
|------|:---------:|:---------:|:---------:|
| `;en` 从日语 → 英文，在 Notepad | ✓ | ✓ | ✓ |
| `;ja` 从英文 → 日语，在 Notepad | ✓ | ✓ | ✓ |
| `;en` 在 VSCode 编辑器 | ✓ | ✓ | ✓ |
| `;en` 在 Chrome 地址栏 | ✓ | ✓ | ✓ |
| `;en` 在 Win11 设置（UWP） | log 静默 | log 静默 | log 静默 |
| 切换后读 conversion mode 验证 | == ALPHA | == ALPHA | == ALPHA |
| 中文 IME 下 `;en` `;zh` 来回切 100 次 | 无脱节 | 无脱节 | n/a |

## 迁移步骤

每步独立可提交、可回滚：

1. **新增** `crates/slipkey-tsf-helper` 空 crate（cdylib 配置 + 最小 `DllMain`）
2. **新增** `imeswitch-windows::ime::tsf_protocol`（纯类型 + 单测）
3. **实现** `slipkey-tsf-helper::compartment` 的 TSF Compartment 写入
4. **实现** `imeswitch-windows::ime::tsf_dispatch::TsfDispatcher`
5. **修改** `ime::mod::switch_entry` 改用 `TsfDispatcher`
6. **删除** `ime/mode.rs` 整个文件
7. **删除** `hook.rs` 中 `REPLAY_MAGIC` 过滤逻辑（不再需要）
8. **修改** `bins/slipkey-windows` 打包脚本：把 `slipkey_tsf.dll` 与 `Slipkey.exe` 一起放进发布 zip
9. **修改** GitHub Actions workflow：构建 + 签名 DLL
10. **更新** README 的 "Architecture" 与 "Build from source" 章节

## 风险与未覆盖项

- **UWP 应用**：注入会被拒绝。可通过 `UIAccess` manifest + 签名 + 安装到 `Program Files` 缓解，但需要 Microsoft Authenticode 签名证书。**本次不做**，未来 polish。
- **AntiVirus 误报**：`SetWindowsHookEx` 是文档化合法 API，但持续注入"短命 DLL"仍可能触发某些 EDR 启发式扫描。降低概率的措施：DLL 名固定、由签名后的 `Slipkey.exe` 加载、不做任何"绕过/隐藏"动作。
- **Wow64 跨架构**：64 位主进程只能注入到 64 位目标进程；32 位 app（如老版 PuTTY）需要额外的 32 位 DLL + 32 位 helper。**本次不做**，老 32 位 app 现今罕见。
- **DLL 签名**：暂时 ad-hoc 签名（与 `Slipkey.exe` 同策略）。

## 一句话回顾

> 现在的问题是在外面拍门、对方爱开不开。
> 新方案是用 Windows 自己给的合法钥匙开门进屋、按一下按钮、出来锁门。门里就一个按钮（TSF Compartment），按下去就是按下去，不存在"按了但没生效"。
