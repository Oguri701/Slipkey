# 单文件分发：嵌入 helper DLL 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `slipkey_tsf.dll` 改为编译期嵌入到 `Slipkey.exe`，首次启动释放到 `%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll`。最终用户只看到一个 `Slipkey.exe`，不会因为误删 DLL 而坏掉 TSF 切换。

**Architecture:** 用 Rust `include_bytes!` 在编译时把 helper DLL 字节嵌入 EXE → 启动时 `dll_provisioning::ensure_helper_dll()` 检查 / 写出 / 校验 SHA256 → 把绝对路径通过 `tsf_dispatch::set_helper_dll_path` 注入到全局 `TsfDispatcher`。CI 调整 build 顺序：先 build helper，cp DLL 到 `embed/`，再 build slipkey-windows。

**Tech Stack:** Rust `include_bytes!`, `sha2` (DLL 完整性校验), `std::env::var("LOCALAPPDATA")`, build.rs (preflight check), GitHub Actions (build orchestration)。

**前置 spec:** 无独立 spec；本计划自包含。背景见 `docs/superpowers/specs/2026-05-11-windows-ime-tsf-compartment-design.md`（TSF 重构架构）。

**当前分支：** `design/windows-ime-tsf-compartment`（与 TSF 重构同分支，作为后续 polish）。

---

## 任务依赖图

```
T1 (embed 目录 + gitignore)
  └─→ T2 (build.rs preflight)
        └─→ T3 (dll_provisioning 模块)
              └─→ T4 (TsfDispatcher 接受路径参数)
                    └─→ T5 (main.rs 早期 provision + 注入路径)
                          └─→ T6 (CI workflow 调整)
                                └─→ T7 (README 更新)
```

---

## Task 1: 准备 `embed/` 目录与 `.gitignore`

**Files:**
- Create: `bins/slipkey-windows/embed/.gitignore`

- [ ] **Step 1: 创建目录与 gitignore**

```bash
mkdir -p bins/slipkey-windows/embed
```

写入 `bins/slipkey-windows/embed/.gitignore`：

```
# DLL 字节由 CI build 流程生成，不入仓库
*.dll
```

(只 ignore 二进制文件本身，`.gitignore` 自身仍被 git 追踪，确保目录在仓库里存在。)

- [ ] **Step 2: Commit**

```bash
git add bins/slipkey-windows/embed/.gitignore
git commit -m "新增 embed 目录用于 helper DLL 编译期嵌入

CI build 时把 slipkey_tsf_helper.dll 拷到此目录；
slipkey-windows 的 include_bytes! 从这里读取。
DLL 文件本身被 gitignore，仅保留 .gitignore 占位。"
```

---

## Task 2: `build.rs` preflight 检查

**Files:**
- Create: `bins/slipkey-windows/build.rs`
- Modify: `bins/slipkey-windows/Cargo.toml`

- [ ] **Step 1: 写 build.rs**

写入 `bins/slipkey-windows/build.rs`：

```rust
//! Preflight: verify embed/slipkey_tsf.dll exists before compiling main.rs.
//!
//! The DLL is produced by the slipkey-tsf-helper crate and copied here by the
//! release workflow / local build script. Without it, include_bytes!() in
//! dll_provisioning.rs would fail with a cryptic error.

use std::path::Path;

fn main() {
    let dll_path = Path::new("embed/slipkey_tsf.dll");
    println!("cargo:rerun-if-changed=embed/slipkey_tsf.dll");

    if !dll_path.exists() {
        eprintln!();
        eprintln!("❌ Missing: bins/slipkey-windows/embed/slipkey_tsf.dll");
        eprintln!();
        eprintln!("   Build the helper crate first, then copy the DLL into embed/:");
        eprintln!();
        eprintln!("     cargo build --release -p slipkey-tsf-helper \\");
        eprintln!("       --target x86_64-pc-windows-msvc");
        eprintln!("     cp target/x86_64-pc-windows-msvc/release/slipkey_tsf_helper.dll \\");
        eprintln!("       bins/slipkey-windows/embed/slipkey_tsf.dll");
        eprintln!();
        eprintln!("   (On Windows PowerShell: Copy-Item with backslash paths.)");
        eprintln!();
        std::process::exit(1);
    }
}
```

- [ ] **Step 2: 在 Cargo.toml 声明 build.rs**

修改 `bins/slipkey-windows/Cargo.toml`，在 `[package]` 块下方追加（紧跟 `rust-version.workspace`）：

```toml
build = "build.rs"
```

完整 `[package]` 块应是：

```toml
[package]
name = "slipkey-windows"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
build = "build.rs"
```

- [ ] **Step 3: 本地验证（必须先准备好 DLL 才能 cargo check）**

在 Windows 上：

```bash
cargo build --release -p slipkey-tsf-helper --target x86_64-pc-windows-msvc
cp target/x86_64-pc-windows-msvc/release/slipkey_tsf_helper.dll \
   bins/slipkey-windows/embed/slipkey_tsf.dll
cargo check -p slipkey-windows --target x86_64-pc-windows-msvc
```

在 macOS / Linux 上：build.rs 仍会跑，但因为 DLL 缺失会 exit(1)。可以手动 `touch bins/slipkey-windows/embed/slipkey_tsf.dll` 创建空文件让 cargo check 通过。

期望（Windows 准备好 DLL 后）：cargo check 干净通过。

- [ ] **Step 4: Commit**

```bash
git add bins/slipkey-windows/build.rs bins/slipkey-windows/Cargo.toml
git commit -m "build.rs: 编译前校验 embed/slipkey_tsf.dll 存在

include_bytes! 直接读取 embed/，缺失时给出明确的 build 指引；
避免出现 'file not found' 这类难以理解的 macro 错误。"
```

---

## Task 3: `dll_provisioning` 模块（核心逻辑）

**Files:**
- Create: `bins/slipkey-windows/src/dll_provisioning.rs`
- Modify: `bins/slipkey-windows/src/main.rs` — 添加 `mod dll_provisioning;`（实际调用在 Task 5）
- Modify: `bins/slipkey-windows/Cargo.toml` — 添加 `sha2` 依赖

- [ ] **Step 1: 添加 sha2 依赖**

修改 `bins/slipkey-windows/Cargo.toml` 的 `[dependencies]` 块追加：

```toml
sha2 = "0.10"
```

- [ ] **Step 2: 写失败测试**

新建 `bins/slipkey-windows/src/dll_provisioning.rs`，先写测试 + 桩函数：

```rust
//! Provision the bundled helper DLL into a stable location at runtime so
//! tray-app users do not need to keep an external slipkey_tsf.dll next to
//! Slipkey.exe.
//!
//! Layout:
//!   %LOCALAPPDATA%\Slipkey\slipkey_tsf.dll  ← target
//!   <embedded bytes from bins/slipkey-windows/embed/slipkey_tsf.dll>
//!
//! On every launch we hash the on-disk DLL with SHA256 and compare with the
//! hash of EMBEDDED_DLL. Mismatch (first run, upgrade, tampering) triggers a
//! rewrite. Equal: leave the file as-is, return path.

use std::path::{Path, PathBuf};

/// SHA256 of `bytes`, returned as a 32-byte array.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Decide whether to write a new copy of the DLL.
/// Returns true if the file is missing, unreadable, or its hash differs.
pub fn needs_rewrite(target: &Path, expected_hash: &[u8; 32]) -> bool {
    match std::fs::read(target) {
        Ok(existing) => &sha256(&existing) != expected_hash,
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn sha256_of_empty_input_is_known() {
        // SHA256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let h = sha256(b"");
        assert_eq!(
            hex::encode(h),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn needs_rewrite_when_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.dll");
        assert!(needs_rewrite(&path, &sha256(b"hello")));
    }

    #[test]
    fn needs_rewrite_when_hash_differs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("file.dll");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"world").unwrap();
        drop(f);
        assert!(needs_rewrite(&path, &sha256(b"hello")));
    }

    #[test]
    fn no_rewrite_when_hash_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("file.dll");
        std::fs::write(&path, b"hello").unwrap();
        assert!(!needs_rewrite(&path, &sha256(b"hello")));
    }
}
```

测试需要的 dev-dependencies：在 `bins/slipkey-windows/Cargo.toml` 加：

```toml
[dev-dependencies]
hex = "0.4"
tempfile = "3"
```

- [ ] **Step 3: 运行测试，确认通过**

```bash
cargo test -p slipkey-windows dll_provisioning
```

期望：4 个测试通过。

- [ ] **Step 4: 实现真正的 ensure_helper_dll**

在 `bins/slipkey-windows/src/dll_provisioning.rs` 文件**顶部 use 之后**追加：

```rust
/// The helper DLL bytes baked into the EXE at compile time.
pub const EMBEDDED_DLL: &[u8] = include_bytes!("../embed/slipkey_tsf.dll");

/// File name written under %LOCALAPPDATA%\Slipkey\.
const DLL_FILE_NAME: &str = "slipkey_tsf.dll";

#[derive(Debug)]
pub enum ProvisionError {
    NoLocalAppData,
    Io(std::io::Error),
}

impl std::fmt::Display for ProvisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLocalAppData => write!(f, "LOCALAPPDATA environment variable is unset"),
            Self::Io(e) => write!(f, "filesystem error: {}", e),
        }
    }
}

impl From<std::io::Error> for ProvisionError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Provision the helper DLL into `%LOCALAPPDATA%\Slipkey\` if needed and return
/// its absolute path. Called once at startup, before any TSF dispatch is wired.
pub fn ensure_helper_dll() -> Result<PathBuf, ProvisionError> {
    let dir = local_app_data()?.join("Slipkey");
    std::fs::create_dir_all(&dir)?;

    let path = dir.join(DLL_FILE_NAME);
    let expected = sha256(EMBEDDED_DLL);

    if needs_rewrite(&path, &expected) {
        // Atomic-ish write: temp file → rename.
        let tmp = dir.join(format!("{DLL_FILE_NAME}.tmp"));
        std::fs::write(&tmp, EMBEDDED_DLL)?;
        // Best-effort remove of any half-written destination from a prior crash.
        let _ = std::fs::remove_file(&path);
        std::fs::rename(&tmp, &path)?;
        log::info!("provisioned helper DLL: {}", path.display());
    } else {
        log::debug!("helper DLL up to date: {}", path.display());
    }

    Ok(path)
}

fn local_app_data() -> Result<PathBuf, ProvisionError> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or(ProvisionError::NoLocalAppData)
}
```

- [ ] **Step 5: 在 main.rs 声明 mod（实际调用在 Task 5）**

修改 `bins/slipkey-windows/src/main.rs` 顶部 `mod` 块：

```rust
mod app;
mod dll_provisioning;
mod hook_thread;
mod startup;
mod tray;
mod ui;
```

- [ ] **Step 6: cargo check 验证**

```bash
cargo check -p slipkey-windows --target x86_64-pc-windows-msvc
```

期望：编译通过。`ensure_helper_dll` 暂时未被调用，会有"function never used"警告，Task 5 会消除。

- [ ] **Step 7: Commit**

```bash
git add bins/slipkey-windows/Cargo.toml bins/slipkey-windows/src/dll_provisioning.rs bins/slipkey-windows/src/main.rs
git commit -m "dll_provisioning: SHA256 校验 + 释放 helper DLL 到 LOCALAPPDATA

包含 EMBEDDED_DLL (include_bytes!)、sha256/needs_rewrite 工具、
ensure_helper_dll 入口。原子写：写 tmp → rename。
单测覆盖 hash 与 needs_rewrite 决策。"
```

---

## Task 4: `TsfDispatcher` 接受 DLL 路径

**Files:**
- Modify: `crates/imeswitch-windows/src/ime/tsf_dispatch.rs`

- [ ] **Step 1: 读当前 TsfDispatcher::new 与 global**

```bash
grep -n "fn new\|fn global\|next_sequence\|DLL_PATH" crates/imeswitch-windows/src/ime/tsf_dispatch.rs
```

记下当前 `TsfDispatcher::new()` 的实现位置与 `global()` 函数。

- [ ] **Step 2: 添加 DLL 路径注入点 + 改造 new**

在 `tsf_dispatch.rs` 文件中（在 `TsfDispatcher` impl 之前）添加：

```rust
use std::sync::OnceLock as StdOnceLock;

/// Static slot for the resolved helper DLL path. Must be `set()` by the host
/// app before the first call to `tsf_dispatch::global()`.
static DLL_PATH: StdOnceLock<PathBuf> = StdOnceLock::new();

/// Inject the path that `TsfDispatcher` should use when injecting the helper
/// DLL via `SetWindowsHookEx`. Call this from `main()` after provisioning.
pub fn set_helper_dll_path(path: PathBuf) {
    let _ = DLL_PATH.set(path);
}
```

并把 `TsfDispatcher::new()` 改造为 `TsfDispatcher::new_with_path(path: PathBuf)`：

```rust
impl TsfDispatcher {
    pub fn new_with_path(helper_dll_path: PathBuf) -> Result<Self, DispatchError> {
        if !helper_dll_path.exists() {
            return Err(DispatchError::DllNotFound(helper_dll_path));
        }
        Ok(Self {
            next_sequence: AtomicU32::new(1),
            helper_dll_path,
        })
    }

    // ... existing dispatch() method unchanged ...
}
```

**删除**老的 `TsfDispatcher::new` 实现（它从 `current_exe().parent()` 找 DLL，本任务后不再需要）。

- [ ] **Step 3: 改造 global() 使用 DLL_PATH**

替换现有的 `pub fn global()`：

```rust
pub fn global() -> Option<&'static TsfDispatcher> {
    static INSTANCE: StdOnceLock<Option<TsfDispatcher>> = StdOnceLock::new();
    INSTANCE
        .get_or_init(|| {
            let path = match DLL_PATH.get().cloned() {
                Some(p) => p,
                None => {
                    log::warn!(
                        "TsfDispatcher disabled: helper DLL path not set \
                         (call set_helper_dll_path() from main before first dispatch)"
                    );
                    return None;
                }
            };
            match TsfDispatcher::new_with_path(path) {
                Ok(d) => Some(d),
                Err(e) => {
                    log::warn!("TsfDispatcher disabled: {:?}", e);
                    None
                }
            }
        })
        .as_ref()
}
```

- [ ] **Step 4: cargo check**

```bash
cargo check -p imeswitch-windows --target x86_64-pc-windows-msvc
```

期望：编译通过。

- [ ] **Step 5: 跑 workspace 测试**

```bash
cargo test --workspace
```

期望：现有测试不被破坏（`TsfDispatcher::new_with_path` 的 path 检查只在 path 不存在时报错，单测路径不命中）。

- [ ] **Step 6: Commit**

```bash
git add crates/imeswitch-windows/src/ime/tsf_dispatch.rs
git commit -m "tsf_dispatch: 接受外部注入的 helper DLL 路径

新增 set_helper_dll_path + TsfDispatcher::new_with_path；
global() 改为从 DLL_PATH OnceLock 读取，未设置时静默禁用。
为单文件分发铺路：host 自行 provision DLL 后注入路径。"
```

---

## Task 5: `main.rs` 早期 provision + 注入路径

**Files:**
- Modify: `bins/slipkey-windows/src/main.rs`

- [ ] **Step 1: 修改 main 函数顶部**

把 `main()` 里 `env_logger::Builder::...init()` 之后、`let state = ...` 之前，插入：

```rust
    // Provision the bundled helper DLL into %LOCALAPPDATA%\Slipkey\ before
    // wiring TSF dispatch. Without this, IME switching falls back to "HKL only"
    // silently (which is degraded behavior for Japanese alphanumeric mode).
    match dll_provisioning::ensure_helper_dll() {
        Ok(path) => {
            imeswitch_windows::ime::tsf_dispatch::set_helper_dll_path(path);
        }
        Err(e) => {
            log::error!(
                "helper DLL provisioning failed: {} \
                 (Japanese alphanumeric mode will be degraded)",
                e
            );
        }
    }
```

- [ ] **Step 2: 验证 imeswitch_windows::ime::tsf_dispatch 可达性**

```bash
grep -n "pub mod\|pub use" crates/imeswitch-windows/src/lib.rs
grep -n "pub mod\|pub use" crates/imeswitch-windows/src/ime/mod.rs
```

如果 `tsf_dispatch` 未在 `ime/mod.rs` 中 `pub` 导出，加：

```rust
pub mod tsf_dispatch;
```

应该已经是 `pub mod tsf_dispatch;`（Task 5 of TSF refactor 已加）。如果不是，补上。

- [ ] **Step 3: cargo check**

```bash
cargo check -p slipkey-windows --target x86_64-pc-windows-msvc
```

期望：编译通过；之前 Task 3 的 `ensure_helper_dll never used` 警告消失。

- [ ] **Step 4: Commit**

```bash
git add bins/slipkey-windows/src/main.rs crates/imeswitch-windows/src/ime/mod.rs
git commit -m "main: 启动早期 provision helper DLL + 注入到 TsfDispatcher

env_logger 初始化后立即调用 ensure_helper_dll；
成功 → set_helper_dll_path 注入到全局 dispatcher；
失败 → 记录 error，TSF 切换将自动降级（不影响 HKL 切换）。"
```

---

## Task 6: GitHub Actions 调整 build 顺序 + zip 内容

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: 修改 windows job 的 Build + Package 步骤**

把 windows job 中现有的 "Build Slipkey.exe and helper DLL" + "Package Windows artifact" 替换为：

```yaml
      - name: Test Rust workspace
        run: cargo test --workspace

      - name: Build helper DLL
        run: cargo build --release -p slipkey-tsf-helper --target x86_64-pc-windows-msvc

      - name: Stage helper DLL into embed/
        shell: pwsh
        run: |
          New-Item -ItemType Directory -Force -Path bins/slipkey-windows/embed | Out-Null
          Copy-Item `
            target/x86_64-pc-windows-msvc/release/slipkey_tsf_helper.dll `
            bins/slipkey-windows/embed/slipkey_tsf.dll `
            -Force

      - name: Build Slipkey.exe (with embedded DLL)
        run: cargo build --release -p slipkey-windows --target x86_64-pc-windows-msvc

      - name: Package Windows artifact (single file)
        shell: pwsh
        run: |
          $version = $env:GITHUB_REF_NAME.TrimStart("v")
          New-Item -ItemType Directory -Force -Path dist | Out-Null
          Copy-Item target/x86_64-pc-windows-msvc/release/Slipkey.exe dist/Slipkey.exe
          Compress-Archive `
            -Path dist/Slipkey.exe `
            -DestinationPath "dist/Slipkey-$version-windows-x64.zip" `
            -Force
```

关键变化：
- build 顺序：先 helper，再 slipkey-windows
- 显式 `Stage helper DLL into embed/` 步骤
- zip 里**只包含** `Slipkey.exe`（不再有 `slipkey_tsf.dll`）
- 都用 `--target x86_64-pc-windows-msvc` 显式指定（保持 release artifact 一致）

- [ ] **Step 2: workflow_dispatch 触发一次测试（推荐）**

如果你能跑 GitHub Actions：手动触发一次 release 流程，或本地 push 一个 test tag `v0.1.5-rc1`，看 workflow 是否绿色 + 产物正确。

或者本地按相同顺序跑一遍：

```powershell
cargo build --release -p slipkey-tsf-helper --target x86_64-pc-windows-msvc
Copy-Item target/x86_64-pc-windows-msvc/release/slipkey_tsf_helper.dll bins/slipkey-windows/embed/slipkey_tsf.dll -Force
cargo build --release -p slipkey-windows --target x86_64-pc-windows-msvc
Get-Item target/x86_64-pc-windows-msvc/release/Slipkey.exe | Select-Object Name, Length
```

期望：`Slipkey.exe` 体积比之前大 ~30-100KB（嵌入了 DLL）。

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "CI: 单文件分发——zip 只含 Slipkey.exe

build 顺序：helper crate → cp 到 embed/ → slipkey-windows。
Slipkey.exe 通过 include_bytes! 嵌入 DLL 字节；
首次启动时 dll_provisioning 释放到 %LOCALAPPDATA%\\Slipkey\\。"
```

---

## Task 7: 更新 README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: 修改 Windows Install 段落**

定位中文 "## Windows 使用" → "### 安装"，替换：

```markdown
### 安装

1. 从 [Releases](https://github.com/Oguri701/Slipkey/releases/latest) 下载 `Slipkey-*-windows-x64.zip`
2. 解压得到 `Slipkey.exe`（**单文件**，可以放任意位置）
3. 双击 `Slipkey.exe` 运行
4. 托盘区域会出现 Slipkey 图标
5. 右键托盘图标，打开 **Settings** 配置快捷键
6. 在 **General** 页可以启用开机启动

> 首次运行时 Slipkey 会在 `%LOCALAPPDATA%\Slipkey\` 下创建一个 `slipkey_tsf.dll`
> 辅助文件，用于在 TSF 层切换 Microsoft 日语 IME 的内部模式。Slipkey 自己管理
> 这个文件——你不需要手动操作。
>
> Windows SmartScreen 在首次运行时可能弹出 "Windows protected your PC" 提示。
> 这是因为目前 Slipkey 还没有 Code Signing 证书。点 **More info** → **Run anyway** 即可。
```

英文 "## Windows" → "### Install" 同步：

```markdown
### Install

1. Download `Slipkey-*-windows-x64.zip` from [Releases](https://github.com/Oguri701/Slipkey/releases/latest)
2. Unzip — you get a single `Slipkey.exe` (put it anywhere)
3. Double-click `Slipkey.exe`
4. Right-click the tray icon and open **Settings**
5. Configure shortcuts in **Shortcuts**
6. Enable **Launch at login** in **General** if desired

> On first run, Slipkey provisions a helper file at
> `%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll` for TSF-level IME mode switching
> (specifically for Microsoft Japanese IME's alphanumeric mode). Slipkey
> manages this file automatically.
>
> Windows SmartScreen may show "Windows protected your PC" on first run because
> Slipkey is not yet code-signed. Click **More info** → **Run anyway**.
```

- [ ] **Step 2: 修改 "Build from source" 段落**

中文：

```markdown
### 从源码构建

```bash
# 1. 先构建 helper DLL
cargo build --release -p slipkey-tsf-helper --target x86_64-pc-windows-msvc

# 2. 把 DLL 拷到 embed 目录（include_bytes! 编译期读取）
cp target/x86_64-pc-windows-msvc/release/slipkey_tsf_helper.dll \
   bins/slipkey-windows/embed/slipkey_tsf.dll

# 3. 构建 Slipkey.exe（DLL 字节会被嵌入）
cargo build --release -p slipkey-windows --target x86_64-pc-windows-msvc
```

构建产物：

```text
target/x86_64-pc-windows-msvc/release/Slipkey.exe   ← 单文件，可直接分发
```
```

英文同步翻译。

- [ ] **Step 3: 修改 Architecture 章节中 crates 描述（如果之前提到 DLL 是独立文件）**

把 `slipkey-tsf-helper` 的描述改为：

```text
slipkey-tsf-helper/  Short-lived cdylib injected into focused GUI thread.
                     Bundled into Slipkey.exe via include_bytes! and
                     provisioned to %LOCALAPPDATA%\Slipkey\ at first launch.
```

- [ ] **Step 4: 修改 Uninstall 段落**

中文：

```markdown
### 卸载

1. 通过托盘菜单退出 Slipkey
2. 删除 `Slipkey.exe`
3. 删除 `%APPDATA%\imeswitch\`（配置文件）
4. 删除 `%LOCALAPPDATA%\Slipkey\`（运行时辅助文件）
5. 如启用了开机启动，可删除注册表项：`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Slipkey`
```

英文同步。

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "README: 单文件分发说明 + 卸载步骤 + SmartScreen 提示

Install 改为'解压得到一个 Slipkey.exe'；
Build from source 拆成 helper → embed → exe 三步；
Uninstall 增加 %LOCALAPPDATA%\\Slipkey\\ 清理；
新增 SmartScreen 警告应对说明。"
```

---

## 验收检查

完成 7 个 Task 后：

- [ ] `cargo test --workspace` 全过
- [ ] 本地（Windows）按 Task 6 step 2 的命令序列 build，产出单文件 `Slipkey.exe`
- [ ] zip 里**只有** `Slipkey.exe`，没有 `slipkey_tsf.dll`
- [ ] 拷贝 `Slipkey.exe` 到一个**完全空**的目录运行：
  - 启动正常
  - `%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll` 被创建
  - `;en` / `;ja` 切换正常工作
- [ ] 删除 `%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll` 后再次启动 → DLL 被重新写出
- [ ] 修改 `%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll` 内容（hexedit 改一字节）后再次启动 → 检测 hash 不匹配 → DLL 被覆盖回正确版本
- [ ] 托盘 Open Settings / Quit 仍然正常
- [ ] git push origin design/windows-ime-tsf-compartment 成功

## 已知后续工作（不在本计划范围）

- **Code Signing**：用 EV 证书或 SignPath.io（OSS 免费）签 EXE 消除 SmartScreen 警告
- **自动更新**：检测 GitHub Releases 新版本提示用户
- **winget 上架**：让有经验的用户用 `winget install` 完全绕过 SmartScreen
- **DLL 版本号**：当前用 SHA256 检测变更；未来可改成 ABI version + semver 更优雅

## 一句话回顾

> 把"用户必须不删 DLL"这个永远会出问题的约束，换成"Slipkey 自己负责释放和校验"。普通用户只看到一个 EXE。
