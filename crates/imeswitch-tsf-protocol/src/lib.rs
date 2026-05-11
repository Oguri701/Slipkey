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

/// Stable shared memory name used by the injected DLL to discover the host PID.
pub fn host_pid_memory_name() -> String {
    format!(r"Local\Slipkey_TSF_HostPid_v{}", ABI_VERSION)
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
        assert_eq!(shared_memory_name(1234), "Local\\Slipkey_TSF_v1_1234");
    }

    #[test]
    fn host_pid_memory_name_includes_version() {
        assert_eq!(host_pid_memory_name(), "Local\\Slipkey_TSF_HostPid_v1");
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
