use std::thread;
use std::time::{Duration, Instant};

use zbus::blocking::Connection;
use zbus::zvariant::OwnedFd;

use crate::{Argument, Bus, Lock, Service};

const LOGIN: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const SESSION: &str = "org.freedesktop.login1.Session";

// how long sleep is held back for the lock screens; logind gives up waiting at 5 seconds
const LOCK_WAIT: Duration = Duration::from_secs(3);
const LOOK_EVERY: Duration = Duration::from_millis(20);

pub fn follow() {
    let _ = thread::Builder::new()
        .name(String::from("amane-lock-session"))
        .spawn(lock_when_asked);

    let _ = thread::Builder::new()
        .name(String::from("amane-lock-sleep"))
        .spawn(lock_before_sleep);
}

// `loginctl lock-session`, or anything else asking logind to lock this session
fn lock_when_asked() {
    let bus = Bus::system();

    // the shell's own session, or the user's graphical one when a service manager started it
    let arguments = [Argument::from("auto")];
    let session = bus.call(LOGIN, MANAGER_PATH, MANAGER, "GetSession", &arguments);
    let session = session.text();

    if session.is_empty() {
        eprintln!("amane: logind has no session for this shell, so it can't ask for a lock");

        return;
    }

    for signal in bus.signals(SESSION, "Lock") {
        if signal.path() == session {
            Lock::start();
        }
    }
}

/*
 * logind waits for delay inhibitors before sleeping: one is held while
 * awake, and let go once the lock screens are up
 */
fn lock_before_sleep() {
    let Some(connection) = Bus::system().connection else {
        return;
    };

    // watched before the first inhibitor is taken, so no PrepareForSleep is missed
    let signals = Bus::system().signals(MANAGER, "PrepareForSleep");

    let mut held = hold_sleep(connection);

    for signal in signals {
        let going_to_sleep = signal.arguments().first().is_some_and(|value| value.bool());

        if !going_to_sleep {
            held = hold_sleep(connection);

            continue;
        }

        Lock::start();

        wait_for_lock();

        // dropping the descriptor lets the sleep go ahead
        held = None;
    }

    drop(held);
}

fn hold_sleep(connection: &Connection) -> Option<OwnedFd> {
    let reply = connection
        .call_method(
            Some(LOGIN),
            MANAGER_PATH,
            Some(MANAGER),
            "Inhibit",
            &("sleep", "amane", "Locking the screen before sleep", "delay"),
        )
        .ok()?;

    reply.body().deserialize::<OwnedFd>().ok()
}

// until the compositor has granted the lock, or long enough that sleep shouldn't wait more
fn wait_for_lock() {
    let started = Instant::now();

    while !Lock::read().locked() && started.elapsed() < LOCK_WAIT {
        thread::sleep(LOOK_EVERY);
    }
}
