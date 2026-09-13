//! Linux session idle inhibition for embedded playback.
//!
//! The pinned libmpv fork renders through the gpu-next host context, whose
//! `host_control` answers `VO_NOTIMPL` to `VOCTRL_KILL_SCREENSAVER`: embedded
//! playback therefore never reaches mpv's own screensaver handling. The
//! application owns inhibition instead, mirroring mpv's `playback_active`
//! semantics (inhibit only while a file is loaded and unpaused).
//!
//! Wayland binds `zwp_idle_inhibit_manager_v1` on the presentation surface;
//! X11 suspends the screensaver on a private connection (suspension is
//! per-connection and lifts automatically if the process dies). Both are
//! driven by the same desired/surface reconciliation in [`IdleInhibit`].
//!
//! Handle lifetime contract: the caller transfers native display and window
//! owners ([`std::any::Any`] clones) into this struct for as long as a binding
//! exists, so the raw handles passed to the platform layer cannot dangle.
//! Soundness rests on that owner retention alone: it is required regardless
//! of whether the backend offers liveness tracking for the adopted objects —
//! nothing here detects the owner destroying the surface, and teardown sends
//! explicit protocol `destroy` requests because dropping our backend never
//! disconnects the borrowed display.

use std::any::Any;
use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

use crate::Error;

/// Opaque owner keeping a native display or window object alive.
type Owner = Arc<dyn Any + Send + Sync>;

/// A bound inhibitor plus the owners that keep its raw handles valid.
///
/// `_bound` is an opaque RAII guard: dropping it performs the protocol
/// teardown (Wayland `destroy` requests, X11 resume) on a best-effort basis —
/// teardown requests are queued and flushed, but a failed flush is silently
/// discarded. There is no consumer for release errors, and the server drops
/// the objects when the display connection closes anyway.
struct Active {
    _bound: Box<dyn Any + Send>,
    _display: Owner,
    _window: Owner,
}

/// Selects how [`IdleInhibit`] binds a platform inhibitor.
enum Binder {
    Native,
    #[cfg(test)]
    Recording(Arc<std::sync::Mutex<Vec<Event>>>),
}

/// Reconciles the desired inhibition state with the registered display and
/// presentation surface. All methods are idempotent; callers may invoke them
/// on every projection update.
pub struct IdleInhibit {
    /// Declared first so the bound inhibitor is released before the surface
    /// and display owners whose handles it was created from.
    active: Option<Active>,
    binder: Binder,
    desired: bool,
    /// Set once the compositor or server reports the mechanism absent; avoids
    /// re-attempting a bind that can never succeed until the display changes.
    unsupported: bool,
    display: Option<(RawDisplayHandle, Owner)>,
    surface: Option<SurfaceSlot>,
    /// Last bind failure, for diagnostics and the native regression report.
    last_error: Option<String>,
    next_token: u64,
}

struct SurfaceSlot {
    token: u64,
    handle: RawWindowHandle,
    owner: Owner,
}

// SAFETY: the raw display/window handles are only ever read to pass their
// pointers to thread-safe libwayland/Xlib calls; the owners that keep the
// pointed-to objects alive are `Send + Sync`, and every mutation of this
// struct happens under the caller's lock.
unsafe impl Send for IdleInhibit {}

impl IdleInhibit {
    pub const fn new() -> Self {
        Self {
            active: None,
            binder: Binder::Native,
            desired: false,
            unsupported: false,
            display: None,
            surface: None,
            last_error: None,
            next_token: 0,
        }
    }

    /// Takes ownership of the display once at compositor creation. The raw
    /// handle is extracted from the exact retained owner, so the stored
    /// pointer cannot outlive the object it names; a changed display
    /// invalidates any binding bound against the old one.
    pub fn display_changed<D>(&mut self, display: D)
    where
        D: HasDisplayHandle + Send + Sync + 'static,
    {
        let handle = display.display_handle().ok().map(|handle| handle.as_raw());
        self.display = handle.map(|handle| (handle, Arc::new(display) as Owner));
        self.unsupported = false;
        self.active = None;
        self.sync();
    }

    /// Registers the surface backing a newly created compositor surface and
    /// returns its token for [`Self::surface_dropped`]. Any existing binding
    /// targeted the previous surface and is rebound when desired.
    pub fn surface_changed<W>(&mut self, window: W) -> u64
    where
        W: HasWindowHandle + Send + Sync + 'static,
    {
        self.next_token += 1;
        let token = self.next_token;
        let handle = window.window_handle().ok().map(|handle| handle.as_raw());
        self.surface = handle.map(|handle| SurfaceSlot {
            token,
            handle,
            owner: Arc::new(window),
        });
        self.active = None;
        self.sync();
        token
    }

    /// Drops the registration for a destroyed surface. A stale drop (the
    /// compositor creates the replacement surface before dropping the old one)
    /// must not clear the newer registration or its binding.
    pub fn surface_dropped(&mut self, token: u64) {
        if self
            .surface
            .as_ref()
            .is_some_and(|slot| slot.token == token)
        {
            self.surface = None;
            self.active = None;
        }
    }

    /// Drives the desired state from the playback projection. Acquires on the
    /// rising edge, releases on the falling edge, and is a no-op otherwise.
    pub fn set_desired(&mut self, desired: bool) {
        self.desired = desired;
        self.sync();
    }

    /// Releases any binding and forgets all registered handles. Used at
    /// compositor teardown; `desired` is preserved so a later compositor can
    /// re-acquire without the session re-asserting.
    pub fn shutdown(&mut self) {
        self.active = None;
        self.surface = None;
        self.display = None;
        self.unsupported = false;
    }

    /// Whether a platform inhibitor is currently bound.
    pub fn inhibited(&self) -> bool {
        self.active.is_some()
    }

    /// Whether inhibition is currently requested by the session.
    pub fn desired(&self) -> bool {
        self.desired
    }

    /// The most recent bind failure, cleared on the next successful bind.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    fn sync(&mut self) {
        if !self.desired || self.unsupported {
            self.active = None;
            return;
        }
        if self.active.is_some() {
            return;
        }
        let (Some((display, display_owner)), Some(surface)) =
            (self.display.clone(), self.surface.as_ref())
        else {
            return;
        };
        let bound = match &self.binder {
            Binder::Native => bind_native(display, surface.handle),
            #[cfg(test)]
            Binder::Recording(events) => {
                events
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(Event::Acquired);
                Ok(Some(Box::new(RecordedBound {
                    events: Arc::clone(events),
                }) as Box<dyn Any + Send>))
            }
        };
        match bound {
            Ok(Some(bound)) => {
                self.last_error = None;
                self.active = Some(Active {
                    _bound: bound,
                    _display: display_owner,
                    _window: Arc::clone(&surface.owner),
                });
            }
            Ok(None) => {
                self.unsupported = true;
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
            }
        }
    }
}

impl Default for IdleInhibit {
    fn default() -> Self {
        Self::new()
    }
}

/// Binds a platform inhibitor for the given handles.
///
/// `Ok(None)` means the compositor or server lacks the mechanism; `Err` is a
/// transient failure worth retrying on the next state change.
fn bind_native(
    display: RawDisplayHandle,
    surface: RawWindowHandle,
) -> Result<Option<Box<dyn Any + Send>>, Error> {
    match display {
        RawDisplayHandle::Wayland(display) => match surface {
            RawWindowHandle::Wayland(surface) => {
                // SAFETY: the registered owners keep the display and window
                // objects alive for the binding's lifetime; the handles are
                // real libwayland objects from winit's client_system backend.
                unsafe {
                    wayland::Inhibitor::new(
                        display.display.as_ptr().cast(),
                        surface.surface.as_ptr().cast(),
                    )
                }
                .map(|inhibitor| inhibitor.map(|i| Box::new(i) as Box<dyn Any + Send>))
            }
            _ => Err(Error::from("Wayland display with a non-Wayland surface")),
        },
        RawDisplayHandle::Xcb(_) | RawDisplayHandle::Xlib(_) => x11::Inhibitor::new()
            .map(|inhibitor| inhibitor.map(|i| Box::new(i) as Box<dyn Any + Send>)),
        _ => Ok(None),
    }
}

#[cfg(target_os = "linux")]
mod wayland {
    use std::ffi::c_void;
    use wayland_client::backend::{Backend, ObjectId};
    use wayland_client::globals::{registry_queue_init, GlobalList, GlobalListContents};
    use wayland_client::protocol::wl_registry::WlRegistry;
    use wayland_client::protocol::wl_surface::WlSurface;
    use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
    use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1;
    use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1;

    use super::Error;

    /// Wayland state for the inhibitor's private event queue. The registry's
    /// global list is maintained internally by `registry_queue_init`; the
    /// inhibitor objects emit no events.
    struct State;

    impl Dispatch<WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: <WlRegistry as Proxy>::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwpIdleInhibitManagerV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &ZwpIdleInhibitManagerV1,
            _: <ZwpIdleInhibitManagerV1 as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwpIdleInhibitorV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &ZwpIdleInhibitorV1,
            _: <ZwpIdleInhibitorV1 as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    /// A `zwp_idle_inhibitor_v1` bound on the application's surface.
    ///
    /// Field order matters: `inhibitor`, `manager`, and `globals` are
    /// destroyed before `connection`/`_queue` so their teardown requests still
    /// reach the wire. The caller retains the window owner, keeping
    /// `wl_surface` alive for the inhibitor's lifetime.
    pub struct Inhibitor {
        inhibitor: ZwpIdleInhibitorV1,
        manager: ZwpIdleInhibitManagerV1,
        globals: Option<GlobalList>,
        connection: Connection,
        _queue: EventQueue<State>,
    }

    impl Inhibitor {
        /// Binds `zwp_idle_inhibit_manager_v1` on the foreign display and
        /// creates an inhibitor for `surface`.
        ///
        /// # Safety
        /// `display` and `surface` must be live libwayland `wl_display` /
        /// `wl_surface` objects owned by the caller, and must remain valid
        /// until the returned inhibitor is dropped. Owner retention is
        /// required regardless of whether the backend tracks liveness for the
        /// adopted ids; the caller's contract is the sole guarantee that the
        /// surface outlives the binding.
        pub unsafe fn new(
            display: *mut c_void,
            surface: *mut c_void,
        ) -> Result<Option<Self>, Error> {
            // SAFETY: the caller guarantees `display` is a live wl_display.
            let backend = unsafe { Backend::from_foreign_display(display.cast()) };
            let connection = Connection::from_backend(backend);
            let (globals, queue) = registry_queue_init::<State>(&connection)
                .map_err(|_| Error::from("Wayland registry roundtrip failed"))?;
            // SAFETY: the caller guarantees `surface` is a live wl_surface;
            // from_ptr also verifies the interface name matches. Adopted
            // before any object creation so a failure leaves nothing behind.
            let surface = unsafe {
                ObjectId::from_ptr(WlSurface::interface(), surface.cast())
                    .and_then(|id| WlSurface::from_id(&connection, id))
                    .map_err(|_| Error::from("surface handle is not a wl_surface"))
            };
            let surface = match surface {
                Ok(surface) => surface,
                Err(error) => {
                    globals.destroy();
                    let _ = connection.flush();
                    return Err(error);
                }
            };
            // Everything after this point must release the globals on
            // failure; the borrowed display outlives us, so nothing cleans
            // these objects up implicitly.
            let built = (|| {
                let manager = match globals.bind::<ZwpIdleInhibitManagerV1, State, ()>(
                    &queue.handle(),
                    1..=1,
                    (),
                ) {
                    Ok(manager) => manager,
                    Err(wayland_client::globals::BindError::NotPresent) => return Ok(None),
                    Err(error) => return Err(Error::from(format!("idle-inhibit bind: {error}"))),
                };
                let inhibitor = manager.create_inhibitor(&surface, &queue.handle(), ());
                if connection.flush().is_err() {
                    // Queue the teardown requests so a later successful flush
                    // cannot leave a live inhibitor behind after we report
                    // failure.
                    inhibitor.destroy();
                    manager.destroy();
                    return Err(Error::from("Wayland flush failed"));
                }
                Ok(Some((inhibitor, manager)))
            })();
            match built {
                Ok(Some((inhibitor, manager))) => Ok(Some(Self {
                    inhibitor,
                    manager,
                    globals: Some(globals),
                    connection,
                    _queue: queue,
                })),
                Ok(None) => {
                    globals.destroy();
                    let _ = connection.flush();
                    Ok(None)
                }
                Err(error) => {
                    globals.destroy();
                    let _ = connection.flush();
                    Err(error)
                }
            }
        }
    }

    impl Drop for Inhibitor {
        fn drop(&mut self) {
            self.inhibitor.destroy();
            self.manager.destroy();
            if let Some(globals) = self.globals.take() {
                globals.destroy();
            }
            let _ = self.connection.flush();
        }
    }
}

mod x11 {
    use x11rb::connection::Connection;
    use x11rb::errors::ConnectionError;
    use x11rb::protocol::screensaver;
    use x11rb::rust_connection::RustConnection;

    use super::Error;

    /// An XScreenSaver suspension held on a private connection. Suspension is
    /// per-connection, so process death releases it automatically; `resume`
    /// lifts it explicitly while the connection is still open.
    pub struct Inhibitor {
        connection: RustConnection,
    }

    impl Inhibitor {
        /// Suspends the screensaver on `$DISPLAY`. Returns `Ok(None)` when the
        /// server lacks the MIT-SCREEN-SAVER extension or predatesSuspend (1.1).
        pub fn new() -> Result<Option<Self>, Error> {
            let (connection, _) =
                x11rb::connect(None).map_err(|error| Error::from(error.to_string()))?;
            let version = match screensaver::query_version(&connection, 1, 1) {
                Ok(cookie) => match cookie.reply() {
                    Ok(reply) => reply,
                    Err(_) => return Err(Error::from("XScreenSaver version query failed")),
                },
                Err(ConnectionError::UnsupportedExtension) => return Ok(None),
                Err(error) => return Err(Error::from(error.to_string())),
            };
            if version.server_major_version < 1
                || (version.server_major_version == 1 && version.server_minor_version < 1)
            {
                return Ok(None);
            }
            let inhibitor = Self { connection };
            inhibitor.suspend(1)?;
            Ok(Some(inhibitor))
        }

        fn suspend(&self, suspend: u32) -> Result<(), Error> {
            screensaver::suspend(&self.connection, suspend)
                .map_err(|error| Error::from(error.to_string()))?
                .check()
                .map_err(|error| Error::from(error.to_string()))?;
            self.connection
                .flush()
                .map_err(|error| Error::from(error.to_string()))
        }
    }

    impl Drop for Inhibitor {
        /// Lifts the suspension best-effort; closing the connection would
        /// release it anyway.
        fn drop(&mut self) {
            let _ = self.suspend(0);
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Acquired,
    Released,
}

#[cfg(test)]
struct RecordedBound {
    events: Arc<std::sync::Mutex<Vec<Event>>>,
}

#[cfg(test)]
impl Drop for RecordedBound {
    fn drop(&mut self) {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Event::Released);
    }
}

#[cfg(test)]
impl IdleInhibit {
    /// A controller whose binder records acquire/release events instead of
    /// touching the platform. Exists to prove the reconciliation invariants —
    /// especially stale surface drops — without a compositor.
    fn recording(events: Arc<std::sync::Mutex<Vec<Event>>>) -> Self {
        Self {
            binder: Binder::Recording(events),
            ..Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raw_window_handle::{
        DisplayHandle, HandleError, RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle,
        WaylandWindowHandle, WindowHandle,
    };
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::Mutex;

    /// A fake owner whose handles point at a fixed sentinel address. The
    /// recording binder never dereferences them; they exist only to exercise
    /// the handle/owner plumbing.
    struct FakeWindow(u8);

    impl HasWindowHandle for FakeWindow {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let handle = WaylandWindowHandle::new(
                NonNull::new(&self.0 as *const u8 as *mut c_void).unwrap(),
            );
            // SAFETY: the sentinel pointer is never dereferenced by the
            // recording binder; it only distinguishes handle identity.
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Wayland(handle)) })
        }
    }

    #[derive(Clone)]
    struct FakeDisplay;

    impl HasDisplayHandle for FakeDisplay {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            let handle = WaylandDisplayHandle::new(
                NonNull::new(std::ptr::dangling_mut::<c_void>()).unwrap(),
            );
            // SAFETY: see FakeWindow.
            Ok(unsafe { DisplayHandle::borrow_raw(RawDisplayHandle::Wayland(handle)) })
        }
    }
    fn events() -> Arc<Mutex<Vec<Event>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn taken(events: &Arc<Mutex<Vec<Event>>>) -> Vec<Event> {
        std::mem::take(
            &mut events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    #[test]
    fn acquires_on_desired_with_surface_and_releases_on_undesired() {
        let events = events();
        let mut inhibit = IdleInhibit::recording(Arc::clone(&events));
        inhibit.display_changed(&FakeDisplay);
        inhibit.surface_changed(FakeWindow(1));

        inhibit.set_desired(true);
        assert!(inhibit.inhibited());
        inhibit.set_desired(true);
        assert_eq!(taken(&events), [Event::Acquired]);

        inhibit.set_desired(false);
        assert!(!inhibit.inhibited());
        assert_eq!(taken(&events), [Event::Released]);
    }

    #[test]
    fn binds_when_surface_arrives_after_desired() {
        let events = events();
        let mut inhibit = IdleInhibit::recording(Arc::clone(&events));
        inhibit.display_changed(&FakeDisplay);
        inhibit.set_desired(true);
        assert!(!inhibit.inhibited());

        inhibit.surface_changed(FakeWindow(1));
        assert!(inhibit.inhibited());
        assert_eq!(taken(&events), [Event::Acquired]);
    }

    #[test]
    fn stale_surface_drop_keeps_the_newer_binding() {
        let events = events();
        let mut inhibit = IdleInhibit::recording(Arc::clone(&events));
        inhibit.display_changed(&FakeDisplay);
        let old = inhibit.surface_changed(FakeWindow(1));
        inhibit.set_desired(true);
        assert_eq!(taken(&events), [Event::Acquired]);

        // The compositor creates the replacement surface before dropping the
        // old one (SurfaceError::Lost path); the stale drop must not release
        // the new binding.
        let new = inhibit.surface_changed(FakeWindow(2));
        assert_eq!(taken(&events), [Event::Released, Event::Acquired]);
        inhibit.surface_dropped(old);
        assert!(inhibit.inhibited());
        assert_eq!(taken(&events), []);

        inhibit.surface_dropped(new);
        assert!(!inhibit.inhibited());
        assert_eq!(taken(&events), [Event::Released]);
    }

    #[test]
    fn surface_drop_releases_and_reopen_reacquires() {
        let events = events();
        let mut inhibit = IdleInhibit::recording(Arc::clone(&events));
        inhibit.display_changed(&FakeDisplay);
        let token = inhibit.surface_changed(FakeWindow(1));
        inhibit.set_desired(true);
        assert_eq!(taken(&events), [Event::Acquired]);

        inhibit.surface_dropped(token);
        assert!(!inhibit.inhibited());
        assert_eq!(taken(&events), [Event::Released]);

        // Desired survived the close: reopening the window rebinds.
        inhibit.surface_changed(FakeWindow(2));
        assert!(inhibit.inhibited());
        assert_eq!(taken(&events), [Event::Acquired]);
    }

    #[test]
    fn shutdown_releases_and_forgets_handles() {
        let events = events();
        let mut inhibit = IdleInhibit::recording(Arc::clone(&events));
        inhibit.display_changed(&FakeDisplay);
        inhibit.surface_changed(FakeWindow(1));
        inhibit.set_desired(true);
        assert_eq!(taken(&events), [Event::Acquired]);

        inhibit.shutdown();
        assert!(!inhibit.inhibited());
        assert_eq!(taken(&events), [Event::Released]);

        // Desired persists, but without handles nothing rebinds until the
        // next compositor registers its display and surface.
        inhibit.set_desired(true);
        assert!(!inhibit.inhibited());
        assert_eq!(taken(&events), []);
    }
}
