use std::collections::HashMap;

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
        Self {
            suppress: false,
            replay: Vec::new(),
            switch: None,
        }
    }

    fn suppress() -> Self {
        Self {
            suppress: true,
            replay: Vec::new(),
            switch: None,
        }
    }

    fn switch_to(lang: Language) -> Self {
        Self {
            suppress: true,
            replay: Vec::new(),
            switch: Some(lang),
        }
    }

    fn cancel(replay: Vec<Key>) -> Self {
        Self {
            suppress: false,
            replay,
            switch: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct TrieNode {
    children: HashMap<Key, usize>,
    terminal: Option<Language>,
}

#[derive(Debug)]
pub struct StateMachine {
    nodes: Vec<TrieNode>,
    current: usize,
    buffer: Vec<Key>,
}

impl StateMachine {
    pub fn new() -> Self {
        Self::from_mappings([("en", "en"), ("ja", "ja"), ("zh", "zh")])
    }

    pub fn from_mappings<I, L, P>(mappings: I) -> Self
    where
        I: IntoIterator<Item = (L, P)>,
        L: Into<Language>,
        P: AsRef<str>,
    {
        let mut sm = Self {
            nodes: vec![TrieNode::default()],
            current: 0,
            buffer: Vec::new(),
        };

        for (language, prefix) in mappings {
            sm.insert_mapping(Language::from(language.into()), prefix.as_ref());
        }

        sm
    }

    pub fn insert_mapping(&mut self, language: Language, prefix: &str) {
        if prefix.is_empty() {
            return;
        }

        let mut node = 0;
        for key in std::iter::once(Key::Leader).chain(prefix.chars().map(Key::alpha_num)) {
            if matches!(key, Key::Other) {
                return;
            }

            if let Some(next) = self.nodes[node].children.get(&key).copied() {
                node = next;
            } else {
                let next = self.nodes.len();
                self.nodes.push(TrieNode::default());
                self.nodes[node].children.insert(key, next);
                node = next;
            }
        }
        self.nodes[node].terminal = Some(language);
    }

    pub fn reset(&mut self) {
        self.current = 0;
        self.buffer.clear();
    }

    pub fn is_idle(&self) -> bool {
        self.current == 0
    }

    /// Called on a physical keydown. The caller (platform hook) is responsible
    /// for acting on the returned `Response`.
    pub fn on_keydown(&mut self, key: Key) -> Response {
        if self.is_idle() {
            return self.start_or_pass(key);
        }

        if let Some(next) = self.nodes[self.current].children.get(&key).copied() {
            return self.advance(next, key);
        }

        let replay = std::mem::take(&mut self.buffer);
        self.current = 0;

        if let Some(next) = self.nodes[0].children.get(&key).copied() {
            self.buffer.push(key);
            self.current = next;
            return Response {
                suppress: true,
                replay,
                switch: self.finish_if_terminal(),
            };
        }

        Response::cancel(replay)
    }

    fn start_or_pass(&mut self, key: Key) -> Response {
        if let Some(next) = self.nodes[0].children.get(&key).copied() {
            self.advance(next, key)
        } else {
            Response::pass()
        }
    }

    fn advance(&mut self, next: usize, key: Key) -> Response {
        self.buffer.push(key);
        self.current = next;
        if let Some(language) = self.finish_if_terminal() {
            Response::switch_to(language)
        } else {
            Response::suppress()
        }
    }

    fn finish_if_terminal(&mut self) -> Option<Language> {
        let language = self.nodes[self.current].terminal.clone()?;
        self.reset();
        Some(language)
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

    fn k(value: char) -> Key {
        Key::alpha_num(value)
    }

    fn lang(code: &str) -> Language {
        Language::from(code)
    }

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
        let r = sm.on_keydown(k('e'));
        assert!(!r.suppress);
        assert!(r.replay.is_empty());
        assert!(r.switch.is_none());
    }

    #[test]
    fn full_trigger_en() {
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        sm.on_keydown(k('e'));
        let r = sm.on_keydown(k('n'));
        assert!(r.suppress);
        assert_eq!(r.switch, Some(lang("en")));
        assert!(r.replay.is_empty());
    }

    #[test]
    fn full_trigger_ja() {
        let mut sm = StateMachine::new();
        let r = feed(&mut sm, &[Key::Leader, k('j'), k('a')]);
        assert_eq!(r.switch, Some(lang("ja")));
        assert!(r.suppress);
    }

    #[test]
    fn full_trigger_zh() {
        let mut sm = StateMachine::new();
        let r = feed(&mut sm, &[Key::Leader, k('z'), k('h')]);
        assert_eq!(r.switch, Some(lang("zh")));
        assert!(r.suppress);
    }

    #[test]
    fn custom_language_prefixes_trigger() {
        let mut sm = StateMachine::from_mappings([("fr", "fr"), ("ko", "ko")]);
        let r = feed(&mut sm, &[Key::Leader, k('f'), k('r')]);
        assert_eq!(r.switch, Some(lang("fr")));

        let r = feed(&mut sm, &[Key::Leader, k('k'), k('o')]);
        assert_eq!(r.switch, Some(lang("ko")));
    }

    #[test]
    fn long_prefix_does_not_trigger_early() {
        let mut sm = StateMachine::from_mappings([("en", "eng")]);
        assert!(feed(&mut sm, &[Key::Leader, k('e'), k('n')])
            .switch
            .is_none());
        assert!(!sm.is_idle());

        let r = sm.on_keydown(k('g'));
        assert_eq!(r.switch, Some(lang("en")));
    }

    #[test]
    fn cancel_at_leader_branch() {
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        let r = sm.on_keydown(Key::Other);
        assert!(!r.suppress);
        assert_eq!(r.replay, vec![Key::Leader]);
        assert!(r.switch.is_none());
    }

    #[test]
    fn cancel_mid_sequence() {
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        sm.on_keydown(k('e'));
        let r = sm.on_keydown(k('k'));
        assert!(!r.suppress);
        assert_eq!(r.replay, vec![Key::Leader, k('e')]);
    }

    #[test]
    fn wrong_l2_for_j_cancels() {
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        sm.on_keydown(k('j'));
        let r = sm.on_keydown(k('z'));
        assert!(!r.suppress);
        assert_eq!(r.replay, vec![Key::Leader, k('j')]);
        assert!(sm.is_idle());
    }

    #[test]
    fn double_leader_restarts() {
        let mut sm = StateMachine::new();
        let r1 = sm.on_keydown(Key::Leader);
        assert!(r1.suppress);
        let r2 = sm.on_keydown(Key::Leader);
        assert!(r2.suppress);
        assert_eq!(r2.replay, vec![Key::Leader]);
    }

    #[test]
    fn leader_restarts_mid_sequence() {
        let mut sm = StateMachine::new();
        let r = feed(&mut sm, &[Key::Leader, k('e'), Key::Leader, k('j'), k('a')]);
        assert_eq!(r.switch, Some(lang("ja")));
    }

    #[test]
    fn reset_clears_state() {
        let mut sm = StateMachine::new();
        sm.on_keydown(Key::Leader);
        sm.on_keydown(k('e'));
        sm.reset();
        let r = sm.on_keydown(k('n'));
        assert!(!r.suppress);
        assert!(r.switch.is_none());
    }

    #[test]
    fn sequential_triggers() {
        let mut sm = StateMachine::new();
        assert_eq!(
            feed(&mut sm, &[Key::Leader, k('e'), k('n')]).switch,
            Some(lang("en"))
        );
        assert_eq!(
            feed(&mut sm, &[Key::Leader, k('j'), k('a')]).switch,
            Some(lang("ja"))
        );

        let mut sm2 = StateMachine::new();
        assert_eq!(
            feed(&mut sm2, &[Key::Leader, k('j'), Key::Other]).switch,
            None
        );
        assert_eq!(
            feed(&mut sm, &[Key::Leader, k('z'), k('h')]).switch,
            Some(lang("zh"))
        );
    }
}
