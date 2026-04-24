//! Minimal libdispatch (GCD) FFI: defer a closure to the main queue.
//!
//! We only need this to kick the `TISSelectInputSource` call out of the
//! CGEventTap callback. Calling TIS synchronously inside the callback
//! races with the system's `kTISNotifySelectedKeyboardInputSourceChanged`
//! notification — observers (focused-app input contexts) may not have
//! refreshed by the time the next keystroke arrives.

use std::os::raw::c_void;

#[repr(C)]
struct OpaqueQueue {
    _unused: [u8; 0],
}

extern "C" {
    /// The global main dispatch queue symbol. `dispatch_get_main_queue()`
    /// is a C macro that expands to `&_dispatch_main_q`.
    static _dispatch_main_q: OpaqueQueue;

    fn dispatch_async_f(
        queue: *const OpaqueQueue,
        context: *mut c_void,
        work: unsafe extern "C" fn(*mut c_void),
    );
}

/// Schedule `f` to run on the main dispatch queue (== main thread's runloop).
pub fn async_main<F: FnOnce() + Send + 'static>(f: F) {
    unsafe extern "C" fn trampoline<F: FnOnce()>(ctx: *mut c_void) {
        let boxed: Box<F> = unsafe { Box::from_raw(ctx as *mut F) };
        boxed();
    }
    let boxed: Box<F> = Box::new(f);
    let ctx = Box::into_raw(boxed) as *mut c_void;
    unsafe {
        dispatch_async_f(&_dispatch_main_q, ctx, trampoline::<F>);
    }
}
