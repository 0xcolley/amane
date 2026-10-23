# Clipboard

`Clipboard` keeps what was copied lately, text and pictures, newest first, and can put any of it back on the clipboard. It also lets text inputs paste with `Ctrl+V`.

It doesn't poll. The compositor reports every copy, whichever program made it, through the **ext-data-control-v1** protocol. niri supports it; to check another compositor, look for `ext_data_control_manager_v1` in `wayland-info`. Without it the history stays empty and paste does nothing.

## A history

```rust,ignore
use amane::{Clipboard, ClipKind, Service};

fn history() -> Column {
    let clipboard = Clipboard::read();

    let rows = clipboard.clips().iter().take(10).map(|clip| {
        let id = clip.id();

        let label = match clip.kind() {
            ClipKind::Text(text) => text.lines().next().unwrap_or_default().to_string(),
            ClipKind::Image { width, height, .. } => format!("picture, {width}×{height}"),
        };

        Rectangle::new()
            .width(300.0)
            .height(32.0)
            .on_click(move |button| match button {
                Button::Right => Clipboard::remove(id),
                _ => Clipboard::copy(id),
            })
            .child(Text::new(label).elide())
    });

    Column::new(rows.map(|row| Box::new(row) as Box<dyn Widget>).collect())
}
```

## What's kept

- **Nothing until something reads it.** The first `Clipboard::read()` or `Clipboard::write()` starts the history, beginning with what's on the clipboard at that moment. A shell that never uses `Clipboard` never looks at what's copied.
- **Text before pictures.** A copy that offers both, like a picture copied from a browser, is kept as text. Pictures are kept if they're PNG or JPEG.
- **Never passwords.** Password managers like KeePassXC mark what they copy with `x-kde-passwordManagerHint`, and those copies are left out.
- **Up to 100 clips, 20 of them pictures.** The oldest go first. Copying the same thing again moves it to the top instead of keeping it twice.
- **Size limits:** text up to 1 MB, pictures up to 16 MB. Bigger copies still paste in other programs, they just aren't kept.
- **Only while the shell runs.** The history is kept in memory. Pictures are written to `$XDG_RUNTIME_DIR/amane/clipboard/`, which only you can read, and are deleted when they drop out of the history or the shell starts again.

## Putting a clip back

`Clipboard::copy(id)` puts a clip back on the clipboard and moves it to the top. The shell keeps offering it until something else is copied. `Clipboard::copy_text(text)` copies text from the shell itself, like an address from a network panel, and keeps it like any other copy.

When the program that copied something quits, the clipboard would normally empty. If that copy was kept, the shell offers it again, so it still pastes. This doesn't happen after a copy that wasn't kept, so a password a manager clears stays cleared.

## Pasting into a text input

`Ctrl+V` in a focused [text input](text_input.md) pastes the clipboard's text at the cursor. It's read straight from the clipboard, not the history, so it works without `Clipboard` and pastes passwords too. Line breaks become spaces, since an input holds one line. The text arrives a moment after the key, once the program that copied it has handed it over, and `on_change` runs then.

## Reference

| Function | Does |
|---|---|
| `Clipboard::read().clips()` | the kept clips, newest first |
| `Clipboard::read().current()` | the id of the clip the clipboard holds now, if it was kept |
| `Clipboard::copy(id)` | puts that clip back on the clipboard, at the top |
| `Clipboard::copy_text(text)` | copies text, keeping it like any other copy |
| `Clipboard::remove(id)` | forgets one clip |
| `Clipboard::clear()` | forgets every clip; what the clipboard holds stays there |
| `clip.id()` | a number that stays with the clip |
| `clip.kind()` | `ClipKind::Text(text)` or `ClipKind::Image { path, width, height }` |
| `clip.text()` | the text, for a text clip |
| `clip.copied()` | when it was last copied, as a `SystemTime` |

Show a picture clip with `Image::contain(path)`, and add `.thumbnail(width, height)` when it's drawn far smaller than its file.

Try `examples/clipboard.rs`: the latest copies, which a click copies again and a right click forgets, under a field that `Ctrl+V` pastes into.
