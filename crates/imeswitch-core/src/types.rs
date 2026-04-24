#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Language {
    En,
    Jp,
    Zh,
}

/// Logical key recognized by the trigger state machine.
///
/// Maps one-to-one onto physical keys on a US-QWERTY layout.
/// Any key we don't care about is `Other`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Key {
    /// `;` — the leader symbol.
    Leader,
    E,
    J,
    Z,
    N,
    A,
    H,
    Other,
}
