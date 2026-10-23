use std::sync::atomic::{AtomicU8, Ordering};

// which of the keys that change other keys are held
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,

    // the Super or Windows key
    pub logo: bool,
}

const CTRL: u8 = 1;
const SHIFT: u8 = 2;
const ALT: u8 = 4;
const LOGO: u8 = 8;

// kept as bits, so any thread can ask without a lock
static HELD: AtomicU8 = AtomicU8::new(0);

// what's held right now, on the keyboard that last typed into one of the shell's windows
pub fn modifiers() -> Modifiers {
    let held = HELD.load(Ordering::Relaxed);

    Modifiers {
        ctrl: held & CTRL != 0,
        shift: held & SHIFT != 0,
        alt: held & ALT != 0,
        logo: held & LOGO != 0,
    }
}

pub fn set(modifiers: Modifiers) {
    let mut held = 0;

    for (on, bit) in [
        (modifiers.ctrl, CTRL),
        (modifiers.shift, SHIFT),
        (modifiers.alt, ALT),
        (modifiers.logo, LOGO),
    ] {
        if on {
            held |= bit;
        }
    }

    HELD.store(held, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_each_key_apart() {
        let held = Modifiers {
            ctrl: true,
            shift: false,
            alt: true,
            logo: false,
        };

        set(held);

        assert_eq!(modifiers(), held);

        set(Modifiers::default());

        assert_eq!(modifiers(), Modifiers::default());
    }
}
