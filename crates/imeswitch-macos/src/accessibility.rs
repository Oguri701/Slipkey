use core_foundation_sys::base::{kCFAllocatorDefault, kCFAllocatorNull, CFTypeRef};
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::number::kCFBooleanTrue;
use core_foundation_sys::string::{CFStringCreateWithCStringNoCopy, kCFStringEncodingUTF8};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

extern "C" {
    fn CFDictionaryCreate(
        allocator: core_foundation_sys::base::CFAllocatorRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        num_values: isize,
        key_callbacks: *const std::ffi::c_void,
        value_callbacks: *const std::ffi::c_void,
    ) -> CFDictionaryRef;
    fn CFRelease(cf: CFTypeRef);
}

pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Returns true if Accessibility is already granted.
/// If not granted, triggers the macOS permission dialog pointing the user
/// to System Settings → Privacy & Security → Accessibility.
pub fn request_accessibility_permission() -> bool {
    unsafe {
        let key = CFStringCreateWithCStringNoCopy(
            kCFAllocatorDefault,
            b"AXTrustedCheckOptionPrompt\0".as_ptr() as *const _,
            kCFStringEncodingUTF8,
            kCFAllocatorNull,
        );
        if key.is_null() {
            return AXIsProcessTrusted();
        }
        let value = kCFBooleanTrue as CFTypeRef;
        let keys = [key as CFTypeRef];
        let values = [value];
        let dict = CFDictionaryCreate(
            kCFAllocatorDefault,
            keys.as_ptr(),
            values.as_ptr(),
            1,
            std::ptr::null(),
            std::ptr::null(),
        );
        let trusted = if dict.is_null() {
            AXIsProcessTrusted()
        } else {
            let result = AXIsProcessTrustedWithOptions(dict);
            CFRelease(dict as CFTypeRef);
            result
        };
        CFRelease(key as CFTypeRef);
        trusted
    }
}
