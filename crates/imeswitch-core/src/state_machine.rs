use crate::types::{Key, Language};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// If true, the hook must NOT forward the current key to the OS.
    pub suppress: bool,
    /// Previously-suppressed keys to synth-post back into the event stream
    /// (in order) BEFORE the current key is handled.
    pub replay: Vec<Key>,
    /// If set, the hook should switch the IME after handling this key.
    pub switch: Option<Language>,
}

impl Response {
    fn pass() -> Self {
        Self { suppress: false, replay: Vec::new(), switch: None }
    }
    fn suppress() -> Self {
        Self { suppress: true, replay: Vec::new(), switch: None }
    }
    fn switch(lang: Language) -> Self {
        Self { suppress: true, replay: Vec::new(), switch: Some(lang) }
    }
    fn cancel(replay: Vec<Key>) -> Self {
        Self { suppress: false, replay, switch: None }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum State {
    Idle,
    AfterLeader,
    /// Saw `;e`, expecting `n`.
    AfterE,
    /// Saw `;j`, expecting `a`.
    AfterJ,
    /// Saw `;z`, expecting `h`.
    AfterZ,
}

#[derive(Debug)]
pub struct StateMachine {
    state: State,
}

impl StateMachine {
    pub fn new() -> Self {
        Self { state: State::Idle }
    }

    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    /// Called on a physical keydown. The caller (platform hook) is responsible
    /// for acting on the returned `Response`.
    pub fn on_keydown(&mut self, key: Key) -> Response {
        match (self.state, key) {
            // Start a sequence.
            (State::Idle, Key::Leader) => {
                self.state = State::AfterLeader;
                Response::suppress()
            }
            // Anything else at idle: pass through.
            (State::Idle, _) => Response::pass(),

            // After leader, pick a language branch.
            (State::AfterLeader, Key::E) => {
                self.state = State::AfterE;
                Response::suppress()
            }
            (State::AfterLeader, Key::J) => {
                self.state = State::AfterJ;
                Response::suppress()
            }
            (State::AfterLeader, Key::Z) => {
                self.state = State::AfterZ;
                Response::suppress()
            }
            // Second `;` while waiting — treat as restart.
            (State::AfterLeader, Key::Leader) => {
                // Flush the first `;`, start fresh with this one.
                self.state = State::AfterLeader;
                Response { suppress: true, replay: vec![Key::Leader], switch: None }
            }
            // Any other key: sequence aborted; replay `;` and let current pass.
            (State::AfterLeader, _) => {
                self.state = State::Idle;
                Response::cancel(vec![Key::Leader])
            }

            // Complete triggers.
            (State::AfterE, Key::N) => {
                self.state = State::Idle;
                Response::switch(Language::En)
            }
            (State::AfterJ, Key::A) => {
                self.state = State::Idle;
                Response::switch(Language::Jp)
            }
            (State::AfterZ, Key::H) => {
                self.state = State::Idle;
                Response::switch(Language::Zh)
            }

            // Second `;` mid-sequence: flush what we had, restart.
            (State::AfterE, Key::Leader) => {
                self.state = State::AfterLeader;
                Response { suppress: true, replay: vec![Key::Leader, Key::E], switch: None }
            }
            (State::AfterJ, Key::Leader) => {
                self.state = State::AfterLeader;
                Response { suppress: true, replay: vec![Key::Leader, Key::J], switch: None }
            }
            (State::AfterZ, Key::Leader) => {
                self.state = State::AfterLeader;
                Response { suppress: true, replay: vec![Key::Leader, Key::Z], switch: None }
            }

            // Broken sequence: replay buffered, pass current.
            (State::AfterE, _) => {
                self.state = State::Idle;
                Response::cancel(vec![Key::Leader, Key::E])
            }
            (State::AfterJ, _) => {
                self.state = State::Idle;
                Response::cancel(vec![Key::Leader, Key::J])
            }
            (State::AfterZ, _) => {
                self.state = State::Idle;
                Response::cancel(vec![Key::Leader, Key::Z])
            }
        }
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(sm: &mut StateMachine, keys: &[Key]) -> Response {
        let mut last = Response::pass();
        for k in keys {
            last = sm.on_keydown(*k);
        }
        last
    }

    #[test]
    fn idle_passthrough() {
        let mut sm = StateMachine::new();
        let r = sm.on_keydown(Key::E);
        assert!(!r.suppress);
        assert!(r.replay.is_empty());
        assert!(r.switch.is_none());
    }

    #[test]
    fn full_trigger_en() {
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        sm.on_keydown(Key::E);
        let r = sm.on_keydown(Key::N);
        assert!(r.suppress);
        assert_eq!(r.switch, Some(Language::En));
        assert!(r.replay.is_empty());
    }

    #[test]
    fn full_trigger_ja() {
        let mut sm = StateMachine::new();
        let r = feed(&mut sm, &[Key::Leader, Key::J, Key::A]);
        assert_eq!(r.switch, Some(Language::Jp));
        assert!(r.suppress);
    }

    #[test]
    fn full_trigger_zh() {
        let mut sm = StateMachine::new();
        let r = feed(&mut sm, &[Key::Leader, Key::Z, Key::H]);
        assert_eq!(r.switch, Some(Language::Zh));
        assert!(r.suppress);
    }

    #[test]
    fn cancel_at_leader_branch() {
        // `;x` → should replay `;` then pass `x`
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        let r = sm.on_keydown(Key::Other);
        assert!(!r.suppress);
        assert_eq!(r.replay, vec![Key::Leader]);
        assert!(r.switch.is_none());
    }

    #[test]
    fn cancel_mid_sequence() {
        // `;ex` with x = Other → replay `;e`, pass `x`
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        sm.on_keydown(Key::E);
        let r = sm.on_keydown(Key::Other);
        assert!(!r.suppress);
        assert_eq!(r.replay, vec![Key::Leader, Key::E]);
    }

    #[test]
    fn wrong_l2_for_j_cancels() {
        // `;jz` → replay `;j`, pass `z` (z is not l1 in idle — it is! re-enters)
        // So from Idle with z alone, z is Other from the machine's perspective
        // Actually Key::Z *is* a recognized key; after cancel we go to Idle then
        // the caller gets (;, j) in replay and the current `z` is... "passed".
        // Pass semantics: the hook should let the OS see `z`.
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        sm.on_keydown(Key::J);
        let r = sm.on_keydown(Key::Z);
        assert!(!r.suppress);
        assert_eq!(r.replay, vec![Key::Leader, Key::J]);
        // State is back to Idle — next `z` would just be typed, next `;` starts fresh
        assert_eq!(sm.state, State::Idle);
    }

    #[test]
    fn double_leader_restarts() {
        // `;;` → first is suppressed, second suppresses and replays the first `;`
        let mut sm = StateMachine::new();
        let r1 = sm.on_keydown(Key::Leader);
        assert!(r1.suppress);
        let r2 = sm.on_keydown(Key::Leader);
        assert!(r2.suppress);
        assert_eq!(r2.replay, vec![Key::Leader]);
    }

    #[test]
    fn reset_clears_state() {
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        sm.on_keydown(Key::E);
        sm.reset();
        // After reset, `n` alone should just pass through, not trigger En.
        let r = sm.on_keydown(Key::N);
        assert!(!r.suppress);
        assert!(r.switch.is_none());
    }

    #[test]
    fn sequential_triggers() {
        let mut sm = StateMachine::new();
        assert_eq!(
            feed(&mut sm, &[Key::Leader, Key::E, Key::N]).switch,
            Some(Language::En)
        );
        assert_eq!(
            feed(&mut sm, &[Key::Leader, Key::J, Key::A]).switch,
            Some(Language::Jp)
        );
        // Confirm the old `;jp` sequence no longer triggers.
        let mut sm2 = StateMachine::new();
        assert_eq!(
            feed(&mut sm2, &[Key::Leader, Key::J, Key::Other]).switch,
            None
        );
        assert_eq!(
            feed(&mut sm, &[Key::Leader, Key::Z, Key::H]).switch,
            Some(Language::Zh)
        );
    }
}
