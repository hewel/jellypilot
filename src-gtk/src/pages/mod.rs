pub(crate) mod browse;
pub(crate) mod cards;
pub(crate) mod detail;
pub(crate) mod diagnostics;
pub(crate) mod home;
pub(crate) mod login;
pub(crate) mod player;
pub(crate) mod settings;

#[derive(Clone, Default)]
pub(crate) enum LoadState<T> {
  #[default]
  Idle,
  Loading,
  Ready(T),
  Failed(String),
}
