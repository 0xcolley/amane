use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::sync::{Mutex, PoisonError};
use std::thread;

use smithay_client_toolkit::reexports::protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::{self, ExtDataControlManagerV1},
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    ext_data_control_source_v1::{self, ExtDataControlSourceV1},
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};

use crate::services::clipboard::{self, Offering};
use crate::{Clipboard, widgets};

use super::WaylandState;

// the types an offer can be read as, which arrive just before the offer itself
type Types = Mutex<Vec<String>>;

/*
 * the clipboard as a clipboard manager sees it: every copy, whichever
 * program has focus. None of it when the compositor lacks ext-data-control
 */
pub struct DataControl {
    manager: Option<ExtDataControlManagerV1>,
    device: Option<ExtDataControlDeviceV1>,

    // what the clipboard holds right now
    selection: Option<ExtDataControlOfferV1>,
}

impl DataControl {
    pub fn new(manager: Option<ExtDataControlManagerV1>) -> Self {
        Self {
            manager,
            device: None,
            selection: None,
        }
    }
}

impl WaylandState {
    // runs on every wake, since that is when the clipboard is asked for things
    pub fn update_clipboard(&mut self) {
        // without a seat yet the device waits for a later wake
        if self.data_control.device.is_none()
            && let (Some(manager), Some(seat)) =
                (&self.data_control.manager, self.seat.seats().next())
        {
            self.data_control.device = Some(manager.get_data_device(&seat, &self.qh, ()));
        }

        if self.data_control.device.is_none() {
            return;
        }

        // what was copied before anything read the history is its first clip
        if Clipboard::take_start()
            && let Some(offer) = self.data_control.selection.clone()
        {
            self.keep(&offer);
        }

        if clipboard::take_paste_asked() {
            self.paste();
        }

        if let Some(text) = clipboard::take_pasted() {
            widgets::paste(&text);
        }

        if let Some(offering) = Clipboard::take_offering() {
            self.offer(offering);
        }
    }

    fn selected(&mut self, offer: Option<ExtDataControlOfferV1>) {
        if let Some(old) = self.data_control.selection.take() {
            old.destroy();
        }

        let Some(offer) = offer else {
            // the program that copied it quit; what it held is offered again
            if Clipboard::recording() {
                Clipboard::emptied();

                if let Some(offering) = Clipboard::take_offering() {
                    self.offer(offering);
                }
            }

            return;
        };

        self.keep(&offer);

        self.data_control.selection = Some(offer);
    }

    // reads a copy into the history, if anything reads the history
    fn keep(&self, offer: &ExtDataControlOfferV1) {
        if !Clipboard::recording() {
            return;
        }

        let types = types(offer);

        let copy = Clipboard::selected(types.iter().any(|kind| kind == clipboard::OURS));

        let Some(kind) = clipboard::pick(&types) else {
            return;
        };

        self.receive(offer, kind, move |bytes| {
            Clipboard::record(copy, kind, bytes)
        });
    }

    fn paste(&self) {
        let Some(offer) = &self.data_control.selection else {
            return;
        };

        let Some(kind) = clipboard::text_type(&types(offer)) else {
            return;
        };

        self.receive(offer, kind, |bytes| {
            if let Ok(text) = String::from_utf8(bytes) {
                clipboard::pasted(text);
            }
        });
    }

    /*
     * asks the program that copied it for the bytes, and reads them on a
     * thread of their own, so a program slow to write never stalls drawing;
     * a copy past the limit is let go
     */
    fn receive(
        &self,
        offer: &ExtDataControlOfferV1,
        kind: &'static str,
        then: impl FnOnce(Vec<u8>) + Send + 'static,
    ) {
        let Ok((reader, writer)) = io::pipe() else {
            return;
        };

        offer.receive(String::from(kind), writer.as_fd());

        // only the other program holds the write end, so the read ends when it's done
        drop(writer);

        // the request has to reach the other program before it can write anything
        let _ = self.connection.flush();

        let limit = clipboard::limit(kind);

        thread::spawn(move || {
            let mut bytes = Vec::new();

            // one byte past the limit tells a copy that's too big
            let read = reader.take(limit as u64 + 1).read_to_end(&mut bytes);

            if read.is_ok() && bytes.len() <= limit {
                then(bytes);
            }
        });
    }

    // puts the shell's own copy on the clipboard, marked so the history doesn't keep it twice
    fn offer(&self, offering: Offering) {
        let (Some(manager), Some(device)) = (&self.data_control.manager, &self.data_control.device)
        else {
            return;
        };

        let types = offering.types.clone();

        let source = manager.create_data_source(&self.qh, offering);

        for kind in types.into_iter().chain([clipboard::OURS]) {
            source.offer(String::from(kind));
        }

        device.set_selection(Some(&source));
    }
}

fn types(offer: &ExtDataControlOfferV1) -> Vec<String> {
    offer.data::<Types>().map_or_else(Vec::new, |types| {
        types.lock().unwrap_or_else(PoisonError::into_inner).clone()
    })
}

impl Dispatch<ExtDataControlDeviceV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        device: &ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::Selection { id } => state.selected(id),

            // only the clipboard is kept, not text that's merely selected
            ext_data_control_device_v1::Event::PrimarySelection { id: Some(offer) } => {
                offer.destroy();
            }

            // the seat went away; a new device is made if another one turns up
            ext_data_control_device_v1::Event::Finished => {
                device.destroy();

                state.data_control.device = None;
            }

            _ => {}
        }
    }

    // each offer gathers the types it can be read as, which arrive before it's selected
    event_created_child!(WaylandState, ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE =>
            (ExtDataControlOfferV1, Types::default())
    ]);
}

impl Dispatch<ExtDataControlOfferV1, Types> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        types: &Types,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            types
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(mime_type);
        }
    }
}

impl Dispatch<ExtDataControlSourceV1, Offering> for WaylandState {
    fn event(
        _: &mut Self,
        source: &ExtDataControlSourceV1,
        event: ext_data_control_source_v1::Event,
        offering: &Offering,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // the mark holds nothing, so its pipe just closes
            ext_data_control_source_v1::Event::Send { mime_type, fd }
                if mime_type != clipboard::OURS =>
            {
                let bytes = offering.bytes.clone();

                // written on a thread of its own, so a program slow to read never stalls drawing
                thread::spawn(move || {
                    let _ = File::from(fd).write_all(&bytes);
                });
            }

            // something else was copied since
            ext_data_control_source_v1::Event::Cancelled => source.destroy(),

            _ => {}
        }
    }
}

// the manager has no events
impl Dispatch<ExtDataControlManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtDataControlManagerV1,
        _: ext_data_control_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
