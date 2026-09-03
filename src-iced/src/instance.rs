use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{
  atomic::{AtomicBool, Ordering},
  Arc,
};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use interprocess::local_socket::GenericFilePath;
#[cfg(not(unix))]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{prelude::*, ListenerOptions, Stream};
use tokio::sync::{mpsc, Mutex};

const ACTIVATION: &[u8] = b"show\n";
#[derive(Clone)]
pub(crate) struct ActivationChannel {
  pub(crate) receiver: Arc<Mutex<mpsc::UnboundedReceiver<()>>>,
}
impl PartialEq for ActivationChannel {
  fn eq(&self, _other: &Self) -> bool {
    true
  }
}

impl Eq for ActivationChannel {}

impl Hash for ActivationChannel {
  fn hash<H: Hasher>(&self, hasher: &mut H) {
    "jellypilot-instance-activation".hash(hasher);
  }
}

pub(crate) struct Guard {
  channel: ActivationChannel,
  stop: Arc<AtomicBool>,
  thread: Option<thread::JoinHandle<()>>,
  unix_path: Option<PathBuf>,
}

pub(crate) enum Startup {
  Existing,
  Primary(Guard),
  Unavailable,
}

impl Guard {
  pub(crate) fn channel(&self) -> ActivationChannel {
    self.channel.clone()
  }
}

impl Drop for Guard {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Release);
    // Wake the blocking accept with a self-connection so the thread can
    // observe the stop flag and exit; the wake-up stream reads no message.
    if let Ok((raw_name, unix_path)) = socket_identity() {
      #[cfg(unix)]
      let name = unix_path
        .as_deref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or(raw_name)
        .to_fs_name::<GenericFilePath>();
      #[cfg(not(unix))]
      let name = raw_name.to_ns_name::<GenericNamespaced>();
      if let Ok(name) = name {
        let _ = Stream::connect(name);
      }
    }
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
    if let Some(path) = &self.unix_path {
      let _ = std::fs::remove_file(path);
    }
  }
}

pub(crate) fn acquire() -> Startup {
  let (raw_name, unix_path) = match socket_identity() {
    Ok(name) => name,
    Err(error) => {
      tracing::warn!(%error, "single-instance guard unavailable");
      return Startup::Unavailable;
    }
  };

  #[cfg(unix)]
  let name = match raw_name.to_fs_name::<GenericFilePath>() {
    Ok(name) => name,
    Err(error) => {
      tracing::warn!(%error, "single-instance socket name is invalid");
      return Startup::Unavailable;
    }
  };
  #[cfg(not(unix))]
  let name = match raw_name.to_ns_name::<GenericNamespaced>() {
    Ok(name) => name,
    Err(error) => {
      tracing::warn!(%error, "single-instance socket name is invalid");
      return Startup::Unavailable;
    }
  };

  match Stream::connect(name) {
    Ok(mut stream) => {
      if let Err(error) = stream.write_all(ACTIVATION) {
        tracing::warn!(%error, "could not activate existing JellyPilot instance");
      }
      return Startup::Existing;
    }
    Err(error) if stale_connect_error(&error) => {
      if let Some(path) = &unix_path {
        let _ = std::fs::remove_file(path);
      }
    }
    Err(error) => {
      tracing::warn!(%error, "single-instance check failed; continuing without guard");
      return Startup::Unavailable;
    }
  }

  let (raw_name, unix_path) = match socket_identity() {
    Ok(name) => name,
    Err(error) => {
      tracing::warn!(%error, "single-instance guard unavailable");
      return Startup::Unavailable;
    }
  };
  #[cfg(unix)]
  let name = match raw_name.to_fs_name::<GenericFilePath>() {
    Ok(name) => name,
    Err(error) => {
      tracing::warn!(%error, "single-instance socket name is invalid");
      return Startup::Unavailable;
    }
  };
  #[cfg(not(unix))]
  let name = match raw_name.to_ns_name::<GenericNamespaced>() {
    Ok(name) => name,
    Err(error) => {
      tracing::warn!(%error, "single-instance socket name is invalid");
      return Startup::Unavailable;
    }
  };
  let listener = match ListenerOptions::new().name(name).create_sync() {
    Ok(listener) => listener,
    Err(error) => {
      tracing::warn!(%error, "single-instance listener bind failed; continuing without guard");
      return Startup::Unavailable;
    }
  };
  let (sender, receiver) = mpsc::unbounded_channel();
  let channel = ActivationChannel {
    receiver: Arc::new(Mutex::new(receiver)),
  };
  let stop = Arc::new(AtomicBool::new(false));
  let thread_stop = stop.clone();
  let thread = match thread::Builder::new()
    .name("instance-activation".to_owned())
    .spawn(move || accept_loop(listener, sender, thread_stop))
  {
    Ok(thread) => thread,
    Err(error) => {
      tracing::warn!(%error, "single-instance listener thread failed; continuing without guard");
      if let Some(path) = &unix_path {
        let _ = std::fs::remove_file(path);
      }
      return Startup::Unavailable;
    }
  };

  Startup::Primary(Guard {
    channel,
    stop,
    thread: Some(thread),
    unix_path,
  })
}

fn accept_loop(
  listener: interprocess::local_socket::Listener,
  sender: mpsc::UnboundedSender<()>,
  stop: Arc<AtomicBool>,
) {
  // Blocking accept keeps an idle instance at zero wakeups; Guard::drop wakes
  // the accept with a self-connection so the stop flag is observed.
  while !stop.load(Ordering::Acquire) {
    match listener.accept() {
      Ok(mut stream) => {
        if stop.load(Ordering::Acquire) {
          break;
        }
        // The accepted stream is blocking by default; a silent peer would
        // otherwise hang the accept loop (and Guard::drop's join) forever.
        if stream.set_nonblocking(true).is_err() {
          continue;
        }
        let mut message = [0; ACTIVATION.len()];
        let deadline = std::time::Instant::now() + Duration::from_millis(250);
        let read = loop {
          match stream.read_exact(&mut message) {
            Ok(()) => break true,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
              if std::time::Instant::now() >= deadline || stop.load(Ordering::Acquire) {
                break false;
              }
              thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break false,
          }
        };
        if read && is_show_message(&message) {
          let _ = sender.send(());
        }
      }
      Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
      Err(error) => {
        tracing::warn!(%error, "single-instance activation accept failed");
        thread::sleep(Duration::from_millis(25));
      }
    }
  }
}

fn stale_connect_error(error: &io::Error) -> bool {
  matches!(
    error.kind(),
    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused | io::ErrorKind::AddrNotAvailable
  )
}

fn is_show_message(message: &[u8]) -> bool {
  message == ACTIVATION
}

fn socket_identity() -> io::Result<(String, Option<PathBuf>)> {
  let user = socket_user();
  #[cfg(unix)]
  {
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
      .or_else(|| std::env::var_os("TMPDIR"))
      .unwrap_or_else(|| "/tmp".into());
    let path = PathBuf::from(directory).join(format!("jellypilot-{user}.sock"));
    Ok((path.to_string_lossy().into_owned(), Some(path)))
  }
  #[cfg(not(unix))]
  {
    Ok((format!("jellypilot-{user}"), None))
  }
}

fn socket_user() -> String {
  std::env::var("USER")
    .or_else(|_| std::env::var("USERNAME"))
    .unwrap_or_else(|_| "default".to_owned())
    .chars()
    .map(|character| {
      if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
        character
      } else {
        '_'
      }
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn activation_message_is_exactly_show_line() {
    assert!(is_show_message(b"show\n"));
    assert!(!is_show_message(b"show"));
    assert!(!is_show_message(b"quit\n"));
  }

  #[test]
  fn only_expected_connect_errors_are_stale_candidates() {
    assert!(stale_connect_error(&io::Error::from(
      io::ErrorKind::NotFound
    )));
    assert!(stale_connect_error(&io::Error::from(
      io::ErrorKind::ConnectionRefused,
    )));
    assert!(!stale_connect_error(&io::Error::from(
      io::ErrorKind::PermissionDenied
    )));
  }

  #[test]
  fn socket_identity_is_user_scoped() {
    let (name, path) = socket_identity().expect("socket identity");
    assert!(name.contains("jellypilot-"));
    #[cfg(unix)]
    assert!(path.is_some_and(|path| path
      .extension()
      .is_some_and(|extension| extension == "sock")));
    #[cfg(not(unix))]
    assert!(path.is_none());
  }
  #[test]
  fn socket_user_has_safe_filename_characters() {
    let user = socket_user();
    assert!(user
      .chars()
      .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_')));
  }
}
