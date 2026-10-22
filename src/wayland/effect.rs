use smithay_client_toolkit::reexports::protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1},
    ext_background_effect_surface_v1::{self, ExtBackgroundEffectSurfaceV1},
};
use wayland_client::{Connection, Dispatch, QueueHandle, protocol::wl_surface::WlSurface};

use crate::graphics::{Backdrop, Corners};

use super::WaylandState;
use super::surface::OpenWindow;

/*
 * blur behind a layer surface, under the rectangles marked with backdrop();
 * the region last sent is kept, so a frame with the same glass sends nothing
 */
pub struct BackgroundEffect {
    surface: ExtBackgroundEffectSurfaceV1,
    sent: Vec<[i32; 4]>,
}

// has to go before the surface it belongs to
impl Drop for BackgroundEffect {
    fn drop(&mut self) {
        self.surface.destroy();
    }
}

impl WaylandState {
    // none when the compositor can't blur
    pub fn make_background_effect(&self, surface: &WlSurface) -> Option<BackgroundEffect> {
        let manager = self.background_effect.as_ref()?;

        let surface = manager.get_background_effect(surface, &self.qh, ());

        Some(BackgroundEffect {
            surface,
            sent: Vec::new(),
        })
    }
}

impl OpenWindow {
    // takes effect with the commit that presents the frame
    pub fn update_blur_region(&mut self, backdrops: &[Backdrop]) {
        let Some(effect) = &mut self.background_effect else {
            return;
        };

        let mut rows = Vec::new();

        for backdrop in backdrops {
            rows.extend(surface_rows(backdrop, self.scale));
        }

        if rows == effect.sent {
            return;
        }

        if rows.is_empty() {
            effect.surface.set_blur_region(None);
        } else {
            let region = self.compositor.create_region(&self.qh, ());

            for [x, y, width, height] in &rows {
                region.add(*x, *y, *width, *height);
            }

            effect.surface.set_blur_region(Some(&region));

            // the surface keeps its own copy, so the region can go right away
            region.destroy();
        }

        effect.sent = rows;
    }
}

/*
 * a backdrop in surface pixels, its rounded corners as rows of whole pixels
 * the compositor can follow exactly; rows alike are joined into one rectangle
 */
fn surface_rows(backdrop: &Backdrop, scale: f32) -> Vec<[i32; 4]> {
    let area = backdrop.area;
    let transform = backdrop.transform;

    // the transform reaches buffer pixels, the surface is that much smaller
    let (left, top) = transform.map(area.x, area.y);
    let (right, bottom) = transform.map(area.x + area.width, area.y + area.height);

    let left = (left / scale).round() as i32;
    let top = (top / scale).round() as i32;
    let right = (right / scale).round() as i32;
    let bottom = (bottom / scale).round() as i32;

    let factor = transform.sx / scale;

    rows(left, top, right, bottom, backdrop.radius.map(|radius| radius * factor))
}

fn rows(left: i32, top: i32, right: i32, bottom: i32, radius: Corners) -> Vec<[i32; 4]> {
    let mut rows: Vec<[i32; 4]> = Vec::new();

    let height = bottom - top;

    for row in 0..height {
        // measured at the middle of the row
        let from_top = row as f32 + 0.5;
        let from_bottom = height as f32 - from_top;

        let inset_left = inset(radius.top_left, from_top).max(inset(radius.bottom_left, from_bottom));
        let inset_right = inset(radius.top_right, from_top).max(inset(radius.bottom_right, from_bottom));

        let x = left + inset_left;
        let width = right - inset_right - x;

        if width <= 0 {
            continue;
        }

        let y = top + row;

        match rows.last_mut() {
            Some(last) if last[0] == x && last[2] == width && last[1] + last[3] == y => last[3] += 1,
            _ => rows.push([x, y, width, 1]),
        }
    }

    rows
}

// how far a corner of this radius cuts into a row this far from its edge
fn inset(radius: f32, from_edge: f32) -> i32 {
    if radius <= 0.0 || from_edge >= radius {
        return 0;
    }

    let rise = radius - from_edge;

    (radius - (radius * radius - rise * rise).sqrt()).round() as i32
}

// the capabilities are only read by the compositor's own policy, there's nothing to change here
impl Dispatch<ExtBackgroundEffectManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        _: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtBackgroundEffectSurfaceV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtBackgroundEffectSurfaceV1,
        _: ext_background_effect_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(rows: &[[i32; 4]]) -> i32 {
        rows.iter().map(|row| row[2] * row[3]).sum()
    }

    #[test]
    fn square_corners_are_one_rectangle() {
        assert_eq!(rows(10, 20, 110, 60, Corners::from(0.0)), vec![[10, 20, 100, 40]]);
    }

    #[test]
    fn rounded_corners_step_in_and_cover_about_the_rounded_area() {
        let rows = rows(0, 0, 100, 40, Corners::from(10.0));

        // the first row is pulled in, the middle is one whole rectangle
        assert!(rows[0][0] > 0);
        assert!(rows.iter().any(|row| row[0] == 0 && row[2] == 100 && row[3] >= 20));

        let expected = 100.0 * 40.0 - (4.0 - std::f32::consts::PI) * 100.0;

        assert!((area(&rows) as f32 - expected).abs() < 40.0);
    }

    #[test]
    fn a_capsule_has_no_flat_rows_when_fully_round() {
        let rows = rows(0, 0, 60, 20, Corners::from(10.0));

        assert!(rows.iter().all(|row| row[2] <= 60));
        assert!(rows[0][2] < 60);
    }
}
