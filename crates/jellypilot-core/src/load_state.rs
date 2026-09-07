/// Lifecycle state for an asynchronous value.
#[derive(Clone, Default)]
pub enum LoadState<T, E = String> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(E),
}
