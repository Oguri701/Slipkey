//! TSF Compartment write logic. Runs inside the target GUI thread.

use std::sync::atomic::Ordering;

use imeswitch_tsf_protocol::{
    completion_event_name, shared_memory_name, TsfCommand, TsfResult, ABI_VERSION,
    HOST_PID_ENV_VAR,
};
use windows::core::{Interface, Result, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Environment::GetEnvironmentVariableW;
use windows::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    MEMORY_MAPPED_VIEW_ADDRESS,
};
use windows::Win32::System::Threading::{OpenEventW, SetEvent, EVENT_MODIFY_STATE};
use windows::Win32::System::Variant::VARIANT;
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
    let host_pid = read_host_pid().ok_or_else(windows::core::Error::from_win32)?;

    let shm_name = wide(&shared_memory_name(host_pid));
    let shm_handle =
        unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, false, PCWSTR(shm_name.as_ptr())) }?;
    let view = unsafe {
        MapViewOfFile(
            shm_handle,
            FILE_MAP_ALL_ACCESS,
            0,
            0,
            std::mem::size_of::<TsfCommand>(),
        )
    };
    if view.Value.is_null() {
        unsafe {
            let _ = CloseHandle(shm_handle);
        };
        return Err(windows::core::Error::from_win32());
    }
    let cmd_ptr = view.Value as *mut TsfCommand;
    let cmd = unsafe { &*cmd_ptr };

    if cmd.abi_version != ABI_VERSION {
        cmd.result
            .store(TsfResult::AbiMismatch as u32, Ordering::SeqCst);
        signal_done(host_pid, cmd.sequence);
        cleanup(view, shm_handle);
        return Ok(());
    }

    let target_mode = cmd.target_conversion_mode;
    let target_open = cmd.target_open_status != 0;
    let sequence = cmd.sequence;

    let tsf_result = do_tsf_write(target_open, target_mode);
    match tsf_result {
        Ok(()) => cmd.result.store(TsfResult::Ok as u32, Ordering::SeqCst),
        Err(e) => {
            unsafe {
                (*cmd_ptr).error_hresult = e.code().0 as u32;
            }
            cmd.result
                .store(TsfResult::Failed as u32, Ordering::SeqCst);
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

    let thread_mgr: ITfThreadMgr =
        unsafe { CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER) }?;
    let client_id = unsafe { thread_mgr.Activate() }?;

    let cmp_mgr: ITfCompartmentMgr = thread_mgr.cast()?;

    let open_cmp: ITfCompartment =
        unsafe { cmp_mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE) }?;
    let v_open = i32_variant(if open { 1 } else { 0 });
    unsafe { open_cmp.SetValue(client_id, &v_open) }?;

    let conv_cmp: ITfCompartment = unsafe {
        cmp_mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION)
    }?;
    let v_conv = i32_variant(conversion_mode as i32);
    unsafe { conv_cmp.SetValue(client_id, &v_conv) }?;

    unsafe { thread_mgr.Deactivate() }?;
    Ok(())
}

fn i32_variant(value: i32) -> VARIANT {
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
