use super::*;

pub(crate) fn axis(keys: &HashSet<KeyCode>, positive: &[KeyCode], negative: &[KeyCode]) -> f32 {
    f32::from(positive.iter().any(|key| keys.contains(key)))
        - f32::from(negative.iter().any(|key| keys.contains(key)))
}

pub(crate) fn joystick_movement((x, y): (f32, f32)) -> (f32, f32) {
    (-y, x)
}

pub(crate) fn should_forward_gameplay_keyboard(playing: bool, shell_consumed: bool) -> bool {
    playing || !shell_consumed
}
