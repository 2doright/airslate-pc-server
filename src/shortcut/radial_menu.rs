use super::domain::{RadialInnerSlot, ScreenPoint};

const CENTER_RADIUS: f32 = 34.0;
const INNER_RADIUS: f32 = 132.0;
const OUTER_RADIUS: f32 = 212.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadialSelection {
    Center,
    Inner(RadialInnerSlot),
    Outer(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadialAnchor {
    pub center: ScreenPoint,
}

impl RadialAnchor {
    pub fn new(center: ScreenPoint) -> Self {
        Self { center }
    }
}

pub fn selection_from_offset(dx: f32, dy: f32, inner_enabled: bool) -> RadialSelection {
    let radius = dx.hypot(dy);
    if radius <= CENTER_RADIUS {
        return RadialSelection::Center;
    }

    let clockwise_from_top = angle_from_top(dx, dy);
    if inner_enabled && radius <= INNER_RADIUS {
        return RadialSelection::Inner(inner_slot(clockwise_from_top));
    }

    if radius <= OUTER_RADIUS {
        return RadialSelection::Outer(outer_slot(clockwise_from_top));
    }

    RadialSelection::Outer(outer_slot(clockwise_from_top))
}

pub fn anchor_from_point(point: ScreenPoint) -> RadialAnchor {
    RadialAnchor::new(point)
}

fn angle_from_top(dx: f32, dy: f32) -> f32 {
    let raw = dy.atan2(dx).to_degrees();
    (raw + 90.0).rem_euclid(360.0)
}

fn inner_slot(angle: f32) -> RadialInnerSlot {
    match (((angle + 45.0) / 90.0).floor() as i32).rem_euclid(4) {
        0 => RadialInnerSlot::Top,
        1 => RadialInnerSlot::Right,
        2 => RadialInnerSlot::Bottom,
        _ => RadialInnerSlot::Left,
    }
}

fn outer_slot(angle: f32) -> usize {
    (((angle + 22.5) / 45.0).floor() as i32).rem_euclid(8) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_radius_hits_center() {
        assert_eq!(
            selection_from_offset(10.0, 12.0, true),
            RadialSelection::Center
        );
        assert_eq!(
            selection_from_offset(10.0, 12.0, false),
            RadialSelection::Center
        );
    }

    #[test]
    fn top_hits_inner_top_slot() {
        assert_eq!(
            selection_from_offset(0.0, -56.0, true),
            RadialSelection::Inner(RadialInnerSlot::Top)
        );
    }

    #[test]
    fn right_hits_inner_right_slot() {
        assert_eq!(
            selection_from_offset(56.0, 0.0, true),
            RadialSelection::Inner(RadialInnerSlot::Right)
        );
    }

    #[test]
    fn bottom_hits_inner_bottom_slot() {
        assert_eq!(
            selection_from_offset(0.0, 56.0, true),
            RadialSelection::Inner(RadialInnerSlot::Bottom)
        );
    }

    #[test]
    fn left_hits_inner_left_slot() {
        assert_eq!(
            selection_from_offset(-56.0, 0.0, true),
            RadialSelection::Inner(RadialInnerSlot::Left)
        );
    }

    #[test]
    fn movement_inside_inner_ring_stays_inner() {
        assert_eq!(
            selection_from_offset(110.0, 0.0, true),
            RadialSelection::Inner(RadialInnerSlot::Right)
        );
        assert_eq!(
            selection_from_offset(0.0, -110.0, true),
            RadialSelection::Inner(RadialInnerSlot::Top)
        );
    }

    #[test]
    fn disabled_inner_ring_maps_inner_radius_to_outer_sector() {
        assert_eq!(
            selection_from_offset(110.0, 0.0, false),
            RadialSelection::Outer(2)
        );
        assert_eq!(
            selection_from_offset(0.0, -110.0, false),
            RadialSelection::Outer(0)
        );
        assert_eq!(
            selection_from_offset(-110.0, 0.0, false),
            RadialSelection::Outer(6)
        );
        assert_eq!(
            selection_from_offset(0.0, 110.0, false),
            RadialSelection::Outer(4)
        );
    }

    #[test]
    fn horizontal_and_vertical_axes_hit_outer_sector_centers() {
        assert_eq!(
            selection_from_offset(160.0, 0.0, true),
            RadialSelection::Outer(2)
        );
        assert_eq!(
            selection_from_offset(0.0, -160.0, true),
            RadialSelection::Outer(0)
        );
        assert_eq!(
            selection_from_offset(-160.0, 0.0, true),
            RadialSelection::Outer(6)
        );
        assert_eq!(
            selection_from_offset(0.0, 160.0, true),
            RadialSelection::Outer(4)
        );
    }
}
