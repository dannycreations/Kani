use std::{
  process::Command,
  sync::{Arc, Mutex},
};

use super::{kill_all_children, register_child, ACTIVE_CHILDREN};

#[test]
fn test_active_children_registration() {
  #[cfg(windows)]
  let mut cmd = Command::new("cmd");
  #[cfg(not(windows))]
  let mut cmd = Command::new("sh");

  #[cfg(windows)]
  cmd.args(["/C", "ping 127.0.0.1 -n 2"]);
  #[cfg(not(windows))]
  cmd.args(["-c", "sleep 1"]);

  if let Ok(child) = cmd.spawn() {
    let handle = Arc::new(Mutex::new(Some(child)));
    register_child(Arc::clone(&handle));

    {
      let lock = ACTIVE_CHILDREN.lock().unwrap();
      assert!(lock.iter().any(|h| Arc::ptr_eq(h, &handle)));
    }

    kill_all_children();

    {
      let lock = ACTIVE_CHILDREN.lock().unwrap();
      assert!(lock.is_empty());
    }

    let child_lock = handle.lock().unwrap();
    assert!(child_lock.is_none());
  }
}
