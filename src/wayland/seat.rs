use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState, pointer::ThemeSpec};
use wayland_client::{Connection, QueueHandle, protocol::wl_seat::WlSeat};

use super::WaylandState;

impl SeatHandler for WaylandState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer => {
                // the fallback theme draws its icons on a surface of their own
                let cursor_surface = self.compositor.create_surface(qh);

                let pointer = self
                    .seat
                    .get_pointer_with_theme::<_, ()>(
                        qh,
                        &seat,
                        self.shm.wl_shm(),
                        cursor_surface,
                        ThemeSpec::default(),
                    )
                    .expect("failed to get pointer");

                self.gestures = self.make_gestures(pointer.pointer());

                self.pointer_device = Some(pointer);

                // files are dropped where the pointer is, so it comes with the pointer
                if let Some(manager) = &self.data_device_manager {
                    self.data_device = Some(manager.get_data_device(qh, &seat));
                }
            }

            Capability::Keyboard => {
                // held keys repeat at the rate the compositor gives
                let keyboard = match self.loop_handle.clone() {
                    Some(handle) => self.seat.get_keyboard_with_repeat(
                        qh,
                        &seat,
                        None,
                        handle,
                        Box::new(|state: &mut WaylandState, _, event| state.repeat(event)),
                    ),

                    None => self.seat.get_keyboard(qh, &seat, None),
                };

                self.keyboard_device = Some(keyboard.expect("failed to get keyboard"));
            }

            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer => {
                // the gestures belong to the pointer, so they go first
                self.gestures = None;

                if let Some(pointer) = self.pointer_device.take() {
                    pointer.pointer().release();
                }

                self.data_device = None;
            }

            Capability::Keyboard => {
                if let Some(keyboard) = self.keyboard_device.take() {
                    keyboard.release();
                }
            }

            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}
