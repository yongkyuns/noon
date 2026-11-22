//! Gesture interpretation shared by hosts. Coordinates use viewport-height units.
use noon_core::Vec2;

use crate::SpatialInspectionMotion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialPointerPhase {
    Press,
    Move,
    Release,
    Cancel,
}

#[derive(Clone, Copy, Debug)]
pub struct SpatialPointerInput {
    pub id: i32,
    pub phase: SpatialPointerPhase,
    pub position: Vec2,
    pub view_revision: u64,
    pub viewport_height: f32,
    pub timestamp_ms: f64,
    pub touch: bool,
    pub pan: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum SpatialGesture {
    None,
    Motion(SpatialInspectionMotion),
    Click(Vec2),
    Reset,
}

#[derive(Clone, Copy, Debug)]
struct Contact {
    id: i32,
    start: Vec2,
    position: Vec2,
    pan: bool,
    touch: bool,
    dragged: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SpatialGestures {
    contacts: [Option<Contact>; 2],
    view: u64,
    last_click: Option<(Vec2, f64)>,
}

impl SpatialGestures {
    pub(super) fn submit(&mut self, input: SpatialPointerInput) -> SpatialGesture {
        use SpatialGesture::{Click, Motion, None as NoGesture, Reset};
        if self.view != input.view_revision {
            *self = Self {
                view: input.view_revision,
                ..Self::default()
            };
        }
        let index = self
            .contacts
            .iter()
            .position(|c| c.is_some_and(|c| c.id == input.id));
        if input.phase == SpatialPointerPhase::Cancel {
            if let Some(index) = index {
                self.contacts[index] = None;
            }
            self.last_click = None;
            return NoGesture;
        }
        let position = input.position;
        if !position.x.is_finite()
            || !position.y.is_finite()
            || !input.viewport_height.is_finite()
            || input.viewport_height <= 0.0
            || !input.timestamp_ms.is_finite()
        {
            return NoGesture;
        }
        if input.phase == SpatialPointerPhase::Press {
            if index.is_some() {
                return NoGesture;
            }
            if self
                .contacts
                .iter()
                .flatten()
                .any(|other| !input.touch || !other.touch)
            {
                return NoGesture;
            }
            let Some(index) = self.contacts.iter().position(Option::is_none) else {
                return NoGesture;
            };
            let two_finger = self.contacts[1 - index].is_some();
            if let Some(other) = &mut self.contacts[1 - index] {
                other.dragged = true;
                self.last_click = None;
            }
            self.contacts[index] = Some(Contact {
                id: input.id,
                start: position,
                position,
                pan: input.pan,
                touch: input.touch,
                dragged: two_finger,
            });
            return NoGesture;
        }

        let Some(index) = index else { return NoGesture };
        let mut contact = self.contacts[index].expect("located contact");
        let distance = |a: Vec2, b: Vec2| (a.x - b.x).hypot(a.y - b.y);
        let threshold = 4.0 / input.viewport_height;
        let release = input.phase == SpatialPointerPhase::Release;
        let previous = if contact.dragged {
            contact.position
        } else {
            contact.start
        };
        let crossed_threshold = distance(position, contact.start) >= threshold;
        contact.dragged |= crossed_threshold;
        contact.position = position;
        self.contacts[index] = (!release).then_some(contact);
        if contact.dragged {
            self.last_click = None;
            let delta = [
                f64::from(position.x - previous.x),
                f64::from(position.y - previous.y),
            ];
            let mut motion = SpatialInspectionMotion::default();
            if let Some(other) = self.contacts[1 - index] {
                motion.pan = delta.map(|axis| axis * 0.5);
                let minimum = 1.0 / input.viewport_height;
                let before = distance(previous, other.position).max(minimum);
                let after = distance(position, other.position).max(minimum);
                motion.zoom = f64::from(before / after).ln();
            } else if contact.pan {
                motion.pan = delta;
            } else {
                motion.orbit = delta;
            }
            return Motion(motion);
        }
        if !release || contact.pan {
            return NoGesture;
        }
        if self.last_click.is_some_and(|(old, time)| {
            (0.0..=350.0).contains(&(input.timestamp_ms - time))
                && distance(position, old) <= 8.0 / input.viewport_height
        }) {
            self.last_click = None;
            Reset
        } else {
            self.last_click = Some((position, input.timestamp_ms));
            Click(position)
        }
    }
}
