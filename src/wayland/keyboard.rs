use smithay_client_toolkit::seat::keyboard::{
    self, KeyEvent, KeyboardHandler, Keysym, RawModifiers,
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{wl_keyboard::WlKeyboard, wl_surface::WlSurface},
};

use crate::input::{focus, modifiers};
use crate::{Key, Modifiers};

use super::WaylandState;

impl KeyboardHandler for WaylandState {
    // keys only say which keyboard they came from, so the focused window is kept here
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        surface: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.keyboard_focus = Some(surface.clone());
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        surface: &WlSurface,
        _: u32,
    ) {
        self.keyboard_focus = None;

        // keys stop arriving, so no text input can stay focused
        focus::clear();

        // nor can anything stay held; the compositor sends them again on the way back in
        modifiers::set(Modifiers::default());

        self.request_frame_on(surface);
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.send_key(to_key(&event));
    }

    // repeats come through the callback the keyboard was made with instead
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        held: keyboard::Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        modifiers::set(Modifiers {
            ctrl: held.ctrl,
            shift: held.shift,
            alt: held.alt,
            logo: held.logo,
        });
    }
}

impl WaylandState {
    // a key held down, sent again at the compositor's repeat rate
    pub fn repeat(&mut self, event: KeyEvent) {
        let key = to_key(&event);

        // moving and typing repeat; Enter or Escape again would act twice
        let repeats = matches!(
            key,
            Key::Character(_)
                | Key::Space
                | Key::Backspace
                | Key::Delete
                | Key::Left
                | Key::Right
                | Key::Up
                | Key::Down
        );

        if repeats {
            self.send_key(key);
        }
    }

    fn send_key(&mut self, key: Key) {
        let Some(surface) = self.keyboard_focus.clone() else {
            return;
        };

        // a focused text input takes the key before the window's on_key sees it
        if focus::send(key) {
            self.request_frame_on(&surface);

            return;
        }

        let Some(window) = self.window(&surface) else {
            return;
        };

        let Some(on_key) = &window.on_key else {
            return;
        };

        on_key(key);

        // a service the handler changed wakes its own readers, this window redraws for the rest
        self.request_frame_on(&surface);
    }
}

fn to_key(event: &KeyEvent) -> Key {
    // with Ctrl held, letters type invisible control characters, so the key itself is named
    if modifiers::modifiers().ctrl
        && let Some(letter) = event.keysym.key_char()
        && letter.is_alphanumeric()
    {
        return Key::Control(letter.to_lowercase().next().unwrap_or(letter));
    }

    match event.keysym {
        Keysym::Return | Keysym::KP_Enter => Key::Enter,
        Keysym::Escape => Key::Escape,
        Keysym::Tab => Key::Tab,
        Keysym::BackSpace => Key::Backspace,
        Keysym::Delete | Keysym::KP_Delete => Key::Delete,
        Keysym::space => Key::Space,
        Keysym::Up => Key::Up,
        Keysym::Down => Key::Down,
        Keysym::Left => Key::Left,
        Keysym::Right => Key::Right,
        Keysym::Home => Key::Home,
        Keysym::End => Key::End,
        _ => character(event),
    }
}

// the text a key types already has shift and the keyboard layout applied
fn character(event: &KeyEvent) -> Key {
    let Some(text) = &event.utf8 else {
        return Key::Other;
    };

    let mut letters = text.chars();

    // a key that types more than one character has no single letter to give
    let (Some(letter), None) = (letters.next(), letters.next()) else {
        return Key::Other;
    };

    // with ctrl held, keys type invisible control characters
    if letter.is_control() {
        return Key::Other;
    }

    Key::Character(letter)
}
