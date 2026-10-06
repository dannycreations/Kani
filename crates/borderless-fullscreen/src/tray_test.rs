use super::*;

fn pixel(icon: &[u8; TRAY_ICON_LEN], x: u32, y: u32) -> [u8; 4] {
  let start = ((y * TRAY_ICON_SIZE + x) * 4) as usize;
  let mut out = [0u8; 4];
  out.copy_from_slice(&icon[start..start + 4]);
  out
}

#[test]
fn tray_icon_is_a_plate_framed_by_four_corner_brackets() {
  let icon = &TRAY_ICON_BYTES;

  // Every bracket corner, plus the far end of each of its two arms.
  for (x, y) in [
    (2, 2),
    (8, 2),
    (8, 3),
    (2, 8),
    (29, 2),
    (23, 2),
    (23, 3),
    (29, 8),
    (2, 29),
    (8, 29),
    (8, 28),
    (2, 23),
    (29, 29),
    (23, 29),
    (23, 28),
    (29, 23),
  ] {
    assert_eq!(pixel(icon, x, y), BRACKET_COLOR, "bracket at {x},{y}");
  }

  // One pixel past the arm on the plate's top edge, so an arm that
  // overshoots its length is caught.
  assert_eq!(pixel(icon, 9, 3), PLATE_COLOR);
  assert_eq!(pixel(icon, 15, 15), PLATE_COLOR, "plate center");

  // The margin outside the plate stays transparent.
  assert_eq!(pixel(icon, 1, 1), [0, 0, 0, 0]);
  assert_eq!(pixel(icon, 30, 30), [0, 0, 0, 0]);
}
