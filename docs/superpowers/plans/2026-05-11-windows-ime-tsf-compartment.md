# Windows IME TSF Compartment 重构 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `slipkey-windows` 的"日语 IME 内部模式切换"从脆弱的 IMM32/DBE 兜底改为权威的 TSF Compartment 写入，通过短命 DLL 注入实现。

**Architecture:** 主进程用 `SetWindowsHookEx(WH_CALLWNDPROC)` 把一个 ~30KB 的 cdylib 临时注入到焦点窗口所在 GUI 线程，DLL 通过 TSF `ITfCompartment` 设置 conversion mode，立即写回共享内存 + SetEvent，主进程拿到结果后 `UnhookWindowsHookEx`。整个往返 < 10ms，DLL 在目标进程内零持久化。

**Tech Stack:** Rust 1.75, Cargo workspace, `windows-sys = "0.61"` (主进程), `windows = "0.61"` (helper DLL TSF 接口), GitHub Actions (Windows 构建), TSF (`ITfThreadMgr` / `ITfCompartmentMgr` / `ITfCompartment`)。

**Spec:** `docs/superpowers/specs/2026-05-11-windows-ime-tsf-compartment-design.md`

**当前分支:** `design/windows-ime-tsf-compartment`

---

## 执行前提

- 一台 Windows 10/11 x64 开发机或 VM（CI 也可，但本地有助于真机回归）
- Rust toolchain `1.75+` with `x86_64-pc-windows-msvc` target
- 已安装日语 + 中文 IME（可在系统设置→时间和语言→语言中添加）

## 任务依赖图

```
T1 (protocol crate)
  ├─→ T2 (helper crate skeleton)
  │     └─→ T3 (hook proc + AtomicBool)
  │           └─→ T4 (TSF Compartment 写入)
  │                 └─→ T5 (TsfTarget)
  │                       └─→ T6 (TsfDispatcher)
  │                             └─→ T7 (重写 switch_entry)
  │                                   └─→ T8 (删除 mode.rs)
  │                                         └─→ T9 (GitHub Actions)
  │                                               └─→ T10 (README)
```

每个任务结束后 `cargo test --workspace` 应保持通过。

---

## Task 1: 创建 `imeswitch-tsf-protocol` micro-crate

**Files:**
- Create: `crates/imeswitch-tsf-protocol/Cargo.toml`
- Create: `crates/imeswitch-tsf-protocol/src/lib.rs`
- Modify: `Cargo.toml` (workspace) — 添加成员

- [ ] **Step 1: 创建 crate 目录与 Cargo.toml**

写入 `crates/imeswitch-tsf-protocol/Cargo.toml`：

```toml
[package]
name = "imeswitch-tsf-protocol"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
```

- [ ] **Step 2: 把新 crate 加入 workspace**

修改根 `Cargo.toml` 的 `members`：

```toml
[workspace]
resolver = "2"
members = [
    "crates/imeswitch-core",
    "crates/imeswitch-tsf-protocol",   # 新增
    "crates/imeswitch-windows",
    "bins/slipkey-windows",
]
```

并在 `[workspace.dependencies]` 追加：

```toml
imeswitch-tsf-protocol = { path = "crates/imeswitch-tsf-protocol" }
```

- [ ] **Step 3: 写失败的单元测试**

写入 `crates/imeswitch-tsf-protocol/src/lib.rs`：

```rust
//! Shared ABI between slipkey-windows (host) and slipkey-tsf-helper (DLL).
//!
//! Both sides #[repr(C)] this exact layout. The ABI_VERSION constant is
//! checked on every dispatch; mismatch causes the helper to write
//! TsfResult::AbiMismatch and bail out.

use std::sync::atomic::AtomicU32;

/// Bump whenever the layout or semantics of `TsfCommand` changes.
pub const ABI_VERSION: u32 = 1;

/// Wait budget for the DLL to write back a result before the host gives up.
pub const DISPATCH_TIMEOUT_MS: u32 = 200;

#[repr(C)]
pub struct TsfCommand {
    pub abi_version: u32,
    pub sequence: u32,
    /// Bitfield: TF_CONVERSIONMODE_* (see msctf.h).
    pub target_conversion_mode: u32,
    /// 0 = close IME, 1 = keep IME open.
    pub target_open_status: u32,
    /// Written by helper. See `TsfResult` for values.
    pub result: AtomicU32,
    /// HRESULT, valid only when result == TsfResult::Failed.
    pub error_hresult: u32,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsfResult {
    Pending = 0,
    Ok = 1,
    Failed = 2,
    AbiMismatch = 3,
}

/// Shared memory name (Local kernel namespace, scoped to host PID).
pub fn shared_memory_name(host_pid: u32) -> String {
    format!(r"Local\Slipkey_TSF_v{}_{}", ABI_VERSION, host_pid)
}

/// Per-dispatch completion event name.
pub fn completion_event_name(host_pid: u32, sequence: u32) -> String {
    format!(r"Local\Slipkey_TSF_Done_{}_{}", host_pid, sequence)
}

/// Environment variable through which the DLL learns the host PID
/// (set by host before SetWindowsHookEx, read by DLL on first hook callback).
pub const HOST_PID_ENV_VAR: &str = "SLIPKEY_TSF_HOST_PID";

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    #[test]
    fn abi_version_is_one() {
        assert_eq!(ABI_VERSION, 1);
    }

    #[test]
    fn command_size_and_alignment_are_stable() {
        // Layout: 6 x u32 = 24 bytes, alignment 4.
        assert_eq!(size_of::<TsfCommand>(), 24);
        assert_eq!(align_of::<TsfCommand>(), 4);
    }

    #[test]
    fn shared_memory_name_includes_pid_and_version() {
        assert_eq!(
            shared_memory_name(1234),
            "Local\\Slipkey_TSF_v1_1234"
        );
    }

    #[test]
    fn completion_event_name_includes_pid_and_sequence() {
        assert_eq!(
            completion_event_name(1234, 7),
            "Local\\Slipkey_TSF_Done_1234_7"
        );
    }

    #[test]
    fn host_pid_env_var_is_stable() {
        assert_eq!(HOST_PID_ENV_VAR, "SLIPKEY_TSF_HOST_PID");
    }
}
```

Note: 用 `std` 而不是 `no_std`。crate 会被 EXE 和 DLL 两边都 link，两边都已是 std crate，no_std 没有体积收益还引入复杂度。

- [ ] **Step 4: 运行测试，确认通过**

```
cargo test -p imeswitch-tsf-protocol
```

期望：5 个 test pass。

- [ ] **Step 5: Commit**

```
git add Cargo.toml crates/imeswitch-tsf-protocol/
git commit -m "新增 imeswitch-tsf-protocol micro-crate (ABI v1)

定义主进程与 helper DLL 间共享内存协议：TsfCommand 结构、
TsfResult 枚举、ABI_VERSION 常量、命名函数、超时常量。
no_std + alloc，零外部依赖。"
```

---

## Task 2: 创建 `slipkey-tsf-helper` cdylib crate 骨架

**Files:**
- Create: `crates/slipkey-tsf-helper/Cargo.toml`
- Create: `crates/slipkey-tsf-helper/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

- [ ] **Step 1: 写 helper crate 的 Cargo.toml**

写入 `crates/slipkey-tsf-helper/Cargo.toml`：

```toml
[package]
name = "slipkey-tsf-helper"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
path = "src/lib.rs"
# cdylib 输出 slipkey_tsf_helper.dll；后续 packaging 时重命名为 slipkey_tsf.dll
crate-type = ["cdylib"]

[dependencies]
imeswitch-tsf-protocol.workspace = true
log.workspace = true

[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_System_Com",
    "Win32_System_Environment",
    "Win32_System_LibraryLoader",
    "Win32_System_Memory",
    "Win32_System_Threading",
    "Win32_UI_TextServices",
    "Win32_UI_WindowsAndMessaging",
] }
```

- [ ] **Step 2: 加入 workspace**

修改根 `Cargo.toml`：

```toml
members = [
    "crates/imeswitch-core",
    "crates/imeswitch-tsf-protocol",
    "crates/imeswitch-windows",
    "crates/slipkey-tsf-helper",       # 新增
    "bins/slipkey-windows",
]
```

- [ ] **Step 3: 写最小 lib.rs（占位 hook proc，能编译通过）**

写入 `crates/slipkey-tsf-helper/src/lib.rs`：

```rust
//! Slipkey TSF helper DLL.
//!
//! Injected briefly into the foreground window's GUI thread via
//! SetWindowsHookEx(WH_CALLWNDPROC). On the first hook callback, this DLL
//! reads a shared-memory command, writes the target TSF conversion mode via
//! ITfCompartment, signals completion, and unloads on UnhookWindowsHookEx.

#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
mod compartment;

#[cfg(target_os = "windows")]
pub use platform::call_wnd_hook;
```

写入 `crates/slipkey-tsf-helper/src/platform.rs`：

```rust
//! Windows-only entry points.

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::CallNextHookEx;

/// CallWndProc hook procedure. Exported for `SetWindowsHookEx` to discover via
/// GetProcAddress when the host passes our HMODULE.
#[no_mangle]
pub unsafe extern "system" fn call_wnd_hook(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Step 1 only: pass through. Real logic added in Task 3.
    CallNextHookEx(None, code, wparam, lparam)
}
```

注意 `cfg(target_os = "windows")` 让非 Windows 平台跳过 platform/compartment 模块，本地（macOS）`cargo build --workspace` 不会失败；跨平台通用的测试可以放在 lib.rs 根。

- [ ] **Step 4: 验证编译通过**

```
cargo build -p slipkey-tsf-helper --target x86_64-pc-windows-msvc
```

如果在 macOS/Linux 开发机上，跳过 target，仅检查 `cargo check -p slipkey-tsf-helper`（cfg 守护下应能通过）。

Windows 上期望：`target/x86_64-pc-windows-msvc/debug/slipkey_tsf_helper.dll` 存在。

- [ ] **Step 5: Commit**

```
git add Cargo.toml crates/slipkey-tsf-helper/
git commit -m "新增 slipkey-tsf-helper cdylib 骨架

cdylib 配置 + 最小 call_wnd_hook（仅透传）。
依赖 windows crate 的 TSF/COM 接口模块。"
```

---

## Task 3: helper 中添加 "只执行一次" 守护与触发分支

**Files:**
- Modify: `crates/slipkey-tsf-helper/src/lib.rs`
- Modify: `crates/slipkey-tsf-helper/Cargo.toml` — 暂时不需要

- [ ] **Step 1: 写失败测试（守护逻辑）**

在 `crates/slipkey-tsf-helper/src/lib.rs` 文件底部添加（**lib.rs 根**，跨平台都能跑）：

```rust
/// First-call-only guard. Returns true on the first call, false thereafter.
/// Exposed at crate root so tests can verify the contract on any host.
pub fn first_call_only(flag: &std::sync::atomic::AtomicBool) -> bool {
    !flag.swap(true, std::sync::atomic::Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::first_call_only;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn first_call_returns_true_subsequent_calls_return_false() {
        let flag = AtomicBool::new(false);
        assert!(first_call_only(&flag));
        assert!(!first_call_only(&flag));
        assert!(!first_call_only(&flag));
    }
}
```

运行 `cargo test -p slipkey-tsf-helper`。期望：1 个测试 pass。在 macOS/Linux 上也能跑（守护逻辑不依赖 Windows API）。

- [ ] **Step 2: 实现 call_wnd_hook 的守护与触发**

替换 `crates/slipkey-tsf-helper/src/platform.rs` 中的 `call_wnd_hook`：

```rust
use std::sync::atomic::AtomicBool;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::CallNextHookEx;

use crate::{compartment, first_call_only};

static EXECUTED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub unsafe extern "system" fn call_wnd_hook(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Windows convention: code < 0 means "must call CallNextHookEx and do nothing".
    if code >= 0 && first_call_only(&EXECUTED) {
        // Best-effort: log+swallow any panic so hook never propagates into the
        // target process. compartment::execute_once is implemented in Task 4.
        let result = std::panic::catch_unwind(|| compartment::execute_once());
        if let Err(panic) = result {
            log::error!("slipkey_tsf_helper panic suppressed: {:?}", panic);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
```

并新建 `crates/slipkey-tsf-helper/src/compartment.rs` 空 stub（Task 4 会填实现）：

```rust
//! TSF Compartment write logic. Runs inside the target GUI thread.
//!
//! Read shared TsfCommand → set OPENCLOSE + INPUTMODE_CONVERSION compartments
//! → write result → SetEvent.

/// Called exactly once per DLL injection from `call_wnd_hook`.
pub fn execute_once() {
    // Filled in Task 4. Empty stub here so Task 3 compiles standalone.
}
```

- [ ] **Step 3: 验证编译与测试**

```
cargo test -p slipkey-tsf-helper
```

期望：1 个测试通过（`first_call_returns_true_subsequent_calls_return_false`，跨平台）。Windows 上额外编译 `platform.rs` 和 `compartment.rs`。

- [ ] **Step 4: Commit**

```
git add crates/slipkey-tsf-helper/
git commit -m "helper: 添加 hook proc 单次触发守护

AtomicBool 确保 TSF 操作只执行一次；
catch_unwind 防止 panic 跨越 DLL 边界进入目标进程；
compartment.rs 添加空 stub。"
```

---

## Task 4: 实现 TSF Compartment 写入

**Files:**
- Modify: `crates/slipkey-tsf-helper/src/compartment.rs`

- [ ] **Step 1: 完整实现 `execute_once`**

替换 `crates/slipkey-tsf-helper/src/compartment.rs` 全部内容：

```rust
//! TSF Compartment write logic. Runs inside the target GUI thread.

use std::sync::atomic::Ordering;

use imeswitch_tsf_protocol::{
    completion_event_name, shared_memory_name, TsfCommand, TsfResult, ABI_VERSION,
    HOST_PID_ENV_VAR,
};
use windows::core::{Result, GUID, PCWSTR, VARIANT};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Environment::GetEnvironmentVariableW;
use windows::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS,
};
use windows::Win32::System::Threading::{OpenEventW, SetEvent, EVENT_MODIFY_STATE};
use windows::Win32::UI::TextServices::{
    ITfCompartment, ITfCompartmentMgr, ITfThreadMgr, CLSID_TF_ThreadMgr,
    GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION, GUID_COMPARTMENT_KEYBOARD_OPENCLOSE,
};

pub fn execute_once() {
    if let Err(e) = try_execute() {
        log::error!("tsf execute failed: {:?}", e);
    }
}

fn try_execute() -> Result<()> {
    let host_pid = read_host_pid().ok_or_else(|| windows::core::Error::from_win32())?;

    let shm_name = wide(&shared_memory_name(host_pid));
    let shm_handle = unsafe {
        OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, false, PCWSTR(shm_name.as_ptr()))
    }?;
    let view = unsafe { MapViewOfFile(shm_handle, FILE_MAP_ALL_ACCESS, 0, 0, std::mem::size_of::<TsfCommand>()) };
    if view.Value.is_null() {
        unsafe { let _ = CloseHandle(shm_handle); };
        return Err(windows::core::Error::from_win32());
    }
    let cmd_ptr = view.Value as *mut TsfCommand;
    let cmd = unsafe { &*cmd_ptr };

    // ABI check (helper may be older/newer than host).
    if cmd.abi_version != ABI_VERSION {
        cmd.result.store(TsfResult::AbiMismatch as u32, Ordering::SeqCst);
        signal_done(host_pid, cmd.sequence);
        cleanup(view, shm_handle);
        return Ok(());
    }

    let target_mode = cmd.target_conversion_mode;
    let target_open = cmd.target_open_status != 0;
    let sequence = cmd.sequence;

    // Do TSF work; capture HRESULT on failure.
    let tsf_result = do_tsf_write(target_open, target_mode);
    match tsf_result {
        Ok(()) => cmd.result.store(TsfResult::Ok as u32, Ordering::SeqCst),
        Err(e) => {
            // SAFETY: error_hresult is a plain u32 field; benign data race window
            // before result store is fine — caller only reads it when result==Failed.
            unsafe {
                let mutable: *mut TsfCommand = cmd_ptr;
                (*mutable).error_hresult = e.code().0 as u32;
            }
            cmd.result.store(TsfResult::Failed as u32, Ordering::SeqCst);
        }
    }

    signal_done(host_pid, sequence);
    cleanup(view, shm_handle);
    Ok(())
}

fn do_tsf_write(open: bool, conversion_mode: u32) -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    let _guard = ComGuard;

    let thread_mgr: ITfThreadMgr = unsafe { CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER) }?;
    let mut client_id = 0u32;
    unsafe { thread_mgr.Activate(&mut client_id) }?;

    let cmp_mgr: ITfCompartmentMgr = thread_mgr.cast()?;

    // 1) OpenStatus
    let open_cmp: ITfCompartment = unsafe { cmp_mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE) }?;
    let v_open = i32_variant(if open { 1 } else { 0 });
    unsafe { open_cmp.SetValue(client_id, &v_open) }?;

    // 2) Conversion mode
    let conv_cmp: ITfCompartment = unsafe { cmp_mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION) }?;
    let v_conv = i32_variant(conversion_mode as i32);
    unsafe { conv_cmp.SetValue(client_id, &v_conv) }?;

    unsafe { thread_mgr.Deactivate() }?;
    Ok(())
}

fn i32_variant(value: i32) -> VARIANT {
    // VARIANT initialization helper — windows crate's VARIANT supports From<i32>.
    VARIANT::from(value)
}

struct ComGuard;
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

fn read_host_pid() -> Option<u32> {
    let name = wide(HOST_PID_ENV_VAR);
    let mut buf = vec![0u16; 32];
    let len = unsafe { GetEnvironmentVariableW(PCWSTR(name.as_ptr()), Some(&mut buf)) };
    if len == 0 || (len as usize) >= buf.len() {
        return None;
    }
    let s = String::from_utf16(&buf[..len as usize]).ok()?;
    s.trim().parse::<u32>().ok()
}

fn signal_done(host_pid: u32, sequence: u32) {
    let name = wide(&completion_event_name(host_pid, sequence));
    unsafe {
        if let Ok(handle) = OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) {
            let _ = SetEvent(handle);
            let _ = CloseHandle(handle);
        }
    }
}

fn cleanup(view: MEMORY_MAPPED_VIEW_ADDRESS, handle: HANDLE) {
    unsafe {
        let _ = UnmapViewOfFile(view);
        let _ = CloseHandle(handle);
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
```

- [ ] **Step 2: 验证编译**

```
cargo check -p slipkey-tsf-helper --target x86_64-pc-windows-msvc
```

可能会遇到的常见 `windows` crate features 缺失或者类型名漂移，需要按编译器提示补 features 或调整 import 路径。期望最终编译干净。

- [ ] **Step 3: Commit**

```
git add crates/slipkey-tsf-helper/
git commit -m "helper: 实现 TSF Compartment 写入

读共享内存 TsfCommand，CoCreateInstance(CLSID_TF_ThreadMgr)，
依次设置 OPENCLOSE + INPUTMODE_CONVERSION compartments，
写回 result，SetEvent。失败时记录 HRESULT。"
```

---

## Task 5: 实现 `TsfTarget::for_mode`

**Files:**
- Create: `crates/imeswitch-windows/src/ime/tsf_dispatch.rs`
- Modify: `crates/imeswitch-windows/src/ime/mod.rs` — 添加 `mod tsf_dispatch;`
- Modify: `crates/imeswitch-windows/Cargo.toml` — 添加 `imeswitch-tsf-protocol` 依赖

- [ ] **Step 1: 添加依赖**

修改 `crates/imeswitch-windows/Cargo.toml`，在 `[dependencies]` 块追加：

```toml
imeswitch-tsf-protocol.workspace = true
```

在 `[target.'cfg(target_os = "windows")'.dependencies]` 的 `windows-sys` features 列表追加：

```toml
    "Win32_System_Threading",
    "Win32_System_Memory",
    "Win32_System_Environment",
    "Win32_Security",
```

完整 features 列表应是：

```toml
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_Globalization",
    "Win32_Security",
    "Win32_System_Environment",
    "Win32_System_LibraryLoader",
    "Win32_System_Memory",
    "Win32_System_Threading",
    "Win32_UI_Input_Ime",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_WindowsAndMessaging",
] }
```

- [ ] **Step 2: 写失败测试**

写入 `crates/imeswitch-windows/src/ime/tsf_dispatch.rs`：

```rust
//! Host-side TSF dispatch: inject helper DLL, signal it, wait for completion.

use crate::ime::WinImeMode;

/// TSF conversion mode bits — mirror values from `<msctf.h>`.
pub const TF_CONVERSIONMODE_ALPHANUMERIC: u32 = 0x0000;
pub const TF_CONVERSIONMODE_NATIVE: u32 = 0x0001;
pub const TF_CONVERSIONMODE_FULLSHAPE: u32 = 0x0008;
pub const TF_CONVERSIONMODE_ROMAN: u32 = 0x0010;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsfTarget {
    pub conversion_mode: u32,
    pub open_status: bool,
}

impl TsfTarget {
    /// Translate a (mode, language) pair into the TSF Compartment values.
    pub fn for_mode(mode: WinImeMode, language: &str) -> Option<Self> {
        match mode {
            WinImeMode::Alphanumeric => Some(Self {
                conversion_mode: TF_CONVERSIONMODE_ALPHANUMERIC,
                // Keep the IME active; only switch its internal mode.
                // This is the decision D1 from the design doc.
                open_status: true,
            }),
            WinImeMode::Native => Some(Self {
                conversion_mode: match language {
                    // Japanese needs full-shape + Roman input style for "ja kana via romaji".
                    "ja" => TF_CONVERSIONMODE_NATIVE | TF_CONVERSIONMODE_FULLSHAPE | TF_CONVERSIONMODE_ROMAN,
                    // Chinese: just native. No full-shape forcing.
                    _ => TF_CONVERSIONMODE_NATIVE,
                },
                open_status: true,
            }),
            // LayoutOnly bypasses TSF entirely (e.g. French AZERTY).
            WinImeMode::LayoutOnly => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphanumeric_keeps_ime_open_and_uses_zero_mode() {
        let t = TsfTarget::for_mode(WinImeMode::Alphanumeric, "ja").unwrap();
        assert_eq!(t.conversion_mode, TF_CONVERSIONMODE_ALPHANUMERIC);
        assert!(t.open_status, "must keep IME open (design D1)");
    }

    #[test]
    fn native_japanese_uses_native_fullshape_roman() {
        let t = TsfTarget::for_mode(WinImeMode::Native, "ja").unwrap();
        assert_eq!(
            t.conversion_mode,
            TF_CONVERSIONMODE_NATIVE | TF_CONVERSIONMODE_FULLSHAPE | TF_CONVERSIONMODE_ROMAN
        );
        assert!(t.open_status);
    }

    #[test]
    fn native_chinese_uses_native_only() {
        let t = TsfTarget::for_mode(WinImeMode::Native, "zh").unwrap();
        assert_eq!(t.conversion_mode, TF_CONVERSIONMODE_NATIVE);
        assert!(t.open_status);
    }

    #[test]
    fn native_korean_uses_native_only() {
        let t = TsfTarget::for_mode(WinImeMode::Native, "ko").unwrap();
        assert_eq!(t.conversion_mode, TF_CONVERSIONMODE_NATIVE);
    }

    #[test]
    fn layout_only_returns_none() {
        assert!(TsfTarget::for_mode(WinImeMode::LayoutOnly, "fr").is_none());
    }
}
```

- [ ] **Step 3: 在 `mod.rs` 中声明子模块**

修改 `crates/imeswitch-windows/src/ime/mod.rs`，找到现有 `pub mod` 行，在合适位置追加：

```rust
pub mod tsf_dispatch;
```

（与 `pub mod detect; pub mod layout; pub mod mode;` 同行。注意 `mode` 还在；Task 8 才删。）

- [ ] **Step 4: 运行测试**

```
cargo test -p imeswitch-windows tsf_dispatch
```

期望：5 个测试通过。

- [ ] **Step 5: Commit**

```
git add Cargo.toml crates/imeswitch-windows/
git commit -m "tsf_dispatch: 添加 TsfTarget 与 for_mode 翻译

把 WinImeMode + language 翻译为 TF_CONVERSIONMODE_* + open_status；
Alphanumeric 保持 IME 打开（D1），Native 对 ja 加 FULLSHAPE|ROMAN，
对 zh/ko 只用 NATIVE；LayoutOnly 跳过 TSF。"
```

---

## Task 6: 实现 `TsfDispatcher::new` 与 `dispatch`

**Files:**
- Modify: `crates/imeswitch-windows/src/ime/tsf_dispatch.rs`

- [ ] **Step 1: 扩展 tsf_dispatch.rs，添加 dispatcher 主结构**

在 `crates/imeswitch-windows/src/ime/tsf_dispatch.rs` 文件 **顶部** 添加：

```rust
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use imeswitch_tsf_protocol::{
    completion_event_name, shared_memory_name, TsfCommand, TsfResult, ABI_VERSION,
    DISPATCH_TIMEOUT_MS, HOST_PID_ENV_VAR,
};
```

并在文件内 `impl TsfTarget` 之后追加：

```rust
#[derive(Debug)]
pub enum DispatchError {
    DllNotFound(PathBuf),
    NoFocusWindow,
    InjectionRefused(u32),
    Timeout,
    HelperFailed { hresult: u32 },
    AbiMismatch,
    System(u32),
}

pub struct TsfDispatcher {
    next_sequence: AtomicU32,
    helper_dll_path: PathBuf,
}

impl TsfDispatcher {
    pub fn new() -> Result<Self, DispatchError> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .ok_or(DispatchError::System(0))?;
        let dll = exe_dir.join("slipkey_tsf.dll");
        if !dll.exists() {
            return Err(DispatchError::DllNotFound(dll));
        }
        Ok(Self {
            next_sequence: AtomicU32::new(1),
            helper_dll_path: dll,
        })
    }

    pub fn dispatch(&self, target: TsfTarget) -> Result<(), DispatchError> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = target;
            return Err(DispatchError::System(0));
        }

        #[cfg(target_os = "windows")]
        {
            platform::dispatch_impl(
                &self.helper_dll_path,
                self.next_sequence.fetch_add(1, Ordering::SeqCst),
                target,
            )
        }
    }
}

/// Lazily-initialized global dispatcher. Returns None if the helper DLL is
/// missing from the install directory — caller logs and skips the TSF step.
pub fn global() -> Option<&'static TsfDispatcher> {
    static INSTANCE: OnceLock<Option<TsfDispatcher>> = OnceLock::new();
    INSTANCE
        .get_or_init(|| match TsfDispatcher::new() {
            Ok(d) => Some(d),
            Err(e) => {
                log::warn!("TsfDispatcher disabled: {:?}", e);
                None
            }
        })
        .as_ref()
}
```

- [ ] **Step 2: 添加 Windows 平台实现 module**

在 `tsf_dispatch.rs` 文件底部追加（保留现有的 `#[cfg(test)] mod tests`）：

```rust
#[cfg(target_os = "windows")]
mod platform {
    use super::*;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
    use windows_sys::Win32::System::Environment::SetEnvironmentVariableW;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
        MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
    };
    use windows_sys::Win32::System::Threading::{
        CreateEventW, WaitForSingleObject, INFINITE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, PostThreadMessageW,
        SetWindowsHookExW, UnhookWindowsHookEx, GUITHREADINFO, WH_CALLWNDPROC, WM_NULL,
    };

    pub(super) fn dispatch_impl(
        helper_dll_path: &std::path::Path,
        sequence: u32,
        target: TsfTarget,
    ) -> Result<(), DispatchError> {
        let tid = focused_thread_id().ok_or(DispatchError::NoFocusWindow)?;
        let host_pid = std::process::id();

        // 1. Allocate shared memory for the TsfCommand.
        let shm_name = wide(&shared_memory_name(host_pid));
        let cmd_size = std::mem::size_of::<TsfCommand>();
        let shm_handle = unsafe {
            CreateFileMappingW(
                std::ptr::null_mut(),
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                cmd_size as u32,
                shm_name.as_ptr(),
            )
        };
        if shm_handle.is_null() {
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }
        let view = unsafe { MapViewOfFile(shm_handle, FILE_MAP_ALL_ACCESS, 0, 0, cmd_size) };
        if view.Value.is_null() {
            unsafe { let _ = CloseHandle(shm_handle); };
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }
        let cmd_ptr = view.Value as *mut TsfCommand;

        // 2. Fill the command.
        unsafe {
            std::ptr::write(
                cmd_ptr,
                TsfCommand {
                    abi_version: ABI_VERSION,
                    sequence,
                    target_conversion_mode: target.conversion_mode,
                    target_open_status: if target.open_status { 1 } else { 0 },
                    result: std::sync::atomic::AtomicU32::new(TsfResult::Pending as u32),
                    error_hresult: 0,
                },
            );
        }

        // 3. Create the completion event.
        let event_name = wide(&completion_event_name(host_pid, sequence));
        let event_handle = unsafe {
            CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr())
        };
        if event_handle.is_null() {
            unsafe { let _ = UnmapViewOfFile(view); let _ = CloseHandle(shm_handle); };
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }

        // 4. Tell the DLL (via env var) which PID owns the shared memory.
        unsafe {
            SetEnvironmentVariableW(
                wide(HOST_PID_ENV_VAR).as_ptr(),
                wide(&host_pid.to_string()).as_ptr(),
            );
        }

        // 5. Load the helper DLL into our own process (required for SetWindowsHookEx).
        let helper_hmod = unsafe {
            LoadLibraryW(wide(&helper_dll_path.to_string_lossy()).as_ptr())
        };
        if helper_hmod.is_null() {
            cleanup(view, shm_handle, event_handle);
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }

        // 6. Resolve hook proc address inside the helper module.
        let hook_proc = unsafe {
            windows_sys::Win32::System::LibraryLoader::GetProcAddress(
                helper_hmod,
                b"call_wnd_hook\0".as_ptr(),
            )
        };
        if hook_proc.is_none() {
            cleanup(view, shm_handle, event_handle);
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }

        // 7. Install the hook on the foreground GUI thread.
        let hook = unsafe {
            SetWindowsHookExW(WH_CALLWNDPROC, Some(std::mem::transmute(hook_proc)), helper_hmod, tid)
        };
        if hook.is_null() {
            let err = unsafe { GetLastError() };
            cleanup(view, shm_handle, event_handle);
            return Err(DispatchError::InjectionRefused(err));
        }

        // 8. Wake up the target thread's message pump so it dispatches CallWndProc.
        unsafe {
            let _ = PostThreadMessageW(tid, WM_NULL, 0, 0);
        }

        // 9. Wait for the helper to signal completion.
        let wait_rc = unsafe { WaitForSingleObject(event_handle, DISPATCH_TIMEOUT_MS) };

        // 10. Unhook immediately regardless of outcome (DLL self-cleans on detach).
        unsafe { let _ = UnhookWindowsHookEx(hook); };

        let outcome = match wait_rc {
            WAIT_OBJECT_0 => {
                let cmd_ref = unsafe { &*cmd_ptr };
                match cmd_ref.result.load(Ordering::SeqCst) {
                    v if v == TsfResult::Ok as u32 => Ok(()),
                    v if v == TsfResult::AbiMismatch as u32 => Err(DispatchError::AbiMismatch),
                    v if v == TsfResult::Failed as u32 => {
                        Err(DispatchError::HelperFailed { hresult: cmd_ref.error_hresult })
                    }
                    _ => Err(DispatchError::Timeout),
                }
            }
            WAIT_TIMEOUT => Err(DispatchError::Timeout),
            _ => Err(DispatchError::System(unsafe { GetLastError() })),
        };

        cleanup(view, shm_handle, event_handle);
        outcome
    }

    fn focused_thread_id() -> Option<u32> {
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.is_null() {
                return None;
            }
            let mut info: GUITHREADINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
            let fg_tid = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
            if fg_tid == 0 {
                return None;
            }
            if GetGUIThreadInfo(fg_tid, &mut info) != 0 && !info.hwndFocus.is_null() {
                let focused_tid = GetWindowThreadProcessId(info.hwndFocus, std::ptr::null_mut());
                if focused_tid != 0 {
                    return Some(focused_tid);
                }
            }
            Some(fg_tid)
        }
    }

    fn cleanup(view: MEMORY_MAPPED_VIEW_ADDRESS, shm: HANDLE, event: HANDLE) {
        unsafe {
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(shm);
            let _ = CloseHandle(event);
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}
```

- [ ] **Step 2 (continued): 验证编译**

```
cargo check -p imeswitch-windows --target x86_64-pc-windows-msvc
```

可能要按编译器提示微调 `windows-sys` 的 type / signature。可能需要 `as i32` 等。

- [ ] **Step 3: 添加同步性单测（仅 non-windows host 跑得通的部分）**

在 `tsf_dispatch.rs` 的 `#[cfg(test)] mod tests` 中追加：

```rust
    #[test]
    fn dispatch_error_is_debug() {
        // Just compile-asserts that all variants implement Debug,
        // catches accidental removal of #[derive(Debug)].
        let e = DispatchError::NoFocusWindow;
        let _ = format!("{:?}", e);
    }
```

跑：

```
cargo test -p imeswitch-windows tsf_dispatch
```

期望：6 个测试通过（5 个 for_mode + 1 个 debug）。

- [ ] **Step 4: Commit**

```
git add crates/imeswitch-windows/
git commit -m "tsf_dispatch: 实现 TsfDispatcher::dispatch 完整流程

CreateFileMapping + CreateEvent + SetWindowsHookEx(WH_CALLWNDPROC) +
PostThreadMessage(WM_NULL) + WaitForSingleObject(200ms) +
UnhookWindowsHookEx；按 outcome 返回 DispatchError 各变体。
global() 提供 OnceLock 单例，DLL 缺失时优雅降级。"
```

---

## Task 7: 重写 `ime::mod::switch_entry` 使用 dispatcher

**Files:**
- Modify: `crates/imeswitch-windows/src/ime/mod.rs`

- [ ] **Step 1: 读取当前 switch_entry**

```
sed -n '/^#\[cfg(target_os = "windows")\]$/,/^#\[cfg(not(target_os = "windows"))\]$/p' crates/imeswitch-windows/src/ime/mod.rs
```

预期看到约 30 行的现有 `fn switch_entry` 实现，包含 `WinImeMode::Alphanumeric` 的 `std::thread::spawn` + `WinImeMode::Native` 的 sleep + `mode::set_ime_*` 调用。

- [ ] **Step 2: 替换 switch_entry**

把 `mod.rs` 中 `#[cfg(target_os = "windows")] fn switch_entry` 整个函数替换为：

```rust
#[cfg(target_os = "windows")]
fn switch_entry(entry: &WinEntry) -> Result<(), SwitchError> {
    // Step 1: HKL switch (stable, unchanged from before).
    if let Some(hkl_id) = entry.hkl_id.as_deref() {
        let hwnd = layout::focused_window();
        let hkl = layout::load_or_find_layout(hkl_id)?;
        layout::switch_layout_sync(hwnd, hkl)?;
        layout::broadcast_layout_change(hkl);
    }

    // Step 2: TSF Compartment write (Native / Alphanumeric only).
    if let Some(target) = tsf_dispatch::TsfTarget::for_mode(entry.mode, entry.language.as_str()) {
        if let Some(dispatcher) = tsf_dispatch::global() {
            if let Err(e) = dispatcher.dispatch(target) {
                // Silent by design (decision D6): log and move on.
                log::warn!(
                    "TSF dispatch failed for lang={} mode={:?}: {:?}",
                    entry.language.as_str(),
                    entry.mode,
                    e
                );
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 3: 验证编译**

```
cargo build -p imeswitch-windows --target x86_64-pc-windows-msvc
```

可能会得到 "unused import: mode" 等警告 — Task 8 会清理。

- [ ] **Step 4: 运行整 workspace 测试**

```
cargo test --workspace
```

期望：所有现有测试 + 新增 TsfTarget 测试通过。

- [ ] **Step 5: Commit**

```
git add crates/imeswitch-windows/src/ime/mod.rs
git commit -m "switch_entry: 改用 TsfDispatcher 走 TSF Compartment

删除 std::thread::spawn + 30ms sleep + set_ime_*_mode 调用；
HKL 切换路径保留；TSF dispatch 失败时静默 + log。"
```

---

## Task 8: 删除 `mode.rs` 与无用 import

**Files:**
- Delete: `crates/imeswitch-windows/src/ime/mode.rs`
- Modify: `crates/imeswitch-windows/src/ime/mod.rs`

- [ ] **Step 1: 删除 mod.rs 中 mode 子模块声明**

在 `crates/imeswitch-windows/src/ime/mod.rs` 找到这一行并删除：

```rust
pub mod mode;
```

- [ ] **Step 2: 检查 mod.rs 是否还有 mode:: 引用**

```
grep -n "mode::" crates/imeswitch-windows/src/ime/mod.rs
```

期望：无输出。如果有残留，删除对应行。

- [ ] **Step 3: 删除文件**

```
rm crates/imeswitch-windows/src/ime/mode.rs
```

- [ ] **Step 4: 检查 hook.rs 的 REPLAY_MAGIC 仍然存在（replay 路径需要）**

```
grep -n "REPLAY_MAGIC" crates/imeswitch-windows/src/hook.rs
```

期望：3 处命中（常量定义 + 过滤 + replay 时 SendInput）。**不要动 hook.rs。**

- [ ] **Step 5: 验证编译与测试**

```
cargo test --workspace
```

期望：编译干净（无 warning），所有测试通过。

- [ ] **Step 6: Commit**

```
git add -A crates/imeswitch-windows/
git commit -m "删除 mode.rs：IMM32/DBE 路径退役

完全切到 TSF Compartment（D5）。hook.rs 中的 REPLAY_MAGIC
保留，给状态机的 replay 路径用。"
```

---

## Task 9: 修改 GitHub Actions 把 helper DLL 打入发布 zip

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: 修改 windows job 的 build 步骤**

把 `release.yml` 中 windows job 的两步 (`Test Rust workspace` + `Build Slipkey.exe`) 之间或之后改为：

```yaml
      - name: Test Rust workspace
        run: cargo test --workspace

      - name: Build Slipkey.exe and helper DLL
        run: |
          cargo build --release -p slipkey-windows
          cargo build --release -p slipkey-tsf-helper

      - name: Package Windows artifact
        shell: pwsh
        run: |
          $version = $env:GITHUB_REF_NAME.TrimStart("v")
          New-Item -ItemType Directory -Force -Path dist | Out-Null
          Copy-Item target/release/Slipkey.exe dist/Slipkey.exe
          Copy-Item target/release/slipkey_tsf_helper.dll dist/slipkey_tsf.dll
          Compress-Archive `
            -Path dist/Slipkey.exe, dist/slipkey_tsf.dll `
            -DestinationPath "dist/Slipkey-$version-windows-x64.zip" `
            -Force
```

注意 cdylib 输出名称是 `slipkey_tsf_helper.dll`（基于 crate name），打包时重命名为 `slipkey_tsf.dll`（这是 `TsfDispatcher::new` 期望的）。

- [ ] **Step 2: 本地干跑（如有 Windows 环境）**

```
cargo build --release -p slipkey-windows
cargo build --release -p slipkey-tsf-helper
ls target/release/Slipkey.exe
ls target/release/slipkey_tsf_helper.dll
```

期望：两个文件都存在。

- [ ] **Step 3: Commit**

```
git add .github/workflows/release.yml
git commit -m "CI: 构建并打包 slipkey_tsf.dll 到 Windows zip

新增 cargo build --release -p slipkey-tsf-helper；
打包时 slipkey_tsf_helper.dll → slipkey_tsf.dll；
zip 同时包含 Slipkey.exe 与 slipkey_tsf.dll。"
```

---

## Task 10: 更新 README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: 找到 Architecture 章节**

```
grep -n "^## " README.md
```

定位到 `## 架构` / `## Architecture` 段落（中英双语）。

- [ ] **Step 2: 更新 Architecture 的 bins / crates 块**

把现有的 `crates/` 部分替换为：

```text
crates/
  imeswitch-core/         Pure-Rust state machine shared by platform apps
  imeswitch-tsf-protocol/ Shared ABI between Slipkey.exe and slipkey_tsf.dll
  imeswitch-windows/      Windows hook + HKL + TSF dispatch
  slipkey-tsf-helper/     Short-lived cdylib injected into focused GUI
                          thread for authoritative TSF Compartment writes
```

并在该章节末尾添加一段（中英文版本都要加）：

中文：

```
### Windows IME 切换的两段式

切换 `;ja` / `;en` 时分两步：

1. **HKL 切换** — 通过 `WM_INPUTLANGCHANGEREQUEST` 改变焦点窗口的键盘布局
2. **TSF Compartment 写入** — 把 `slipkey_tsf.dll` 短暂注入焦点 GUI 线程（`SetWindowsHookEx(WH_CALLWNDPROC)`），在该线程内通过 `ITfCompartment` 设置 `GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION`。注入持续 < 10ms，做完即卸。

第 2 步在 UWP / 受保护进程中可能被拒绝，此时静默降级（仅写日志），HKL 已切换、IME 内部模式不变。
```

英文：

```
### Two-Stage Switching on Windows

`;ja` / `;en` triggers two operations:

1. **HKL switch** — `WM_INPUTLANGCHANGEREQUEST` to the focused window
2. **TSF Compartment write** — `slipkey_tsf.dll` is briefly injected into the focused GUI thread via `SetWindowsHookEx(WH_CALLWNDPROC)`; inside that thread we set `GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION` through `ITfCompartment`. The injection lasts <10ms and unloads immediately.

Step 2 may be refused for UWP / protected processes; in that case we silently log and continue (HKL has already switched).
```

- [ ] **Step 3: 更新 Build from source 章节**

把 Windows 的 `cargo build --release -p slipkey-windows ...` 替换为：

```bash
cargo build --release -p slipkey-windows --target x86_64-pc-windows-msvc
cargo build --release -p slipkey-tsf-helper --target x86_64-pc-windows-msvc
```

并在产物路径中追加：

```text
target/x86_64-pc-windows-msvc/release/Slipkey.exe
target/x86_64-pc-windows-msvc/release/slipkey_tsf_helper.dll  (rename to slipkey_tsf.dll)
```

- [ ] **Step 4: Commit**

```
git add README.md
git commit -m "README: 文档化 TSF Compartment 注入架构

新增 imeswitch-tsf-protocol 与 slipkey-tsf-helper crate；
解释 HKL + TSF Compartment 的两段式切换；
更新 Build from source 命令与产物。"
```

---

## 验收检查

完成全部 10 个任务后：

- [ ] `cargo test --workspace` 通过（无 warning）
- [ ] `cargo build --release -p slipkey-windows -p slipkey-tsf-helper --target x86_64-pc-windows-msvc` 产出 EXE + DLL
- [ ] DLL 大小 < 200KB（debug 可能更大；release 应在 30–100KB 范围）
- [ ] 在 Windows 真机上手动跑 spec §测试策略 的回归矩阵
- [ ] 切换后 `ImmGetConversionStatus` 读出的值与预期一致
- [ ] UWP 进程下 `;en` 走 silent fallback、日志有记录

## 已知后续工作（不在本计划范围）

- UWP / AppContainer 注入：需要 `UIAccess` manifest + Authenticode 签名
- 32 位目标进程支持：需要 32 位 helper DLL
- 集成测试自动化：需要 Windows VM 在 CI 跑端到端的 IME 状态读写验证
- DLL Authenticode 签名：跟 `Slipkey.exe` 同策略

## 一句话回顾

> 10 个 task 把"在外面拍门"换成"用合法钥匙开门按按钮再锁门"。每个 task 独立可提交、可回滚；最终代码库比改前更短、更聚焦。
