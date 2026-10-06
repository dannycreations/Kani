use super::{AppState, QueueItemStatus};

fn add_three(state: &mut AppState) {
  for name in ["file1.mkv", "file2.mkv", "file3.mkv"] {
    state.add_file(name.to_string());
  }
}

fn inputs(state: &AppState) -> Vec<&str> {
  state.queue.iter().map(|item| &*item.input_path).collect()
}

fn outputs(state: &AppState) -> Vec<&str> {
  state.queue.iter().map(|item| &*item.output_path).collect()
}

#[test]
fn add_file_seeds_settings_from_the_default_preset() {
  let mut state = AppState::new();
  state.add_file("file1.mkv".to_string());

  let settings = &state.queue[0].settings;
  assert!(!settings.single_track);
  assert_eq!(settings.tracks.len(), 3);
  // The preset lists Mic/Discord/Game, sorted into input-stream order.
  assert_eq!(&*settings.tracks[0].name, "Game");
  assert_eq!(settings.tracks[0].offset, -16.0);
  assert_eq!(&*settings.tracks[2].name, "Discord");
  assert_eq!(settings.tracks[2].offset, -6.0);
}

#[test]
fn each_input_gets_its_own_mp4_output() {
  let mut state = AppState::new();
  add_three(&mut state);
  assert_eq!(outputs(&state), ["file1.mp4", "file2.mp4", "file3.mp4"]);
}

#[test]
fn pending_items_reorder_and_ignore_out_of_range_moves() {
  let mut state = AppState::new();
  add_three(&mut state);

  let second_id = state.queue[1].id;
  state.move_item(second_id, -1);
  assert_eq!(inputs(&state), ["file2.mkv", "file1.mkv", "file3.mkv"]);

  state.move_item(second_id, 1);
  assert_eq!(inputs(&state), ["file1.mkv", "file2.mkv", "file3.mkv"]);

  state.move_item(second_id, 5);
  assert_eq!(inputs(&state), ["file1.mkv", "file2.mkv", "file3.mkv"]);

  state.remove_item(state.queue[0].id);
  assert_eq!(inputs(&state), ["file2.mkv", "file3.mkv"]);
}

#[test]
fn clear_completed_drops_only_finished_items() {
  let mut state = AppState::new();
  add_three(&mut state);

  state.item_mut(state.queue[0].id).unwrap().status =
    QueueItemStatus::Completed;
  state.clear_completed();

  assert_eq!(inputs(&state), ["file2.mkv", "file3.mkv"]);
}
