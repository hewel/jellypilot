use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use gstreamer as gst;
use gstreamer_video::{self as gst_video, prelude::VideoFrameExt};
use iced::widget::shader::{self, Viewport};
use iced::{mouse, wgpu, Rectangle};

use crate::dovi_color::DoviUniform;
use crate::playback::{Backend, Command, FrameSlot, FrameToken, PlaybackError};

type UploadKey = (FrameToken, u64);

#[derive(Debug)]
pub struct VideoSurface {
  frames: Arc<FrameSlot>,
  gpu: Mutex<SurfaceState>,
  failed: Mutex<Option<(FrameToken, Option<u64>)>>,
}

#[derive(Debug, Default)]
struct SurfaceState {
  texture: Option<Picture>,
  uploaded: Option<UploadKey>,
  scratch: Vec<u8>,
  aspect: f64,
  cleared: bool,
}

#[derive(Debug)]
struct Picture {
  texture: wgpu::Texture,
  bind_group: wgpu::BindGroup,
  width: u32,
  height: u32,
  format: wgpu::TextureFormat,
  dovi: Option<DoviPicture>,
}

#[derive(Debug)]
struct DoviPicture {
  chroma: wgpu::Texture,
  uniform: wgpu::Buffer,
  staging: wgpu::Buffer,
  row_pitch: u32,
  chroma_offset: u64,
}

static UPLOAD_POOL: std::sync::LazyLock<Result<rayon::ThreadPool, rayon::ThreadPoolBuildError>> =
  std::sync::LazyLock::new(|| {
    let threads = std::thread::available_parallelism().map_or(1, |count| count.get().min(4));
    rayon::ThreadPoolBuilder::new()
      .num_threads(threads)
      .thread_name(|index| format!("video-upload-{index}"))
      .build()
  });

impl DoviPicture {
  fn upload(
    &self,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    luma: &wgpu::Texture,
    frame: &MappedFrame,
  ) -> Result<(), PlaybackError> {
    use rayon::prelude::*;
    let pool = UPLOAD_POOL
      .as_ref()
      .map_err(|error| frame_error(error.to_string()))?;
    let planes = frame.layout.planes;
    let sources = [
      frame
        .mapped
        .plane_data(0)
        .map_err(|error| frame_error(error.to_string()))?,
      frame
        .mapped
        .plane_data(1)
        .map_err(|error| frame_error(error.to_string()))?,
    ];
    if sources
      .iter()
      .zip(planes)
      .any(|(bytes, plane)| bytes.len() < plane.plane_bytes)
    {
      return Err(frame_error(
        "P010 mapped plane is shorter than its stride layout",
      ));
    }
    let total_rows = planes[0].height + planes[1].height;
    let rows_per_job = total_rows.div_ceil(pool.current_num_threads() as u32);
    let mut writes: [Option<wgpu::QueueWriteBufferView>; 4] = std::array::from_fn(|_| None);
    for (index, write) in writes
      .iter_mut()
      .enumerate()
      .take(pool.current_num_threads())
    {
      let start = index as u32 * rows_per_job;
      if start >= total_rows {
        break;
      }
      let rows = rows_per_job.min(total_rows - start);
      let offset = u64::from(start) * u64::from(self.row_pitch);
      let size = wgpu::BufferSize::new(u64::from(rows) * u64::from(self.row_pitch))
        .ok_or_else(|| frame_error("empty P010 staging region"))?;
      *write = Some(
        queue
          .write_buffer_with(&self.staging, offset, size)
          .ok_or_else(|| frame_error("cannot allocate P010 staging region"))?,
      );
    }
    // SystemMemory caps may still contain slowly CPU-readable VA memory. Copy directly
    // into wgpu staging on bounded reusable workers, not through another CPU image.
    pool.install(|| {
      writes
        .into_par_iter()
        .enumerate()
        .for_each(|(index, write)| {
          let Some(mut write) = write else {
            return;
          };
          let start = index as u32 * rows_per_job;
          let rows = rows_per_job.min(total_rows - start);
          for local_row in 0..rows {
            let global_row = start + local_row;
            let (plane, row) = if global_row < planes[0].height {
              (0, global_row)
            } else {
              (1, global_row - planes[0].height)
            };
            let source = row as usize * planes[plane].stride as usize;
            let destination = local_row as usize * self.row_pitch as usize;
            let length = planes[plane].row_bytes as usize;
            write
              .slice(destination..destination + length)
              .copy_from_slice(&sources[plane][source..source + length]);
          }
        })
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
      label: Some("P010 plane transfer"),
    });
    for (index, texture) in [luma, &self.chroma].into_iter().enumerate() {
      encoder.copy_buffer_to_texture(
        wgpu::TexelCopyBufferInfo {
          buffer: &self.staging,
          layout: wgpu::TexelCopyBufferLayout {
            offset: if index == 0 { 0 } else { self.chroma_offset },
            bytes_per_row: Some(self.row_pitch),
            rows_per_image: Some(planes[index].height),
          },
        },
        wgpu::TexelCopyTextureInfo {
          texture,
          mip_level: 0,
          origin: wgpu::Origin3d::ZERO,
          aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
          width: planes[index].width,
          height: planes[index].height,
          depth_or_array_layers: 1,
        },
      );
    }
    // Dropped write views are flushed before these buffer-to-texture commands.
    queue.submit([encoder.finish()]);
    Ok(())
  }
}

impl VideoSurface {
  pub fn new(frames: Arc<FrameSlot>) -> Self {
    Self {
      frames,
      gpu: Mutex::default(),
      failed: Mutex::default(),
    }
  }

  pub fn uploaded(&self, token: FrameToken) -> Result<Option<u64>, PlaybackError> {
    Ok(
      self
        .lock()?
        .uploaded
        .filter(|(uploaded, _)| *uploaded == token)
        .map(|(_, sequence)| sequence),
    )
  }

  /// Retires the surface, including any primitives still in flight.
  pub fn clear(&self) -> Result<(), PlaybackError> {
    *self.lock()? = SurfaceState {
      cleared: true,
      ..SurfaceState::default()
    };
    Ok(())
  }

  fn lock(&self) -> Result<MutexGuard<'_, SurfaceState>, PlaybackError> {
    self
      .gpu
      .lock()
      .map_err(|error| PlaybackError::Frame(error.to_string()))
  }

  fn report(
    &self,
    backend: &Backend,
    token: FrameToken,
    sequence: Option<u64>,
    error: PlaybackError,
  ) {
    // Only bookkeeping is recovered after poisoning; frame/GPU locks remain fallible.
    let mut failed = match self.failed.lock() {
      Ok(failed) => failed,
      Err(poisoned) => poisoned.into_inner(),
    };
    if *failed == Some((token, sequence)) {
      return;
    }
    *failed = Some((token, sequence));
    drop(failed);
    if let Err(send_error) = backend.send(Command::RenderFailed {
      generation: token.generation,
      seek_generation: token.seek_generation,
      error,
    }) {
      eprintln!("{send_error}");
    }
  }
}

#[derive(Debug, Clone)]
pub struct Video {
  pub surface: Arc<VideoSurface>,
  pub token: FrameToken,
  pub backend: Backend,
}

impl<Message> shader::Program<Message> for Video {
  type State = ();
  type Primitive = VideoPrimitive;

  fn draw(&self, _state: &(), _cursor: mouse::Cursor, _bounds: Rectangle) -> VideoPrimitive {
    VideoPrimitive {
      video: self.clone(),
      geometry: Mutex::new(None),
    }
  }
}

#[derive(Debug)]
pub struct VideoPrimitive {
  video: Video,
  geometry: Mutex<Option<Geometry>>,
}

#[derive(Debug, Clone, Copy)]
struct Geometry {
  picture: Rectangle,
  surface: Rectangle,
  target: Rectangle<u32>,
  key: UploadKey,
}

impl VideoPrimitive {
  fn prepare_frame(
    &self,
    pipeline: &VideoPipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bounds: &Rectangle,
    viewport: &Viewport,
  ) -> Result<Option<Geometry>, PlaybackError> {
    let Some(frame) = self.video.surface.frames.latest(self.video.token)? else {
      return Ok(None);
    };
    let key = (frame.token, frame.sequence);
    let mut state = self.video.surface.lock()?;
    if state.cleared {
      return Ok(None);
    }
    if state.uploaded != Some(key) {
      let metadata = crate::dovi::frame(&frame.sample)?;
      let format = if metadata.is_some() {
        FrameFormat::P010
      } else {
        FrameFormat::Rgba
      };
      let mapped = MappedFrame::map(
        &frame.sample,
        format,
        device.limits().max_texture_dimension_2d,
      )?;
      let layout = mapped.layout;
      let color = metadata
        .as_ref()
        .map(|frame| {
          DoviUniform::new(
            frame,
            pipeline.target_format.is_srgb(),
            layout.chroma_offset,
          )
        })
        .transpose()?;
      let texture_format = if color.is_some() {
        wgpu::TextureFormat::R16Uint
      } else {
        pipeline.texture_format
      };
      let needs_texture = state.texture.as_ref().is_none_or(|picture| {
        picture.width != layout.width
          || picture.height != layout.height
          || picture.format != texture_format
      });
      if needs_texture {
        state.texture = Some(if color.is_some() {
          if DoviUniform::BYTE_SIZE > device.limits().max_uniform_buffer_binding_size {
            return Err(frame_error(
              "Dolby Vision metadata exceeds GPU uniform limits",
            ));
          }
          pipeline
            .dovi
            .get_or_init(|| DoviPipeline::new(device, pipeline.target_format))
            .picture(device, &layout)?
        } else {
          pipeline.picture(device, layout.width, layout.height)
        });
      }
      let SurfaceState {
        texture, scratch, ..
      } = &mut *state;
      let picture = texture
        .as_ref()
        .ok_or_else(|| frame_error("texture was not allocated"))?;
      if let Some(color) = color {
        let dovi = picture
          .dovi
          .as_ref()
          .ok_or_else(|| frame_error("Dolby Vision texture was not allocated"))?;
        dovi.upload(device, queue, &picture.texture, &mapped)?;
        queue.write_buffer(&dovi.uniform, 0, color.as_bytes());
      } else {
        let (bytes, stride) = mapped.upload_bytes(0, scratch)?;
        upload_plane(queue, &picture.texture, bytes, stride, layout.planes[0]);
      }
      state.aspect = layout.aspect;
      state.uploaded = Some(key);
      self.video.backend.notify();
    }
    let scale = viewport.scale_factor();
    let surface = Rectangle {
      x: bounds.x * scale,
      y: bounds.y * scale,
      width: bounds.width * scale,
      height: bounds.height * scale,
    };
    let Some(picture) = contain(surface, state.aspect) else {
      return Ok(None);
    };
    let limit = device.limits().max_texture_dimension_2d as f32;
    if picture.width > limit
      || picture.height > limit
      || picture.x < -2.0 * limit
      || picture.y < -2.0 * limit
      || picture.x + picture.width > 2.0 * limit - 1.0
      || picture.y + picture.height > 2.0 * limit - 1.0
    {
      return Err(frame_error("video viewport exceeds GPU limits"));
    }
    Ok(Some(Geometry {
      picture,
      surface,
      target: Rectangle {
        x: 0,
        y: 0,
        width: viewport.physical_width(),
        height: viewport.physical_height(),
      },
      key,
    }))
  }

  fn render_frame(
    &self,
    pipeline: &VideoPipeline,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    clip_bounds: &Rectangle<u32>,
  ) -> Result<(), PlaybackError> {
    let geometry = *self
      .geometry
      .lock()
      .map_err(|error| frame_error(error.to_string()))?;
    let Some(geometry) = geometry else {
      return Ok(());
    };
    // A seek/replacement can invalidate the retained sample between prepare and render.
    if geometry.key.0 != self.video.token
      || self
        .video
        .surface
        .frames
        .latest(self.video.token)?
        .is_none()
    {
      return Ok(());
    }
    let state = self.video.surface.lock()?;
    if state.cleared || state.uploaded != Some(geometry.key) {
      return Ok(());
    }
    let Some(picture) = state.texture.as_ref() else {
      return Ok(());
    };
    let Some(clip) = scissor(geometry.surface, *clip_bounds, geometry.target) else {
      return Ok(());
    };
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("local video"),
      color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: target,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
          load: wgpu::LoadOp::Load,
          store: wgpu::StoreOp::Store,
        },
      })],
      depth_stencil_attachment: None,
      timestamp_writes: None,
      occlusion_query_set: None,
      multiview_mask: None,
    });
    let render_pipeline = if picture.dovi.is_some() {
      &pipeline
        .dovi
        .get()
        .ok_or_else(|| frame_error("Dolby Vision pipeline was not initialized"))?
        .pipeline
    } else {
      &pipeline.pipeline
    };
    pass.set_pipeline(render_pipeline);
    pass.set_bind_group(0, &picture.bind_group, &[]);
    pass.set_viewport(
      geometry.picture.x,
      geometry.picture.y,
      geometry.picture.width,
      geometry.picture.height,
      0.0,
      1.0,
    );
    pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
    pass.draw(0..3, 0..1);
    Ok(())
  }

  fn report(&self, error: PlaybackError) {
    let sequence = match self.video.surface.frames.latest(self.video.token) {
      Ok(Some(frame)) => Some(frame.sequence),
      _ => None,
    };
    self
      .video
      .surface
      .report(&self.video.backend, self.video.token, sequence, error);
  }
}

impl shader::Primitive for VideoPrimitive {
  type Pipeline = VideoPipeline;

  fn prepare(
    &self,
    pipeline: &mut VideoPipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bounds: &Rectangle,
    viewport: &Viewport,
  ) {
    let result = self.prepare_frame(pipeline, device, queue, bounds, viewport);
    match self.geometry.lock() {
      Ok(mut geometry) => match result {
        Ok(prepared) => *geometry = prepared,
        Err(error) => {
          *geometry = None;
          self.report(error);
        }
      },
      Err(error) => self.report(frame_error(error.to_string())),
    }
  }

  fn render(
    &self,
    pipeline: &VideoPipeline,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    clip_bounds: &Rectangle<u32>,
  ) {
    if let Err(error) = self.render_frame(pipeline, encoder, target, clip_bounds) {
      self.report(error);
    }
  }
}

pub struct VideoPipeline {
  pipeline: wgpu::RenderPipeline,
  layout: wgpu::BindGroupLayout,
  sampler: wgpu::Sampler,
  texture_format: wgpu::TextureFormat,
  target_format: wgpu::TextureFormat,
  dovi: OnceLock<DoviPipeline>,
}

impl shader::Pipeline for VideoPipeline {
  fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("local video texture layout"),
      entries: &[
        wgpu::BindGroupLayoutEntry {
          binding: 0,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
          },
          count: None,
        },
        wgpu::BindGroupLayoutEntry {
          binding: 1,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
          count: None,
        },
      ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      label: Some("local video sampler"),
      mag_filter: wgpu::FilterMode::Linear,
      min_filter: wgpu::FilterMode::Linear,
      ..Default::default()
    });
    let pipeline = render_pipeline(device, format, &layout, include_str!("video.wgsl"));
    Self {
      target_format: format,
      dovi: OnceLock::new(),
      pipeline,
      layout,
      sampler,
      texture_format: if format.is_srgb() {
        wgpu::TextureFormat::Rgba8UnormSrgb
      } else {
        wgpu::TextureFormat::Rgba8Unorm
      },
    }
  }
}

impl VideoPipeline {
  fn picture(&self, device: &wgpu::Device, width: u32, height: u32) -> Picture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some("local video RGBA"),
      size: wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: self.texture_format,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("local video texture"),
      layout: &self.layout,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(&view),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::Sampler(&self.sampler),
        },
      ],
    });
    Picture {
      texture,
      bind_group,
      width,
      height,
      format: self.texture_format,
      dovi: None,
    }
  }
}

struct DoviPipeline {
  pipeline: wgpu::RenderPipeline,
  layout: wgpu::BindGroupLayout,
}

impl DoviPipeline {
  fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
    let plane_binding = |binding| wgpu::BindGroupLayoutEntry {
      binding,
      visibility: wgpu::ShaderStages::FRAGMENT,
      ty: wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Uint,
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
      },
      count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Dolby Vision planes and metadata"),
      entries: &[
        plane_binding(0),
        plane_binding(1),
        wgpu::BindGroupLayoutEntry {
          binding: 2,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(DoviUniform::BYTE_SIZE),
          },
          count: None,
        },
      ],
    });
    let pipeline = render_pipeline(device, target_format, &layout, include_str!("dovi.wgsl"));
    Self { pipeline, layout }
  }

  fn picture(&self, device: &wgpu::Device, frame: &FrameLayout) -> Result<Picture, PlaybackError> {
    let row_pitch = frame.planes[1]
      .row_bytes
      .checked_next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
      .ok_or_else(|| frame_error("P010 staging row overflow"))?;
    let chroma_offset = u64::from(row_pitch) * u64::from(frame.planes[0].height);
    let size = chroma_offset + u64::from(row_pitch) * u64::from(frame.planes[1].height);
    if size > device.limits().max_buffer_size {
      return Err(frame_error("P010 staging exceeds GPU buffer limits"));
    }
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("P010 texture transfer"),
      size,
      usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
      mapped_at_creation: false,
    });
    let plane_texture = |plane: PlaneLayout, format| {
      device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Dolby Vision P010 plane"),
        size: wgpu::Extent3d {
          width: plane.width,
          height: plane.height,
          depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
      })
    };
    let texture = plane_texture(frame.planes[0], wgpu::TextureFormat::R16Uint);
    let chroma = plane_texture(frame.planes[1], wgpu::TextureFormat::Rg16Uint);
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("Dolby Vision frame metadata"),
      size: DoviUniform::BYTE_SIZE,
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
      mapped_at_creation: false,
    });
    let luma_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let chroma_view = chroma.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Dolby Vision frame"),
      layout: &self.layout,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(&luma_view),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::TextureView(&chroma_view),
        },
        wgpu::BindGroupEntry {
          binding: 2,
          resource: uniform.as_entire_binding(),
        },
      ],
    });
    Ok(Picture {
      texture,
      bind_group,
      width: frame.width,
      height: frame.height,
      format: wgpu::TextureFormat::R16Uint,
      dovi: Some(DoviPicture {
        chroma,
        uniform,
        staging,
        row_pitch,
        chroma_offset,
      }),
    })
  }
}

fn render_pipeline(
  device: &wgpu::Device,
  format: wgpu::TextureFormat,
  layout: &wgpu::BindGroupLayout,
  source: &str,
) -> wgpu::RenderPipeline {
  let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    label: Some("video pipeline layout"),
    bind_group_layouts: &[Some(layout)],
    immediate_size: 0,
  });
  let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("video shader"),
    source: wgpu::ShaderSource::Wgsl(source.into()),
  });
  device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    label: Some("video pipeline"),
    layout: Some(&pipeline_layout),
    vertex: wgpu::VertexState {
      module: &shader,
      entry_point: Some("vs_main"),
      compilation_options: Default::default(),
      buffers: &[],
    },
    primitive: wgpu::PrimitiveState::default(),
    depth_stencil: None,
    multisample: wgpu::MultisampleState::default(),
    fragment: Some(wgpu::FragmentState {
      module: &shader,
      entry_point: Some("fs_main"),
      compilation_options: Default::default(),
      targets: &[Some(wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::REPLACE),
        write_mask: wgpu::ColorWrites::ALL,
      })],
    }),
    multiview_mask: None,
    cache: None,
  })
}

fn frame_error(reason: impl Into<String>) -> PlaybackError {
  PlaybackError::Frame(reason.into())
}

#[derive(Clone, Copy)]
enum FrameFormat {
  Rgba,
  P010,
}

impl FrameFormat {
  fn native(self) -> gst_video::VideoFormat {
    match self {
      Self::Rgba => gst_video::VideoFormat::Rgba,
      Self::P010 => gst_video::VideoFormat::P01010le,
    }
  }

  fn plane_count(self) -> usize {
    match self {
      Self::Rgba => 1,
      Self::P010 => 2,
    }
  }
}

#[derive(Clone, Copy, Default)]
struct PlaneLayout {
  width: u32,
  height: u32,
  stride: u32,
  row_bytes: u32,
  plane_bytes: usize,
  texel_bytes: u32,
}

#[derive(Clone, Copy)]
struct FrameLayout {
  width: u32,
  height: u32,
  planes: [PlaneLayout; 2],
  plane_count: usize,
  aspect: f64,
  chroma_offset: [f32; 2],
}

impl FrameLayout {
  fn new(
    info: &gst_video::VideoInfo,
    format: FrameFormat,
    max_dimension: u32,
  ) -> Result<Self, PlaybackError> {
    if !info.is_valid()
      || info.format() != format.native()
      || info.n_planes() as usize != format.plane_count()
    {
      return Err(frame_error(
        "video layout does not match its color metadata",
      ));
    }
    let (width, height) = (info.width(), info.height());
    if width == 0 || height == 0 || width > max_dimension || height > max_dimension {
      return Err(frame_error(
        "video dimensions are zero or exceed GPU texture limits",
      ));
    }
    let par = info.par();
    if par.numer() <= 0 || par.denom() <= 0 {
      return Err(frame_error("pixel aspect ratio must be positive"));
    }
    let mut planes = [PlaneLayout::default(); 2];
    for (index, plane) in planes.iter_mut().enumerate().take(format.plane_count()) {
      let texel_bytes = match (format, index) {
        (FrameFormat::P010, 0) => 2,
        _ => 4,
      };
      let width = info.comp_width(index as u8);
      let height = info.comp_height(index as u8);
      let row_bytes = width
        .checked_mul(texel_bytes)
        .ok_or_else(|| frame_error("video row overflow"))?;
      let stride = u32::try_from(info.stride()[index])
        .ok()
        .filter(|stride| *stride >= row_bytes)
        .ok_or_else(|| frame_error("video stride must be positive and cover the row"))?;
      // Bindings 0.25.2 multiply u32 stride by plane height before creating a slice.
      let plane_bytes = stride
        .checked_mul(height)
        .ok_or_else(|| frame_error("video plane overflow"))? as usize;
      *plane = PlaneLayout {
        width,
        height,
        stride,
        row_bytes,
        plane_bytes,
        texel_bytes,
      };
    }
    let chroma_offset = if matches!(format, FrameFormat::P010) {
      let site = info.chroma_site();
      if info.interlace_mode() != gst_video::VideoInterlaceMode::Progressive
        || site.contains(gst_video::VideoChromaSite::ALT_LINE)
      {
        return Err(frame_error(
          "Dolby Vision requires progressive chroma sampling",
        ));
      }
      // HEVC's absent chroma-location syntax defaults to horizontal cositing.
      [
        if site.is_empty() || site.contains(gst_video::VideoChromaSite::H_COSITED) {
          0.25
        } else {
          0.0
        },
        if site.contains(gst_video::VideoChromaSite::V_COSITED) {
          0.25
        } else {
          0.0
        },
      ]
    } else {
      [0.0; 2]
    };
    Ok(Self {
      width,
      height,
      planes,
      plane_count: format.plane_count(),
      aspect: f64::from(width) * f64::from(par.numer())
        / (f64::from(height) * f64::from(par.denom())),
      chroma_offset,
    })
  }
}

struct MappedFrame {
  mapped: gst_video::VideoFrame<gst_video::video_frame::Readable>,
  layout: FrameLayout,
}

impl MappedFrame {
  fn map(
    sample: &gst::Sample,
    format: FrameFormat,
    max_dimension: u32,
  ) -> Result<Self, PlaybackError> {
    let caps = sample
      .caps()
      .ok_or_else(|| frame_error("sample has no video caps"))?;
    let info =
      gst_video::VideoInfo::from_caps(caps).map_err(|error| frame_error(error.to_string()))?;
    FrameLayout::new(&info, format, max_dimension)?;
    let buffer = sample
      .buffer()
      .ok_or_else(|| frame_error("sample has no video buffer"))?;
    let mapped = gst_video::VideoFrame::from_buffer_readable(buffer.to_owned(), &info)
      .map_err(|_| frame_error("cannot map video buffer"))?;
    // GstVideoMeta may override caps-derived stride/offset. Validate before plane_data.
    let layout = FrameLayout::new(mapped.info(), format, max_dimension)?;
    for (index, plane) in layout.planes.iter().enumerate().take(layout.plane_count) {
      let end = mapped.plane_offset()[index]
        .checked_add(plane.plane_bytes)
        .ok_or_else(|| frame_error("video offset overflow"))?;
      if end > mapped.buffer().size() {
        return Err(frame_error("video plane exceeds mapped buffer storage"));
      }
    }
    Ok(Self { mapped, layout })
  }

  fn upload_bytes<'a>(
    &'a self,
    index: usize,
    scratch: &'a mut Vec<u8>,
  ) -> Result<(&'a [u8], u32), PlaybackError> {
    if index >= self.layout.plane_count {
      return Err(frame_error("video plane index is out of range"));
    }
    let layout = self.layout.planes[index];
    // plane_data already applies its offset: never apply it a second time.
    let plane = self
      .mapped
      .plane_data(index as u32)
      .map_err(|error| frame_error(error.to_string()))?;
    if plane.len() < layout.plane_bytes {
      return Err(frame_error(
        "mapped plane is shorter than its stride layout",
      ));
    }
    if layout.stride.is_multiple_of(layout.texel_bytes) {
      return Ok((&plane[..layout.plane_bytes], layout.stride));
    }
    // GStreamer permits byte padding; wgpu requires a whole texel row stride.
    let row_bytes = layout.row_bytes as usize;
    let packed_bytes = row_bytes
      .checked_mul(layout.height as usize)
      .ok_or_else(|| frame_error("packed video size overflow"))?;
    scratch.resize(packed_bytes, 0);
    for (destination, source) in scratch
      .chunks_exact_mut(row_bytes)
      .zip(plane.chunks_exact(layout.stride as usize))
    {
      destination.copy_from_slice(&source[..row_bytes]);
    }
    Ok((scratch, layout.row_bytes))
  }
}

fn upload_plane(
  queue: &wgpu::Queue,
  texture: &wgpu::Texture,
  bytes: &[u8],
  stride: u32,
  layout: PlaneLayout,
) {
  queue.write_texture(
    wgpu::TexelCopyTextureInfo {
      texture,
      mip_level: 0,
      origin: wgpu::Origin3d::ZERO,
      aspect: wgpu::TextureAspect::All,
    },
    bytes,
    wgpu::TexelCopyBufferLayout {
      offset: 0,
      bytes_per_row: Some(stride),
      rows_per_image: Some(layout.height),
    },
    wgpu::Extent3d {
      width: layout.width,
      height: layout.height,
      depth_or_array_layers: 1,
    },
  );
}

fn contain(bounds: Rectangle, aspect: f64) -> Option<Rectangle> {
  if ![bounds.x, bounds.y, bounds.width, bounds.height]
    .into_iter()
    .all(f32::is_finite)
    || bounds.width <= 0.0
    || bounds.height <= 0.0
    || !aspect.is_finite()
    || aspect <= 0.0
  {
    return None;
  }
  let width = f64::from(bounds.width).min(f64::from(bounds.height) * aspect);
  let height = width / aspect;
  let (width, height) = (width as f32, height as f32);
  if width <= 0.0 || height <= 0.0 {
    return None;
  }
  Some(Rectangle {
    x: bounds.x + (bounds.width - width) * 0.5,
    y: bounds.y + (bounds.height - height) * 0.5,
    width,
    height,
  })
}

fn scissor(
  surface: Rectangle,
  clip: Rectangle<u32>,
  target: Rectangle<u32>,
) -> Option<Rectangle<u32>> {
  let left = surface.x.floor().max(clip.x as f32).max(target.x as f32) as u32;
  let top = surface.y.floor().max(clip.y as f32).max(target.y as f32) as u32;
  let right = (surface.x + surface.width)
    .ceil()
    .min(clip.x.saturating_add(clip.width) as f32)
    .min(target.x.saturating_add(target.width) as f32)
    .max(0.0) as u32;
  let bottom = (surface.y + surface.height)
    .ceil()
    .min(clip.y.saturating_add(clip.height) as f32)
    .min(target.y.saturating_add(target.height) as f32)
    .max(0.0) as u32;
  (right > left && bottom > top).then(|| Rectangle {
    x: left,
    y: top,
    width: right - left,
    height: bottom - top,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn padded_p010(chroma_stride: i32) -> gst::Sample {
    gst::init().expect("GStreamer initialization");
    let info = gst_video::VideoInfo::builder(gst_video::VideoFormat::P01010le, 3, 3)
      .build()
      .expect("P010 caps");
    let mut bytes = vec![0xee; 56];
    for row in 0..3 {
      let offset = 5 + row * 7;
      bytes[offset..offset + 6].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
    }
    for row in 0..2 {
      let offset = 31 + row * 9;
      bytes[offset..offset + 8].copy_from_slice(&[21, 22, 23, 24, 25, 26, 27, 28]);
    }
    let mut buffer = gst::Buffer::from_mut_slice(bytes);
    gst_video::VideoMeta::add_full(
      buffer.get_mut().expect("unique buffer"),
      gst_video::VideoFrameFlags::empty(),
      gst_video::VideoFormat::P01010le,
      3,
      3,
      &[5, 31],
      &[7, chroma_stride],
    )
    .expect("two-plane video metadata");
    gst::Sample::builder()
      .buffer(&buffer)
      .caps(&info.to_caps().expect("caps"))
      .build()
  }

  #[test]
  fn odd_p010_dimensions_preserve_both_offset_and_padded_planes() {
    let sample = padded_p010(9);
    let frame = MappedFrame::map(&sample, FrameFormat::P010, 4096).expect("P010 frame");
    let mut scratch = Vec::new();
    let (luma, stride) = frame.upload_bytes(0, &mut scratch).expect("luma upload");
    assert_eq!(stride, 6);
    assert_eq!(luma, [1, 2, 3, 4, 5, 6].repeat(3));
    let (chroma, stride) = frame.upload_bytes(1, &mut scratch).expect("chroma upload");
    assert_eq!(stride, 8);
    assert_eq!(chroma, [21, 22, 23, 24, 25, 26, 27, 28].repeat(2));
  }

  #[test]
  fn invalid_secondary_plane_is_rejected_before_p010_slice_access() {
    let sample = padded_p010(-8);
    assert!(matches!(
      MappedFrame::map(&sample, FrameFormat::P010, 4096),
      Err(PlaybackError::Frame(_))
    ));
  }

  fn padded_sample(stride: i32) -> gst::Sample {
    gst::init().expect("GStreamer initialization");
    let info = gst_video::VideoInfo::builder(gst_video::VideoFormat::Rgba, 2, 2)
      .par((4, 3))
      .build()
      .expect("RGBA caps");
    let offset = 7;
    let mut bytes = vec![0xee; offset + stride as usize * 2];
    bytes[offset..offset + 8].copy_from_slice(&[1, 2, 3, 255, 4, 5, 6, 255]);
    let second = offset + stride as usize;
    bytes[second..second + 8].copy_from_slice(&[7, 8, 9, 255, 10, 11, 12, 255]);
    let mut buffer = gst::Buffer::from_mut_slice(bytes);
    gst_video::VideoMeta::add_full(
      buffer.get_mut().expect("unique buffer"),
      gst_video::VideoFrameFlags::empty(),
      gst_video::VideoFormat::Rgba,
      2,
      2,
      &[offset],
      &[stride],
    )
    .expect("padded video metadata");
    gst::Sample::builder()
      .buffer(&buffer)
      .caps(&info.to_caps().expect("caps"))
      .build()
  }

  #[test]
  fn negative_stride_is_rejected_before_plane_slice_access() {
    gst::init().expect("GStreamer initialization");
    let info = gst_video::VideoInfo::builder(gst_video::VideoFormat::Rgba, 2, 2)
      .build()
      .expect("RGBA caps");
    let mut buffer = gst::Buffer::with_size(32).expect("frame storage");
    gst_video::VideoMeta::add_full(
      buffer.get_mut().expect("unique buffer"),
      gst_video::VideoFrameFlags::empty(),
      gst_video::VideoFormat::Rgba,
      2,
      2,
      &[8],
      &[-8],
    )
    .expect("bottom-up video metadata");
    let sample = gst::Sample::builder()
      .buffer(&buffer)
      .caps(&info.to_caps().expect("caps"))
      .build();
    assert!(matches!(
      MappedFrame::map(&sample, FrameFormat::Rgba, 4096),
      Err(PlaybackError::Frame(_))
    ));
  }

  #[test]
  fn mapped_offset_and_padding_preserve_both_pixel_rows() {
    let sample = padded_sample(12);
    let rgba = MappedFrame::map(&sample, FrameFormat::Rgba, 4096).expect("mapped padded frame");
    let mut scratch = Vec::new();
    let (bytes, stride) = rgba.upload_bytes(0, &mut scratch).expect("upload bytes");
    let rows: Vec<_> = bytes
      .chunks_exact(stride as usize)
      .flat_map(|row| row[..8].iter().copied())
      .collect();
    assert_eq!(
      rows,
      [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
    );
    assert!(
      scratch.is_empty(),
      "whole-texel padding must not copy the image"
    );
  }

  #[test]
  fn byte_padding_is_packed_without_offset_or_padding_pixels() {
    let sample = padded_sample(9);
    let rgba =
      MappedFrame::map(&sample, FrameFormat::Rgba, 4096).expect("mapped byte-padded frame");
    let mut scratch = Vec::with_capacity(32);
    let allocation = scratch.as_ptr();
    let (bytes, stride) = rgba.upload_bytes(0, &mut scratch).expect("packed bytes");
    assert_eq!(stride, 8);
    assert_eq!(
      bytes,
      [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
    );
    assert_eq!(
      scratch.as_ptr(),
      allocation,
      "reuse the upload scratch allocation"
    );
  }

  #[test]
  fn non_square_pixels_contain_in_tall_and_wide_bounds() {
    let sample = padded_sample(12);
    let rgba = MappedFrame::map(&sample, FrameFormat::Rgba, 4096).expect("PAR frame");
    for bounds in [
      Rectangle {
        x: 17.0,
        y: 31.0,
        width: 300.0,
        height: 600.0,
      },
      Rectangle {
        x: 17.0,
        y: 31.0,
        width: 900.0,
        height: 200.0,
      },
    ] {
      let rect = contain(bounds, rgba.layout.aspect).expect("contained rectangle");
      assert!((f64::from(rect.width / rect.height) - 4.0 / 3.0).abs() < 0.00001);
      assert!(
        rect.x >= bounds.x
          && rect.y >= bounds.y
          && rect.x + rect.width <= bounds.x + bounds.width
          && rect.y + rect.height <= bounds.y + bounds.height
      );
      assert!((rect.x + rect.width * 0.5 - (bounds.x + bounds.width * 0.5)).abs() < 0.0001);
      assert!((rect.y + rect.height * 0.5 - (bounds.y + bounds.height * 0.5)).abs() < 0.0001);
    }
  }

  #[test]
  fn scissor_intersects_surface_parent_clip_and_target() {
    let surface = Rectangle {
      x: -20.0,
      y: 15.0,
      width: 100.0,
      height: 90.0,
    };
    let clip = Rectangle {
      x: 10,
      y: 0,
      width: 200,
      height: 100,
    };
    let target = Rectangle {
      x: 0,
      y: 0,
      width: 60,
      height: 80,
    };
    assert_eq!(
      scissor(surface, clip, target),
      Some(Rectangle {
        x: 10,
        y: 15,
        width: 50,
        height: 65
      })
    );
    assert!(scissor(
      surface,
      Rectangle {
        x: 200,
        y: 0,
        width: 10,
        height: 10
      },
      target
    )
    .is_none());
  }
}
