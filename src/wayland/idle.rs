use std::time::Duration;

use smithay_client_toolkit::reexports::protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::{self, ExtIdleNotifierV1},
};
use smithay_client_toolkit::reexports::protocols::wp::idle_inhibit::zv1::client::{
    zwp_idle_inhibit_manager_v1::{self, ZwpIdleInhibitManagerV1},
    zwp_idle_inhibitor_v1::{self, ZwpIdleInhibitorV1},
};
use wayland_client::{Connection, Dispatch, QueueHandle, protocol::wl_surface::WlSurface};

use crate::{Idle, Service};

use super::{WaylandState, surface::Role};

// one Idle::watch, as the compositor counts it
pub struct IdleWatch {
    after: Duration,
    notification: ExtIdleNotificationV1,
}

impl Drop for IdleWatch {
    fn drop(&mut self) {
        self.notification.destroy();
    }
}

// keeps the screen awake for as long as the window it was made on shows
pub struct Inhibitor {
    inhibitor: ZwpIdleInhibitorV1,
    surface: WlSurface,
}

impl Drop for Inhibitor {
    fn drop(&mut self) {
        self.inhibitor.destroy();
    }
}

impl WaylandState {
    // runs on every wake, since that is when Idle changes show up
    pub fn update_idle(&mut self) {
        self.update_inhibitor();

        // without a seat yet the change waits for a later wake
        let (Some(notifier), Some(seat)) = (&self.idle_notifier, self.seat.seats().next()) else {
            return;
        };

        if !Idle::take_change() {
            return;
        }

        let wanted = Idle::read().wanted();

        // a watch whose wait changed starts counting again
        self.idle_watches.retain(|name, watch| {
            wanted
                .iter()
                .any(|(wanted, after)| wanted == name && *after == watch.after)
        });

        for (name, after) in wanted {
            if self.idle_watches.contains_key(&name) {
                continue;
            }

            let timeout = u32::try_from(after.as_millis()).unwrap_or(u32::MAX);

            // this request respects inhibitors, so keeping awake pauses the watches too
            let notification =
                notifier.get_idle_notification(timeout, &seat, &self.qh, name.clone());

            self.idle_watches.insert(
                name,
                IdleWatch {
                    after,
                    notification,
                },
            );
        }
    }

    fn update_inhibitor(&mut self) {
        let wanted = Idle::wants_awake();

        // the window it was made on may have closed or hidden since
        let still_good = self
            .inhibitor
            .as_ref()
            .is_some_and(|inhibitor| wanted && self.shows(&inhibitor.surface));

        if !still_good {
            self.inhibitor = None;
        }

        if !wanted || self.inhibitor.is_some() {
            return;
        }

        let Some(manager) = &self.idle_inhibit_manager else {
            return;
        };

        // the compositor only counts inhibitors on surfaces it shows, like a bar
        let surface = self
            .windows
            .iter()
            .map(|window| window.role.wl_surface())
            .find(|surface| self.shows(surface))
            .cloned();

        let Some(surface) = surface else {
            return;
        };

        let inhibitor = manager.create_inhibitor(&surface, &self.qh, ());

        self.inhibitor = Some(Inhibitor { inhibitor, surface });
    }

    // an open layer window that isn't hidden
    fn shows(&self, surface: &WlSurface) -> bool {
        self.windows.iter().any(|window| {
            window.role.wl_surface() == surface
                && matches!(&window.role, Role::Layer { settings, .. } if settings.visible)
        })
    }
}

impl Dispatch<ExtIdleNotificationV1, String> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        name: &String,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let now_idle = match event {
            ext_idle_notification_v1::Event::Idled => true,
            ext_idle_notification_v1::Event::Resumed => false,
            _ => return,
        };

        // the service isn't held while the handler runs, so it can watch or keep awake itself
        if let Some(on_change) = Idle::changed(name, now_idle) {
            on_change(now_idle);
        }
    }
}

// neither the managers nor the inhibitor have events
impl Dispatch<ExtIdleNotifierV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotifierV1,
        _: ext_idle_notifier_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpIdleInhibitManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwpIdleInhibitManagerV1,
        _: zwp_idle_inhibit_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpIdleInhibitorV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwpIdleInhibitorV1,
        _: zwp_idle_inhibitor_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
