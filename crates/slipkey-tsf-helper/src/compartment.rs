//! TSF Compartment write logic. Runs inside the target GUI thread.
//!
//! Read shared TsfCommand -> set OPENCLOSE + INPUTMODE_CONVERSION compartments
//! -> write result -> SetEvent.

/// Called exactly once per DLL injection from `call_wnd_hook`.
pub fn execute_once() {
    // Filled in Task 4. Empty stub here so Task 3 compiles standalone.
}
