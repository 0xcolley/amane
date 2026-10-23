use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::Service;

// set when the watches change, taken by the event loop when it wakes
static WATCHES_CHANGED: AtomicBool = AtomicBool::new(false);

// whether to keep the screen awake, read by the event loop on every wake
static AWAKE: AtomicBool = AtomicBool::new(false);

/*
 * whether the computer has been left alone: watches that fire after a stretch
 * without input, and a switch that keeps the screen from going idle at all
 */
#[derive(Default)]
pub struct Idle {
    watches: HashMap<String, Watch>,
    awake: bool,
}

#[derive(Clone, Copy)]
struct Watch {
    after: Duration,
    on_change: fn(bool),

    // fired, and no input since
    idle: bool,
}

// changes through the compositor rather than polling
impl Service for Idle {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {}
}

impl Idle {
    /*
     * calls on_change(true) once nothing has been touched for `after`, and
     * on_change(false) at the next input. Watching the same name again
     * changes it, and None stops it. The handler runs on the event loop, so
     * it should only start things, like Lock::start or a command
     */
    pub fn watch(name: &str, after: Option<Duration>, on_change: fn(bool)) {
        let mut idle = Self::write();

        let Some(after) = after else {
            if idle.watches.remove(name).is_some() {
                WATCHES_CHANGED.store(true, Ordering::Relaxed);
            }

            return;
        };

        match idle.watches.get_mut(name) {
            // the same wait keeps counting from the last input
            Some(watch) if watch.after == after => watch.on_change = on_change,

            _ => {
                let watch = Watch {
                    after,
                    on_change,
                    idle: false,
                };

                idle.watches.insert(String::from(name), watch);

                WATCHES_CHANGED.store(true, Ordering::Relaxed);
            }
        }
    }

    /*
     * keeps the screen from going idle while on, like a video player does:
     * the compositor won't blank it, and watches that respect it, like
     * every watch here, don't fire
     */
    pub fn keep_awake(on: bool) {
        let mut idle = Self::write();

        idle.awake = on;

        AWAKE.store(on, Ordering::Relaxed);
    }

    pub fn awake(&self) -> bool {
        self.awake
    }

    // the watch under this name has fired, and nothing has been touched since
    pub fn idle(&self, name: &str) -> bool {
        self.watches.get(name).is_some_and(|watch| watch.idle)
    }

    pub(crate) fn wants_awake() -> bool {
        AWAKE.load(Ordering::Relaxed)
    }

    pub(crate) fn take_change() -> bool {
        WATCHES_CHANGED.swap(false, Ordering::Relaxed)
    }

    pub(crate) fn wanted(&self) -> Vec<(String, Duration)> {
        self.watches
            .iter()
            .map(|(name, watch)| (name.clone(), watch.after))
            .collect()
    }

    // the compositor says a watch fired or ended; gives back the handler to call, once
    pub(crate) fn changed(name: &str, now_idle: bool) -> Option<fn(bool)> {
        let mut idle = Self::write();

        let watch = idle.watches.get_mut(name)?;

        if watch.idle == now_idle {
            return None;
        }

        watch.idle = now_idle;

        Some(watch.on_change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nothing(_: bool) {}

    // the service is shared by every test in the process, so each uses its own names
    #[test]
    fn a_watch_fires_once_each_way() {
        Idle::watch("test-fires", Some(Duration::from_secs(60)), nothing);

        assert!(Idle::changed("test-fires", true).is_some());
        assert!(Idle::read().idle("test-fires"));
        assert!(Idle::changed("test-fires", true).is_none());
        assert!(Idle::changed("test-fires", false).is_some());
        assert!(!Idle::read().idle("test-fires"));
    }

    #[test]
    fn rewatching_keeps_the_same_wait_and_restarts_a_new_one() {
        Idle::watch("test-rewatch", Some(Duration::from_secs(60)), nothing);
        Idle::changed("test-rewatch", true);

        Idle::watch("test-rewatch", Some(Duration::from_secs(60)), nothing);
        assert!(Idle::read().idle("test-rewatch"));

        Idle::watch("test-rewatch", Some(Duration::from_secs(120)), nothing);
        assert!(!Idle::read().idle("test-rewatch"));

        Idle::watch("test-rewatch", None, nothing);
        assert!(Idle::changed("test-rewatch", true).is_none());
    }
}
