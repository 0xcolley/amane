# Idle

`Idle` notices when the computer has been left alone, and can keep the screen awake. Use it to lock after a few minutes, turn the displays off, or keep them on while something is playing.

It doesn't poll. The compositor counts the time since the last key, click or touch, through the **ext-idle-notify** protocol, and keeps the screen awake through **idle-inhibit**. niri, Hyprland and Sway support both. On other compositors, watches never fire and keeping awake does nothing.

## Locking after five minutes

```rust,ignore
use std::time::Duration;

use amane::{App, Idle, Lock};

fn main() {
    Idle::watch("lock", Some(Duration::from_secs(5 * 60)), |idle| {
        if idle {
            Lock::start();
        }
    });

    App::new().window(bar).lock(lock_screen).run();
}
```

The handler gets `true` once nothing has been touched for the time given, and `false` at the next input. Each change is sent once.

## Watches

- **Several at once:** each watch has its own name, so one can lock after 5 minutes while another turns the displays off after 10.
- **Changing one:** watch the same name again. The same wait keeps counting from the last input, and a different wait starts again from zero. This is how a setting can change the wait while the shell runs.
- **Stopping one:** pass `None`.
- **Handlers run on the event loop,** like click handlers. Start things from them, like `Lock::start()` or `amane::spawn`, and don't wait in them.
- **Watches can start before `run()`.** They begin counting as soon as the shell connects.

```rust,ignore
// the displays go off after ten minutes, and niri turns them back on at the next input
Idle::watch("display", Some(Duration::from_secs(10 * 60)), |idle| {
    if idle {
        amane::spawn("niri msg action power-off-monitors");
    }
});
```

## Keeping the screen awake

`Idle::keep_awake(true)` keeps the screen from going idle, the way a video player does: the compositor won't blank or lock it, and watches here don't fire. `Idle::keep_awake(false)` lets it go idle again.

```rust,ignore
fn awake_button() -> Rectangle {
    let awake = Idle::read().awake();

    Rectangle::new()
        .width(30.0)
        .height(30.0)
        .fill(if awake { "#f9e2af" } else { "#313244" })
        .on_click(move |_| Idle::keep_awake(!awake))
}
```

The compositor only listens to a shell that's showing something, so this needs an open layer window that isn't hidden, like a bar. It holds the screen awake through the first one it finds, and moves to another if that window closes. Nothing is saved: the screen can go idle again after the shell restarts.

## Reference

| Function | Does |
|---|---|
| `Idle::watch(name, Some(after), on_change)` | calls `on_change(true)` after `after` without input, then `on_change(false)` at the next input |
| `Idle::watch(name, None, on_change)` | stops the watch |
| `Idle::keep_awake(on)` | keeps the screen awake while `on` |
| `Idle::read().idle(name)` | that watch has fired, and nothing has been touched since |
| `Idle::read().awake()` | the screen is being kept awake |

Try `examples/idle.rs`: a pill that says when nobody's been there for five seconds, and keeps the screen awake when clicked.
