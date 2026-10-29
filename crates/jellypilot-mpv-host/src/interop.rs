use crate::{completion::Completion, gpu::DeviceContext, pool::Lease, Error, HostAbi, HostOptions};
use ash::vk;
use iced_wgpu::wgpu;
use parking_lot::{lock_api::RawMutex as _, Mutex, RawMutex};
use std::{
    collections::VecDeque,
    ffi::{c_char, c_int, c_void, CString},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

// Exact repr(C) translation of the published mpv/include/mpv/gpu_next.h.
#[repr(C)]
#[derive(Clone, Copy)]
struct Target {
    image: vk::Image,
    width: c_int,
    height: c_int,
    layout: vk::ImageLayout,
    usage: vk::ImageUsageFlags,
    token: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Frame {
    target: Target,
    semaphore: vk::Semaphore,
    wait_value: u64,
    signal_value: u64,
}

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

/// Mirrors `mpv_gpu_next_color` (host ABI version 2). The integer fields
/// numerically match libplacebo's `pl_color_primaries` / `pl_color_transfer`.
/// Zero fields are unknown and fall back to mpv's fixed SDR constants.
/// `depth` is reserved: the host image is always 10-bit, so pass 0 or 10.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TargetColor {
    pub primaries: c_int,
    pub transfer: c_int,
    pub ref_luma: f32,
    pub min_luma: f32,
    pub max_luma: f32,
    pub depth: c_int,
}
impl TargetColor {
    /// BT.709 gamma-2.2 SDR target with a near-zero black level.
    pub const SDR: Self = Self {
        primaries: 0,
        transfer: 0,
        ref_luma: 0.0,
        // Zero selects the host's 0.203-nit fallback and lifts SDR blacks.
        // Match mpv's infinite-contrast target instead.
        min_luma: 1e-7,
        max_luma: 0.0,
        depth: 0,
    };
    /// HDR10: BT.2020 primaries, PQ transfer, 203 nit reference white, 1000 nit
    /// peak, 10-bit. Static metadata is not signaled in ABI v2.
    pub const HDR10: Self = Self {
        primaries: 6,
        transfer: 12,
        ref_luma: 203.0,
        min_luma: 0.0001,
        max_luma: 1000.0,
        depth: 10,
    };
}
#[repr(C)]
struct Descriptor {
    version: u32,
    instance: vk::Instance,
    physical_device: vk::PhysicalDevice,
    device: vk::Device,
    get_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    queue_family: u32,
    features: *const vk::PhysicalDeviceFeatures2<'static>,
    extensions: *const *const c_char,
    num_extensions: c_int,
    opaque: *mut c_void,
    lock_queue: unsafe extern "C" fn(*mut c_void, u32, u32),
    unlock_queue: unsafe extern "C" fn(*mut c_void, u32, u32),
    acquire: unsafe extern "C" fn(*mut c_void, *mut Target) -> c_int,
    release: unsafe extern "C" fn(*mut c_void, *const Target, c_int),
    target_color: unsafe extern "C" fn(*mut c_void, *mut TargetColor),
    acquire_frame: Option<unsafe extern "C" fn(*mut c_void, *mut Frame) -> c_int>,
    release_frame: Option<unsafe extern "C" fn(*mut c_void, *const Frame, c_int)>,
}

// The descriptor prefixes are read according to version, never by Rust's full size.
const _: () = assert!(
    std::mem::offset_of!(Descriptor, acquire_frame)
        == std::mem::offset_of!(Descriptor, target_color) + std::mem::size_of::<usize>()
);

/// Shared external Vulkan queue synchronization for mpv and the compositor.
/// Acquire around submit/present/configure, not an entire renderer frame.
pub struct QueueLock(RawMutex);
impl QueueLock {
    pub(crate) fn new() -> Self {
        Self(RawMutex::INIT)
    }
    pub fn lock(&self) -> QueueGuard<'_> {
        self.0.lock();
        QueueGuard(self, std::marker::PhantomData)
    }
}
// SAFETY: RawMutex excludes both compositor and mpv queue operations. A successful
// lock owns the mutex until the same thread calls unlock; neither operation
// unwinds after changing ownership. Guards cannot transfer ownership to a thread.
unsafe impl iced_wgpu::QueueSynchronization for QueueLock {
    fn lock(&self) {
        self.0.lock();
    }

    unsafe fn unlock(&self) {
        // SAFETY: the synchronization trait requires current-thread ownership.
        unsafe { self.0.unlock() };
    }
}
/// Releases the shared queue lock when leaving a compositor operation.
pub struct QueueGuard<'a>(&'a QueueLock, std::marker::PhantomData<std::rc::Rc<()>>);
impl Drop for QueueGuard<'_> {
    fn drop(&mut self) {
        unsafe { self.0 .0.unlock() }
    }
}

struct Slot {
    target: Target,
    memory: vk::DeviceMemory,
    lease: Lease,
    semaphore: vk::Semaphore,
    color_epoch: u64,
}
struct Pool {
    slots: Vec<Slot>,
    ready: VecDeque<usize>,
    stopping: bool,
    size: (u32, u32),
    color: TargetColor,
    color_epoch: u64,
    error: Option<String>,
}

impl Pool {
    fn fail(&mut self, error: impl Into<String>) {
        self.error.get_or_insert_with(|| error.into());
        self.stopping = true;
    }

    fn acquire(&mut self) -> Result<Option<Frame>, Error> {
        if self.stopping {
            return Ok(None);
        }
        let Some(slot) = self.slots.iter_mut().find(|slot| {
            slot.lease.is_idle()
                && (slot.target.width as u32, slot.target.height as u32) == self.size
        }) else {
            return Ok(None);
        };
        let Some((wait_value, signal_value)) = slot.lease.acquire() else {
            self.fail("embedded MPV host timeline exhausted");
            return Err("embedded MPV host timeline exhausted".into());
        };
        slot.color_epoch = self.color_epoch;
        Ok(Some(Frame {
            target: slot.target,
            semaphore: slot.semaphore,
            wait_value,
            signal_value,
        }))
    }

    /// Even stale-color releases need the host's GPU handback before reuse.
    fn release(&mut self, token: u64, status: c_int) -> Result<bool, Error> {
        let index = usize::try_from(token).map_err(|_| Error::from("invalid MPV target token"))?;
        let slot = self
            .slots
            .get_mut(index)
            .ok_or("invalid MPV target token")?;
        if !slot.lease.release(status == 0) {
            return Err("duplicate or unacquired MPV target release".into());
        }
        if status != 0 {
            slot.target.layout = vk::ImageLayout::UNDEFINED;
            // Error release promises no odd signal. Quarantine, never enqueue
            // a wait or manufacture completion for this lease.
            self.fail("mpv discarded a GPU target; embedded host session stopped");
            return Ok(false);
        }
        slot.target.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        let stale = slot.color_epoch != self.color_epoch;
        self.ready.push_back(index);
        Ok(stale)
    }

    fn begin_copy(&mut self) -> Result<Option<(Frame, u64)>, Error> {
        if self.stopping {
            return Ok(None);
        }
        let Some(index) = self.ready.pop_front() else {
            return Ok(None);
        };
        let slot = &mut self.slots[index];
        let signal_value = slot.lease.copy().ok_or("invalid MPV copy ownership")?;
        Ok(Some((
            Frame {
                target: slot.target,
                semaphore: slot.semaphore,
                wait_value: signal_value,
                signal_value: signal_value + 1,
            },
            slot.color_epoch,
        )))
    }

    fn complete(&mut self, index: usize) -> Result<(), Error> {
        let slot = self
            .slots
            .get_mut(index)
            .ok_or("invalid MPV completion token")?;
        if !slot.lease.complete() {
            return Err("invalid MPV copy completion ownership".into());
        }
        slot.target.layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
        Ok(())
    }

    fn set_target_color(&mut self, color: TargetColor) -> bool {
        if self.color == color {
            return false;
        }
        let Some(epoch) = self.color_epoch.checked_add(1) else {
            self.fail("embedded MPV color epoch exhausted");
            return true;
        };
        self.color = color;
        self.color_epoch = epoch;
        true
    }
}
struct Shared {
    // Retained by the completion worker through shutdown and GPU completion.
    context: Arc<DeviceContext>,
    raw: ash::Device,
    memory: vk::PhysicalDeviceMemoryProperties,
    family: u32,
    queue_lock: Arc<QueueLock>,
    pool: Mutex<Pool>,
    exhausted: AtomicBool,
    asynchronous: bool,
    error_reported: AtomicBool,
    wake: Box<dyn Fn() + Send + Sync>,
}
impl Shared {
    fn notify(&self) {
        // Errors are retained for reopen, but must not remain silent while the
        // last window is closed. Never perform diagnostic I/O under pool/queue gates.
        if !self.error_reported.load(Ordering::Acquire) {
            let error = self.pool.try_lock().and_then(|pool| pool.error.clone());
            if let Some(error) = error {
                if !self.error_reported.swap(true, Ordering::AcqRel) {
                    use std::io::Write as _;
                    let _ = writeln!(std::io::stderr(), "{error}");
                }
            }
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.wake)()));
    }

    fn fail(&self, error: impl Into<String>) {
        self.pool.lock().fail(error);
        self.notify();
    }

    fn allocate(&self, width: u32, height: u32, token: u64) -> Result<Slot, Error> {
        unsafe {
            let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC;
            let image = self.raw.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(vk::Format::A2B10G10R10_UNORM_PACK32)
                    .extent(vk::Extent3D {
                        width,
                        height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?;
            let requirements = self.raw.get_image_memory_requirements(image);
            let memory_type = (0..self.memory.memory_type_count).find(|&i| {
                requirements.memory_type_bits & (1 << i) != 0
                    && self.memory.memory_types[i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            });
            let Some(memory_type) = memory_type else {
                self.raw.destroy_image(image, None);
                return Err("No device-local Vulkan image memory".into());
            };
            let memory = match self.raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(memory_type),
                None,
            ) {
                Ok(memory) => memory,
                Err(error) => {
                    self.raw.destroy_image(image, None);
                    return Err(error.into());
                }
            };
            if let Err(error) = self.raw.bind_image_memory(image, memory, 0) {
                self.raw.destroy_image(image, None);
                self.raw.free_memory(memory, None);
                return Err(error.into());
            }
            Ok(Slot {
                target: Target {
                    image,
                    width: width as i32,
                    height: height as i32,
                    layout: vk::ImageLayout::UNDEFINED,
                    usage,
                    token,
                },
                memory,
                lease: Lease::default(),
                semaphore: vk::Semaphore::null(),
                color_epoch: 0,
            })
        }
    }
}
impl Drop for Shared {
    fn drop(&mut self) {
        for slot in &self.pool.get_mut().slots {
            unsafe {
                self.raw.destroy_image(slot.target.image, None);
                self.raw.free_memory(slot.memory, None);
                if slot.semaphore != vk::Semaphore::null() {
                    self.raw.destroy_semaphore(slot.semaphore, None);
                }
            }
        }
    }
}
unsafe extern "C" fn lock_queue(opaque: *mut c_void, family: u32, index: u32) {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    // A mismatch is an ABI violation; never unwind through C.
    if family != shared.family || index != 0 {
        std::process::abort();
    }
    shared.queue_lock.0.lock();
}
unsafe extern "C" fn unlock_queue(opaque: *mut c_void, family: u32, index: u32) {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    if family != shared.family || index != 0 {
        std::process::abort();
    }
    unsafe { shared.queue_lock.0.unlock() };
}
unsafe extern "C" fn acquire(opaque: *mut c_void, out: *mut Target) -> c_int {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    match shared.acquire() {
        Ok(Some(frame)) => {
            unsafe { out.write(frame.target) };
            1
        }
        Ok(None) => 0,
        Err(_) => -1,
    }
}
unsafe extern "C" fn acquire_frame(opaque: *mut c_void, out: *mut Frame) -> c_int {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    match shared.acquire() {
        Ok(Some(frame)) => {
            unsafe { out.write(frame) };
            1
        }
        Ok(None) => 0,
        Err(_) => -1,
    }
}

unsafe extern "C" fn wakeup(opaque: *mut c_void) {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    shared.notify();
}

impl Shared {
    fn acquire(&self) -> Result<Option<Frame>, Error> {
        let Some(mut pool) = self.pool.try_lock() else {
            self.exhausted.store(true, Ordering::Release);
            // Capacity can become available just before this contention; no
            // periodic polling timer remains to rescue a lost retry.
            self.notify();
            return Ok(None);
        };
        let result = pool.acquire();
        if !matches!(&result, Ok(Some(_))) {
            self.exhausted.store(true, Ordering::Release);
        }
        let failed = result.is_err();
        drop(pool);
        if failed {
            self.notify();
        }
        result
    }

    fn release(&self, token: u64, status: c_int) {
        let mut pool = self.pool.lock();
        match pool.release(token, status) {
            Ok(true) => self.exhausted.store(true, Ordering::Release),
            Ok(false) => {}
            Err(error) => pool.fail(error.to_string()),
        }
        drop(pool);
        self.notify();
    }
}
// VO thread: copy the latest host-described target and return immediately.
unsafe extern "C" fn target_color(opaque: *mut c_void, out: *mut TargetColor) {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    unsafe { *out = shared.pool.lock().color };
}

unsafe extern "C" fn release(opaque: *mut c_void, target: *const Target, status: c_int) {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    shared.release(unsafe { (*target).token }, status);
}

unsafe extern "C" fn release_frame(opaque: *mut c_void, frame: *const Frame, status: c_int) {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    shared.release(unsafe { (*frame).target.token }, status);
}

// HAL attachments are one-shot queue state. This guard drops before the queue
// gate, removing unconsumed attachments on validation failure or unwind.
struct SubmissionSync<'a> {
    queue: &'a wgpu::Queue,
    semaphore: vk::Semaphore,
}
impl Drop for SubmissionSync<'_> {
    fn drop(&mut self) {
        if let Some(native) = unsafe { self.queue.as_hal::<wgpu::hal::api::Vulkan>() } {
            native.remove_wait_semaphore(self.semaphore);
            native.remove_signal_semaphore(self.semaphore);
        }
    }
}

struct CopyOwnership<'a> {
    shared: &'a Shared,
    submitted: bool,
}
impl Drop for CopyOwnership<'_> {
    fn drop(&mut self) {
        if !self.submitted {
            self.shared.fail("embedded MPV copy submission failed");
        }
    }
}

/// Idle libmpv instance with a bounded GPU target pool and private current frame.
/// Playback business/control is provided exclusively by the configured JSON IPC.
pub struct Host {
    handle: *mut c_void,
    terminate: unsafe extern "C" fn(*mut c_void),
    request_redraw: unsafe extern "C" fn(*mut c_void) -> c_int,
    wait_event: unsafe extern "C" fn(*mut c_void, f64) -> *mut MpvEvent,
    _library: libloading::Library,
    _descriptor: Box<Descriptor>,
    _extensions: Vec<*const c_char>,
    shared: Arc<Shared>,
    completion: Completion,
    current: wgpu::Texture,
    frame_generation: u64,
    current_color_epoch: Option<u64>,
    abi_version: u32,
}
// SAFETY: libmpv permits client calls on different threads; Host is not Sync,
// so mutation/teardown remain exclusive. Vulkan sharing uses the retained mutex.
unsafe impl Send for Host {}

impl Host {
    /// Changes only when the private destination is replaced (for example resize).
    pub fn frame_generation(&self) -> u64 {
        self.frame_generation
    }

    /// Creates a binding with texture at binding 0 and nearest sampler at binding 1.
    /// Layout must describe a filterable 2D float texture and filtering sampler.
    /// Refresh after frame_generation changes. No borrowed producer image escapes.
    pub fn bind_group(&self, layout: &wgpu::BindGroupLayout) -> wgpu::BindGroup {
        let device = &self.shared.context.device;
        let view = self
            .current
            .create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mpv code-preserving nearest sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mpv private current frame"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        })
    }

    /// Boots idle with the explicit baseline and IPC endpoint, without loadfile.
    /// `wake` may run on mpv's VO thread or a wgpu completion thread: it must
    /// promptly enqueue an application notification, never call mpv or wait for UI.
    /// Drop terminates mpv and drains outstanding copy completion callbacks.
    pub fn new(
        context: Arc<DeviceContext>,
        options: HostOptions,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        options.validate()?;
        std::fs::create_dir_all(&options.demuxer_cache_dir).map_err(|error| {
            Error::from(format!(
                "cannot create embedded MPV cache directory: {error}"
            ))
        })?;
        validate_size(&context.device, options.width, options.height)?;
        unsafe {
            let native = context
                .device
                .as_hal::<wgpu::hal::api::Vulkan>()
                .ok_or("Vulkan required")?;
            if native.queue_index() != 0 {
                return Err("mpv requires queue index zero".into());
            }
            let instance = native.shared_instance();
            let memory = instance
                .raw_instance()
                .get_physical_device_memory_properties(native.raw_physical_device());
            let shared = Arc::new(Shared {
                context: Arc::clone(&context),
                raw: native.raw_device().clone(),
                family: native.queue_family_index(),
                memory,
                queue_lock: context.queue_lock(),
                pool: Mutex::new(Pool {
                    slots: Vec::new(),
                    ready: VecDeque::new(),
                    stopping: false,
                    size: (options.width, options.height),
                    color: TargetColor::SDR,
                    color_epoch: 0,
                    error: None,
                }),
                exhausted: AtomicBool::new(false),
                asynchronous: options.host_abi == HostAbi::TimelineV3,
                error_reported: AtomicBool::new(false),
                wake: Box::new(wake),
            });
            for index in 0..3 {
                let slot = shared.allocate(options.width, options.height, index)?;
                shared.pool.lock().slots.push(slot);
                if shared.asynchronous {
                    let mut timeline = vk::SemaphoreTypeCreateInfo::default()
                        .semaphore_type(vk::SemaphoreType::TIMELINE)
                        .initial_value(0);
                    let semaphore = shared.raw.create_semaphore(
                        &vk::SemaphoreCreateInfo::default().push_next(&mut timeline),
                        None,
                    )?;
                    shared.pool.lock().slots[index as usize].semaphore = semaphore;
                }
            }
            let extensions: Vec<_> = native
                .enabled_device_extensions()
                .iter()
                .map(|e| e.as_ptr())
                .collect();
            let descriptor = Box::new(Descriptor {
                version: if shared.asynchronous { 3 } else { 2 },
                instance: instance.raw_instance().handle(),
                physical_device: native.raw_physical_device(),
                device: shared.raw.handle(),
                get_proc_addr: instance.entry().static_fn().get_instance_proc_addr,
                queue_family: shared.family,
                features: context.features(),
                extensions: extensions.as_ptr(),
                num_extensions: extensions.len() as i32,
                opaque: Arc::as_ptr(&shared).cast_mut().cast(),
                lock_queue,
                unlock_queue,
                acquire,
                release,
                target_color,
                acquire_frame: shared.asynchronous.then_some(acquire_frame),
                release_frame: shared.asynchronous.then_some(release_frame),
            });
            // Do not retain a wgpu internal guard across libmpv initialization
            // or fallible teardown paths that poll the same device.
            drop(native);
            // The default Windows search order never looks in the loaded DLL's
            // own directory. LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR resolves
            // libmpv-2.dll's dependencies beside it and avoids picking up a
            // same-named system library.
            #[cfg(windows)]
            let library =
                libloading::Library::from(libloading::os::windows::Library::load_with_flags(
                    &options.libmpv,
                    libloading::os::windows::LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR
                        | libloading::os::windows::LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
                )?);
            #[cfg(not(windows))]
            let library = libloading::Library::new(&options.libmpv)
                .map_err(|_| Error::from("cannot load the configured embedded libmpv"))?;
            let create = *library.get::<unsafe extern "C" fn() -> *mut c_void>(b"mpv_create\0")?;
            let terminate =
                *library.get::<unsafe extern "C" fn(*mut c_void)>(b"mpv_terminate_destroy\0")?;
            let request_redraw = *library.get::<unsafe extern "C" fn(*mut c_void) -> c_int>(
                b"mpv_gpu_next_request_redraw\0",
            )?;
            let set_host = *library
                .get::<unsafe extern "C" fn(*mut c_void, *const Descriptor) -> c_int>(
                    b"mpv_gpu_next_set_host\0",
                )?;
            let set_option = *library.get::<unsafe extern "C" fn(
                *mut c_void,
                *const c_char,
                *const c_char,
            ) -> c_int>(b"mpv_set_option_string\0")?;
            let initialize =
                *library.get::<unsafe extern "C" fn(*mut c_void) -> c_int>(b"mpv_initialize\0")?;
            let set_wakeup = *library.get::<unsafe extern "C" fn(
                *mut c_void,
                Option<unsafe extern "C" fn(*mut c_void)>,
                *mut c_void,
            )>(b"mpv_set_wakeup_callback\0")?;
            let wait_event = *library
                .get::<unsafe extern "C" fn(*mut c_void, f64) -> *mut MpvEvent>(
                    b"mpv_wait_event\0",
                )?;
            let completed = Arc::clone(&shared);
            let completion = Completion::new(context.device.clone(), move |token, result| {
                let mut pool = completed.pool.lock();
                match result {
                    Ok(()) => {
                        if let Err(error) = pool.complete(token) {
                            pool.fail(error.to_string());
                        }
                    }
                    Err(error) => pool.fail(error),
                }
                // Wake only for errors, exhausted capacity, or a deferred
                // resize. Successful copies alone must not cause a redraw loop.
                let needs_ui = pool.error.is_some()
                    || completed.exhausted.load(Ordering::Acquire)
                    || pool.slots.iter().any(|slot| {
                        slot.lease.is_idle()
                            && (slot.target.width as u32, slot.target.height as u32) != pool.size
                    });
                drop(pool);
                if needs_ui {
                    completed.notify();
                }
            })?;
            let current = {
                let _guard = shared.queue_lock.lock();
                Self::private_texture(
                    &context.device,
                    &context.queue,
                    options.width,
                    options.height,
                )
            };
            // All fallible setup preceding mpv_create is already owned by RAII.
            let handle = create();
            if handle.is_null() {
                return Err("mpv_create failed".into());
            }
            let mut host = Self {
                handle,
                terminate,
                request_redraw,
                wait_event,
                _library: library,
                _descriptor: descriptor,
                _extensions: extensions,
                shared,
                completion,
                current,
                frame_generation: 0,
                current_color_epoch: None,
                abi_version: 0,
            };
            set_wakeup(
                handle,
                Some(wakeup),
                Arc::as_ptr(&host.shared).cast_mut().cast(),
            );
            let option = |name: &str, value: &str| -> Result<(), Error> {
                let name_c = CString::new(name)?;
                let value_c = CString::new(value)?;
                let result = set_option(handle, name_c.as_ptr(), value_c.as_ptr());
                if result < 0 {
                    return Err(format!("embedded mpv option {name} failed: {result}").into());
                }
                Ok(())
            };
            option("config", "no")?;
            option(
                "include",
                options
                    .baseline
                    .to_str()
                    .ok_or("baseline path is not UTF-8")?,
            )?;
            for argument in &options.extra_args {
                let (name, value) = parse_argument(argument)?;
                option(name, value)?;
            }
            // These cannot be overridden by the baseline or extra options.
            for (name, value) in [
                ("config", "no"),
                ("vo", "gpu-next"),
                ("gpu-api", "vulkan"),
                ("idle", "yes"),
                ("terminal", "no"),
                ("load-scripts", "no"),
                ("osc", "no"),
                ("input-default-bindings", "no"),
            ] {
                option(name, value)?;
            }
            // config=no disables mpv's default cache path as well as user config.
            // Without an explicit directory, packets stay in RAM even with cache-on-disk.
            option(
                "demuxer-cache-dir",
                options
                    .demuxer_cache_dir
                    .to_str()
                    .ok_or("cache directory is not UTF-8")?,
            )?;
            option(
                "input-ipc-server",
                options.ipc.to_str().ok_or("IPC path is not UTF-8")?,
            )?;
            let mut result = set_host(handle, &*host._descriptor);
            if result < 0 && options.host_abi == HostAbi::Legacy {
                // Older pinned builds only accept version 1: retry with the v1
                // contract (fixed SDR target; target_color is never read).
                host._descriptor.version = 1;
                result = set_host(handle, &*host._descriptor);
            }
            if result < 0 {
                return Err(format!(
                    "mpv_gpu_next_set_host ABI {} failed: {result}",
                    host._descriptor.version
                )
                .into());
            }
            host.abi_version = host._descriptor.version;
            let result = initialize(handle);
            if result < 0 {
                return Err(format!("mpv_initialize failed: {result}").into());
            }
            Ok(host)
        }
    }
    pub fn queue_lock(&self) -> Arc<QueueLock> {
        self.shared.queue_lock.clone()
    }

    /// Application-thread only, outside the queue lock and every VO callback.
    /// Drains native wakeups and surfaces recorded GPU failures. Playback control
    /// and business events remain exclusively on JSON IPC. Safe without a window.
    fn maintain(&self) -> Result<(), Error> {
        for index in 0..256 {
            let event = unsafe { (self.wait_event)(self.handle, 0.0).as_ref() }
                .ok_or("embedded MPV returned a null event")?;
            match event.event_id {
                0 => break,
                1 => self.shared.fail("embedded MPV shut down"),
                // This handle owns no playback snapshot. Even overflow of these
                // unused events cannot invalidate the independent GPU callbacks.
                _ => {}
            }
            if index == 255 {
                self.shared.notify();
            }
        }
        {
            let pool = self.shared.pool.lock();
            if let Some(error) = &pool.error {
                return Err(error.clone().into());
            }
        }
        self.refresh_slots()
    }

    /// Drains native wakeups, recycles completed capacity and requests a retry
    /// only after an exhausted acquisition. Call outside every queue gate.
    pub fn retry_if_capacity(&self) -> Result<(), Error> {
        self.maintain()?;
        let retry = {
            let pool = self.shared.pool.lock();
            !pool.stopping
                && pool.slots.iter().any(|slot| {
                    slot.lease.is_idle()
                        && (slot.target.width as u32, slot.target.height as u32) == pool.size
                })
                && self.shared.exhausted.swap(false, Ordering::AcqRel)
        };
        if retry {
            self.request_redraw()?;
        }
        Ok(())
    }

    pub fn request_redraw(&self) -> Result<(), Error> {
        let status = unsafe { (self.request_redraw)(self.handle) };
        if status < 0 {
            Err(format!("mpv_gpu_next_request_redraw failed: {status}").into())
        } else {
            Ok(())
        }
    }

    /// Negotiated ABI: 1 is fixed SDR, 2 adds target color, and 3 adds timelines.
    pub fn abi_version(&self) -> u32 {
        self.abi_version
    }

    /// Changes the target color and invalidates all previously rendered frames.
    /// Call `request_redraw` afterward, including while paused. Until a frame
    /// for the new target is copied, `has_current_frame` is false.
    /// Application-thread only, outside the queue lock and VO callbacks.
    pub fn set_target_color(&mut self, color: TargetColor) {
        if self.shared.pool.lock().set_target_color(color) {
            self.current_color_epoch = None;
        }
    }

    /// Whether the private texture has a copied frame for the current target.
    /// Submissions that sample it must use the shared ordered queue.
    pub fn has_current_frame(&self) -> bool {
        let pool = self.shared.pool.lock();
        !pool.stopping && self.current_color_epoch == Some(pool.color_epoch)
    }

    pub fn resize(&self, width: u32, height: u32) -> Result<(), Error> {
        validate_size(&self.shared.context.device, width, height)?;
        {
            let mut pool = self.shared.pool.lock();
            if pool.size == (width, height) {
                return Ok(());
            }
            pool.size = (width, height);
        }
        self.refresh_slots()?;
        self.request_redraw()
    }

    fn refresh_slots(&self) -> Result<(), Error> {
        for index in 0..3 {
            let size = {
                let mut pool = self.shared.pool.lock();
                if pool.stopping {
                    return Ok(());
                }
                let size = pool.size;
                let slot = &mut pool.slots[index];
                if !slot.lease.is_idle()
                    || (slot.target.width as u32, slot.target.height as u32) == size
                {
                    continue;
                }
                // Reserve only GPU-idle slots. Allocation never holds the pool
                // lock, so VO callbacks cannot block behind a Vulkan allocation.
                if !slot.lease.reserve() {
                    return Err("invalid MPV resize reservation".into());
                }
                size
            };
            let replacement = match self.shared.allocate(size.0, size.1, index as u64) {
                Ok(slot) => slot,
                Err(error) => {
                    self.shared.pool.lock().slots[index]
                        .lease
                        .cancel_reservation();
                    self.shared
                        .fail(format!("embedded MPV image allocation failed: {error}"));
                    return Err(error);
                }
            };
            let old = {
                let mut pool = self.shared.pool.lock();
                let slot = &mut pool.slots[index];
                let mut replacement = replacement;
                replacement.semaphore = slot.semaphore;
                if !slot.lease.cancel_reservation() {
                    std::process::abort();
                }
                replacement.lease = slot.lease;
                std::mem::replace(slot, replacement)
            };
            unsafe {
                self.shared.raw.destroy_image(old.target.image, None);
                self.shared.raw.free_memory(old.memory, None);
            }
        }
        Ok(())
    }

    fn private_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("private 10-bit current frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgb10a2Unorm,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        // Tell wgpu this texture is initialized before writing via raw Vulkan.
        // Otherwise its lazy first-use clear can erase the native GPU copy.
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("initialize private video texture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        queue.submit([encoder.finish()]);
        texture
    }

    /// Copies one scheduler-released frame. None means no new frame; the bool
    /// indicates a replaced private texture, requiring bind-group refresh.
    /// Locks internally. A producer slot is returned only after GPU completion.
    /// Errors quarantine this host session; no synchronization failure downgrades
    /// the selected ABI. Stale-color frames are drained but never made current.
    pub fn copy_ready(&mut self) -> Result<Option<bool>, Error> {
        let result = self.copy_ready_inner();
        if let Err(error) = &result {
            self.shared.fail(error.to_string());
        }
        result
    }

    fn copy_ready_inner(&mut self) -> Result<Option<bool>, Error> {
        let (frame, color_epoch) = {
            let mut pool = self.shared.pool.lock();
            if let Some(error) = &pool.error {
                return Err(error.clone().into());
            }
            let Some(frame) = pool.begin_copy()? else {
                return Ok(None);
            };
            frame
        };
        let mut ownership = CopyOwnership {
            shared: &self.shared,
            submitted: false,
        };
        let target = frame.target;
        let queue = &self.shared.context.queue;
        let destination = &mut self.current;
        let resized = destination.width() != target.width as u32
            || destination.height() != target.height as u32;
        if resized {
            let replacement = {
                let _guard = self.shared.queue_lock.lock();
                Self::private_texture(
                    &self.shared.context.device,
                    queue,
                    target.width as u32,
                    target.height as u32,
                )
            };
            self.frame_generation = self
                .frame_generation
                .checked_add(1)
                .ok_or("embedded MPV texture generation exhausted")?;
            // Replaced GPU handles must be destroyed outside the queue gate.
            *destination = replacement;
        }
        let mut encoder =
            self.shared
                .context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("scheduled video copy"),
                });
        encoder.transition_resources(
            std::iter::empty(),
            std::iter::once(wgpu::TextureTransition {
                texture: &*destination,
                selector: None,
                state: wgpu::TextureUses::COPY_DST,
            }),
        );
        let before = encoder.finish();
        // wgpu forbids mixing its encoder API with raw HAL recording.
        // Keep tracked transitions in separate command buffers around the copy.
        let mut encoder =
            self.shared
                .context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("raw video image copy"),
                });
        unsafe {
            let dest = destination
                .as_hal::<wgpu::hal::api::Vulkan>()
                .ok_or("embedded MPV private texture is not Vulkan")?;
            let image = dest.raw_handle();
            encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|native| {
                let command = native
                    .ok_or("embedded MPV copy encoder is not Vulkan")?
                    .raw_handle();
                let range = vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1);
                let barrier = vk::ImageMemoryBarrier::default()
                    .image(target.image)
                    .subresource_range(range)
                    .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
                self.shared.raw.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                );
                let layers = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1);
                let copy = vk::ImageCopy::default()
                    .src_subresource(layers)
                    .dst_subresource(layers)
                    .extent(vk::Extent3D {
                        width: target.width as u32,
                        height: target.height as u32,
                        depth: 1,
                    });
                self.shared.raw.cmd_copy_image(
                    command,
                    target.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[copy],
                );
                Ok::<(), Error>(())
            })?;
        }
        let copy = encoder.finish();
        let mut encoder =
            self.shared
                .context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("sample copied video image"),
                });
        encoder.transition_resources(
            std::iter::empty(),
            std::iter::once(wgpu::TextureTransition {
                texture: &*destination,
                selector: None,
                state: wgpu::TextureUses::RESOURCE,
            }),
        );
        let after = encoder.finish();
        let submission = {
            let _guard = self.shared.queue_lock.lock();
            // Construct cleanup before attaching either one-shot operation.
            let _sync = if self.shared.asynchronous {
                let sync = SubmissionSync {
                    queue,
                    semaphore: frame.semaphore,
                };
                let native = unsafe { queue.as_hal::<wgpu::hal::api::Vulkan>() }
                    .ok_or("embedded MPV copy queue is not Vulkan")?;
                native.add_wait_semaphore(
                    frame.semaphore,
                    Some(frame.wait_value),
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                );
                native.add_signal_semaphore(frame.semaphore, Some(frame.signal_value));
                Some(sync)
            } else {
                None
            };
            // Attachment and the matching submission are a single transaction
            // on the exact QueueSynchronization gate used by iced and mpv.
            queue.submit([before, copy, after])
        };
        self.completion.track(submission, target.token as usize)?;
        ownership.submitted = true;
        // Never bless a stale-color drain as current for the new target.
        self.current_color_epoch = Some(color_epoch);
        let pending = !self.shared.pool.lock().ready.is_empty();
        if pending {
            self.shared.notify();
        }
        Ok(Some(resized))
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.shared.pool.lock().stopping = true;
        // Never hold queue/pool locks across mpv teardown: it joins the VO thread.
        unsafe { (self.terminate)(self.handle) };
        if let Err(error) = self.completion.shutdown() {
            self.shared.fail(error.to_string());
        }
        // Poll can invoke arbitrary completion callbacks; no queue lock is held.
        if let Err(error) = self
            .shared
            .context
            .device
            .poll(wgpu::PollType::wait_indefinitely())
        {
            self.shared
                .fail(format!("embedded MPV GPU teardown failed: {error}"));
        }
    }
}

fn validate_size(device: &wgpu::Device, width: u32, height: u32) -> Result<(), Error> {
    let limit = device
        .limits()
        .max_texture_dimension_2d
        .min(i32::MAX as u32);
    if width == 0 || height == 0 || width > limit || height > limit {
        return Err("embedded mpv target dimensions exceed device limits".into());
    }
    Ok(())
}

pub(crate) fn parse_argument(argument: &str) -> Result<(&str, &str), Error> {
    let (name, value) = argument
        .strip_prefix("--")
        .and_then(|arg| arg.split_once('='))
        .ok_or("embedded mpv extra arguments require --name=value")?;
    // An allowlist prevents aliases, list mutations, profiles, scripts and config
    // includes from bypassing the host's device/VO/IPC ownership contract.
    if !matches!(
        name,
        "volume"
            | "volume-max"
            | "mute"
            | "pause"
            | "speed"
            | "aid"
            | "sid"
            | "alang"
            | "slang"
            | "audio-device"
            | "audio-delay"
            | "audio-channels"
            | "audio-exclusive"
            | "sub-delay"
            | "sub-scale"
            | "sub-pos"
            | "sub-visibility"
            | "sub-font"
            | "sub-font-size"
            | "sub-ass-override"
            | "sub-auto"
            | "audio-file-auto"
            | "hwdec"
            | "cache"
            | "cache-secs"
            | "demuxer-max-bytes"
            | "demuxer-max-back-bytes"
            | "network-timeout"
    ) || value.contains('\0')
    {
        return Err("embedded mpv argument is not an allowed playback option".into());
    }
    Ok((name, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> Pool {
        Pool {
            slots: (0..3)
                .map(|token| Slot {
                    target: Target {
                        image: vk::Image::null(),
                        width: 1,
                        height: 1,
                        layout: vk::ImageLayout::UNDEFINED,
                        usage: vk::ImageUsageFlags::empty(),
                        token,
                    },
                    memory: vk::DeviceMemory::null(),
                    lease: Lease::default(),
                    semaphore: vk::Semaphore::null(),
                    color_epoch: 0,
                })
                .collect(),
            ready: VecDeque::new(),
            stopping: false,
            size: (1, 1),
            color: TargetColor::SDR,
            color_epoch: 0,
            error: None,
        }
    }

    #[test]
    fn pool_recycles_only_completed_copies_on_independent_timelines() {
        let mut pool = pool();
        let first = pool.acquire().unwrap().unwrap();
        let second = pool.acquire().unwrap().unwrap();
        let third = pool.acquire().unwrap().unwrap();
        assert!(pool.acquire().unwrap().is_none());
        pool.release(second.target.token, 0).unwrap();
        assert!(pool.acquire().unwrap().is_none());
        let (copy, _) = pool.begin_copy().unwrap().unwrap();
        assert_eq!(copy.target.token, second.target.token);
        assert_eq!((copy.wait_value, copy.signal_value), (1, 2));
        assert!(pool.acquire().unwrap().is_none());
        pool.complete(copy.target.token as usize).unwrap();
        let next = pool.acquire().unwrap().unwrap();
        assert_eq!(next.target.token, second.target.token);
        assert_eq!((next.wait_value, next.signal_value), (2, 3));
        assert_eq!((first.wait_value, third.wait_value), (0, 0));
        assert!(pool.release(second.target.token, 0).is_ok());
        assert!(pool.release(second.target.token, 0).is_err());
    }

    #[test]
    fn color_transition_drains_stale_ready_and_in_flight_frames_before_recycling() {
        let mut pool = pool();
        let ready = pool.acquire().unwrap().unwrap();
        let rendering = pool.acquire().unwrap().unwrap();
        pool.release(ready.target.token, 0).unwrap();
        pool.set_target_color(TargetColor::HDR10);
        let replacement = pool.acquire().unwrap().unwrap();
        assert!(pool.release(rendering.target.token, 0).unwrap());
        pool.release(replacement.target.token, 0).unwrap();
        assert!(pool.acquire().unwrap().is_none());
        for expected in [ready, rendering, replacement] {
            let (copy, epoch) = pool.begin_copy().unwrap().unwrap();
            assert_eq!(copy.target.token, expected.target.token);
            assert_eq!(copy.signal_value, 2);
            assert_eq!(
                epoch == pool.color_epoch,
                expected.target.token == replacement.target.token
            );
            assert!(!pool.slots[copy.target.token as usize].lease.is_idle());
            pool.complete(copy.target.token as usize).unwrap();
        }
        assert!(pool.slots.iter().all(|slot| slot.lease.is_idle()));
    }

    #[test]
    fn returning_to_the_same_color_does_not_revive_an_old_frame() {
        let mut pool = pool();
        pool.set_target_color(TargetColor::HDR10);
        let rendering = pool.acquire().unwrap().unwrap();
        pool.set_target_color(TargetColor::SDR);
        pool.set_target_color(TargetColor::HDR10);
        assert!(pool.release(rendering.target.token, 0).unwrap());
        let (copy, old_epoch) = pool.begin_copy().unwrap().unwrap();
        assert_ne!(old_epoch, pool.color_epoch);
        pool.complete(copy.target.token as usize).unwrap();
        let replacement = pool.acquire().unwrap().unwrap();
        pool.release(replacement.target.token, 0).unwrap();
        let (_, new_epoch) = pool.begin_copy().unwrap().unwrap();
        assert_eq!(new_epoch, pool.color_epoch);
    }

    #[test]
    fn failed_producer_release_never_produces_a_host_wait_or_fake_completion() {
        let mut pool = pool();
        let frame = pool.acquire().unwrap().unwrap();
        pool.release(frame.target.token, -1).unwrap();
        assert!(pool.begin_copy().unwrap().is_none());
        assert!(pool.complete(frame.target.token as usize).is_err());
        assert!(pool.release(frame.target.token, 0).is_err());
        assert!(pool.acquire().unwrap().is_none());
        assert!(pool.error.is_some());
        assert!(!pool.slots[frame.target.token as usize].lease.is_idle());
    }

    #[test]
    fn paused_resize_does_not_acquire_old_images_or_replace_busy_slots() {
        let mut pool = pool();
        let frame = pool.acquire().unwrap().unwrap();
        pool.size = (2, 2);
        assert!(pool.acquire().unwrap().is_none());
        assert!(!pool.slots[frame.target.token as usize].lease.reserve());
        pool.release(frame.target.token, 0).unwrap();
        assert!(!pool.slots[frame.target.token as usize].lease.reserve());
        let (copy, _) = pool.begin_copy().unwrap().unwrap();
        assert!(!pool.slots[frame.target.token as usize].lease.reserve());
        pool.complete(copy.target.token as usize).unwrap();
        assert!(pool.slots[frame.target.token as usize].lease.reserve());
        assert!(pool.slots[frame.target.token as usize]
            .lease
            .cancel_reservation());
        // The image replacement preserves this lease and its per-slot timeline.
        pool.slots[frame.target.token as usize].target.width = 2;
        pool.slots[frame.target.token as usize].target.height = 2;
        let next = pool.acquire().unwrap().unwrap();
        assert_eq!(next.target.token, frame.target.token);
        assert_eq!((next.wait_value, next.signal_value), (2, 3));
    }
}
