use crate::{gpu::DeviceContext, Error, HostOptions};
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

// Exact repr(C) translation of mpv/include/mpv/gpu_next.h version 1.
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
    /// mpv's fixed BT.709 gamma-2.2 SDR target (every field unknown).
    pub const SDR: Self = Self {
        primaries: 0,
        transfer: 0,
        ref_luma: 0.0,
        min_luma: 0.0,
        max_luma: 0.0,
        depth: 0,
    };
    /// HDR10: BT.2020 primaries, PQ transfer, 203 nit reference white, 1000 nit
    /// peak, 10-bit. Static metadata is not signaled in ABI v2. `ref_luma` is
    /// honored only with libplacebo API >= 371; older builds keep the fixed
    /// 203 nit default, which equals this value.
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
}

// v2 appends exactly one pointer to the v1 layout; mpv never reads it for v1.
const _: () = assert!(
    std::mem::offset_of!(Descriptor, target_color)
        == std::mem::size_of::<Descriptor>() - std::mem::size_of::<usize>()
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
    busy: bool,
    color_epoch: u64,
}
struct Pool {
    slots: Vec<Slot>,
    ready: VecDeque<usize>,
    stopping: bool,
    size: (u32, u32),
    color: TargetColor,
    color_epoch: u64,
}

impl Pool {
    fn acquire(&mut self) -> Option<Target> {
        if self.stopping {
            return None;
        }
        let slot = self.slots.iter_mut().find(|slot| {
            !slot.busy && (slot.target.width as u32, slot.target.height as u32) == self.size
        })?;
        slot.busy = true;
        slot.color_epoch = self.color_epoch;
        Some(slot.target)
    }

    /// Returns whether a target change discarded a completed render that needs retrying.
    fn release(&mut self, target: Target, status: c_int) -> bool {
        let index = target.token as usize;
        let slot = &mut self.slots[index];
        if status != 0 {
            slot.target.layout = vk::ImageLayout::UNDEFINED;
            slot.busy = false;
            return false;
        }
        slot.target.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        if slot.color_epoch != self.color_epoch {
            // A target change raced this render. Its transfer function is
            // indeterminate; mpv has now completed it, so it is safe to recycle.
            slot.busy = false;
            return true;
        }
        self.ready.push_back(index);
        false
    }

    fn set_target_color(&mut self, color: TargetColor) -> bool {
        if self.color == color {
            return false;
        }
        self.color = color;
        self.color_epoch = self.color_epoch.wrapping_add(1);
        while let Some(index) = self.ready.pop_front() {
            self.slots[index].busy = false;
        }
        true
    }
}
struct Shared {
    // Also retained by completion callbacks, beyond Host teardown if necessary.
    context: Arc<DeviceContext>,
    raw: ash::Device,
    memory: vk::PhysicalDeviceMemoryProperties,
    family: u32,
    queue_lock: Arc<QueueLock>,
    pool: Mutex<Pool>,
    exhausted: AtomicBool,
    wake: Box<dyn Fn() + Send + Sync>,
}
impl Shared {
    fn notify(&self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.wake)()));
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
                busy: false,
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
    let Some(mut pool) = shared.pool.try_lock() else {
        shared.exhausted.store(true, Ordering::Release);
        return 0;
    };
    let Some(target) = pool.acquire() else {
        shared.exhausted.store(true, Ordering::Release);
        return 0;
    };
    unsafe { out.write(target) };
    1
}
// VO thread: copy the latest host-described target and return immediately.
unsafe extern "C" fn target_color(opaque: *mut c_void, out: *mut TargetColor) {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    unsafe { *out = shared.pool.lock().color };
}

unsafe extern "C" fn release(opaque: *mut c_void, target: *const Target, status: c_int) {
    let shared = unsafe { &*opaque.cast::<Shared>() };
    let target = unsafe { *target };
    if shared.pool.lock().release(target, status) {
        shared.exhausted.store(true, Ordering::Release);
    }
    shared.notify();
}

/// Idle libmpv instance with a bounded GPU target pool and private current frame.
/// Playback business/control is provided exclusively by the configured JSON IPC.
pub struct Host {
    handle: *mut c_void,
    terminate: unsafe extern "C" fn(*mut c_void),
    request_redraw: unsafe extern "C" fn(*mut c_void) -> c_int,
    _library: libloading::Library,
    _descriptor: Box<Descriptor>,
    _extensions: Vec<*const c_char>,
    shared: Arc<Shared>,
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
                }),
                exhausted: AtomicBool::new(false),
                wake: Box::new(wake),
            });
            for index in 0..3 {
                let slot = shared.allocate(options.width, options.height, index)?;
                shared.pool.lock().slots.push(slot);
            }
            let extensions: Vec<_> = native
                .enabled_device_extensions()
                .iter()
                .map(|e| e.as_ptr())
                .collect();
            let descriptor = Box::new(Descriptor {
                version: 2,
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
            let handle = create();
            if handle.is_null() {
                return Err("mpv_create failed".into());
            }
            let current = {
                let _guard = shared.queue_lock.lock();
                Self::private_texture(
                    &context.device,
                    &context.queue,
                    options.width,
                    options.height,
                )
            };
            let mut host = Self {
                handle,
                terminate,
                request_redraw,
                _library: library,
                _descriptor: descriptor,
                _extensions: extensions,
                shared,
                current,
                frame_generation: 0,
                current_color_epoch: None,
                abi_version: 0,
            };
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
            if result < 0 {
                // Older pinned builds only accept version 1: retry with the v1
                // contract (fixed SDR target; target_color is never read).
                host._descriptor.version = 1;
                result = set_host(handle, &*host._descriptor);
                if result < 0 {
                    return Err(format!("mpv_gpu_next_set_host failed: {result}").into());
                }
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
    pub fn retry_if_capacity(&self) -> Result<(), Error> {
        self.refresh_slots()?;
        let retry = {
            let pool = self.shared.pool.lock();
            !pool.stopping
                && pool.slots.iter().any(|slot| {
                    !slot.busy && (slot.target.width as u32, slot.target.height as u32) == pool.size
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

    /// Negotiated host ABI version: 2 enables `set_target_color`; 1 is the
    /// legacy fixed-SDR contract of older pinned builds.
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
        self.current_color_epoch == Some(self.shared.pool.lock().color_epoch)
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
                let size = pool.size;
                let slot = &mut pool.slots[index];
                if slot.busy || (slot.target.width as u32, slot.target.height as u32) == size {
                    continue;
                }
                // Reserve only GPU-idle slots. Allocation never holds the pool
                // lock, so VO callbacks cannot block behind a Vulkan allocation.
                slot.busy = true;
                size
            };
            let replacement = match self.shared.allocate(size.0, size.1, index as u64) {
                Ok(slot) => slot,
                Err(error) => {
                    self.shared.pool.lock().slots[index].busy = false;
                    return Err(error);
                }
            };
            let old = std::mem::replace(&mut self.shared.pool.lock().slots[index], replacement);
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
    pub fn copy_ready(&mut self) -> Option<bool> {
        let _guard = self.shared.queue_lock.lock();
        let queue = &self.shared.context.queue;
        let destination = &mut self.current;
        let (target, color_epoch) = {
            let mut pool = self.shared.pool.lock();
            let index = pool.ready.pop_front()?;
            (pool.slots[index].target, pool.color_epoch)
        };
        let resized = destination.width() != target.width as u32
            || destination.height() != target.height as u32;
        if resized {
            self.frame_generation = self.frame_generation.wrapping_add(1);
            *destination = Self::private_texture(
                &self.shared.context.device,
                queue,
                target.width as u32,
                target.height as u32,
            );
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
                .expect("Vulkan texture");
            let image = dest.raw_handle();
            encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|native| {
                let command = native.expect("Vulkan encoder").raw_handle();
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
            });
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
        queue.submit([before, copy, encoder.finish()]);
        // Later sampling submits to the same queue, after these copy commands.
        self.current_color_epoch = Some(color_epoch);
        let shared = self.shared.clone();
        queue.on_submitted_work_done(move || {
            {
                let mut pool = shared.pool.lock();
                let slot = &mut pool.slots[target.token as usize];
                slot.target.layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
                slot.busy = false;
            }
            shared.notify();
        });
        Some(resized)
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.shared.pool.lock().stopping = true;
        // Never hold queue/pool locks across mpv teardown: it joins the VO thread.
        unsafe { (self.terminate)(self.handle) };
        // Poll can invoke arbitrary completion callbacks; no queue lock is held.
        let _ = self
            .shared
            .context
            .device
            .poll(wgpu::PollType::wait_indefinitely());
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
                    busy: false,
                    color_epoch: 0,
                })
                .collect(),
            ready: VecDeque::new(),
            stopping: false,
            size: (1, 1),
            color: TargetColor::SDR,
            color_epoch: 0,
        }
    }

    #[test]
    fn color_transition_discards_ready_and_in_flight_old_frames() {
        let mut pool = pool();
        let ready = pool.acquire().unwrap();
        let rendering = pool.acquire().unwrap();
        pool.release(ready, 0);
        pool.set_target_color(TargetColor::HDR10);
        let replacement = pool.acquire().unwrap();
        assert!(
            pool.release(rendering, 0),
            "stale completion requests a redraw"
        );
        pool.release(replacement, 0);
        assert_eq!(
            pool.ready.into_iter().collect::<Vec<_>>(),
            [replacement.token as usize]
        );
    }

    #[test]
    fn returning_to_the_same_color_does_not_revive_an_old_frame() {
        let mut pool = pool();
        pool.set_target_color(TargetColor::HDR10);
        let rendering = pool.acquire().unwrap();
        pool.set_target_color(TargetColor::SDR);
        pool.set_target_color(TargetColor::HDR10);
        assert!(pool.release(rendering, 0));
        assert!(pool.ready.is_empty());
        let replacement = pool.acquire().unwrap();
        pool.release(replacement, 0);
        assert_eq!(pool.ready.pop_front(), Some(replacement.token as usize));
    }
}
