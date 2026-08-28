/// Lifecycle state for an asynchronous value.
#[derive(Clone, Default)]
pub enum LoadState<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}
