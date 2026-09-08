use std::sync::{Arc, Mutex, MutexGuard};

use gstreamer as gst;
use gstreamer_video::{self as gst_video, prelude::VideoFrameExt};
use iced::widget::shader::{self, Viewport};
use iced::{mouse, wgpu, Rectangle};

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
      let rgba = RgbaFrame::map(&frame.sample, device.limits().max_texture_dimension_2d)?;
      let layout = rgba.layout;
      let needs_texture = state.texture.as_ref().is_none_or(|picture| {
        picture.width != layout.width
          || picture.height != layout.height
          || picture.format != pipeline.texture_format
      });
      if needs_texture {
        state.texture = Some(pipeline.picture(device, layout.width, layout.height));
      }
      let SurfaceState {
        texture, scratch, ..
      } = &mut *state;
      let picture = texture
        .as_ref()
        .ok_or_else(|| frame_error("texture was not allocated"))?;
      let (bytes, bytes_per_row) = rgba.upload_bytes(scratch)?;
      queue.write_texture(
        wgpu::TexelCopyTextureInfo {
          texture: &picture.texture,
          mip_level: 0,
          origin: wgpu::Origin3d::ZERO,
          aspect: wgpu::TextureAspect::All,
        },
        bytes,
        wgpu::TexelCopyBufferLayout {
          offset: 0,
          bytes_per_row: Some(bytes_per_row),
          rows_per_image: Some(layout.height),
        },
        wgpu::Extent3d {
          width: layout.width,
          height: layout.height,
          depth_or_array_layers: 1,
        },
      );
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
    pass.set_pipeline(&pipeline.pipeline);
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
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("local video pipeline layout"),
      bind_group_layouts: &[Some(&layout)],
      immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("local video shader"),
      source: wgpu::ShaderSource::Wgsl(include_str!("video.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("local video pipeline"),
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
    });
    Self {
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
    }
  }
}

fn frame_error(reason: impl Into<String>) -> PlaybackError {
  PlaybackError::Frame(reason.into())
}

#[derive(Clone, Copy)]
struct RgbaLayout {
  width: u32,
  height: u32,
  stride: u32,
  row_bytes: u32,
  plane_bytes: usize,
  aspect: f64,
}

impl RgbaLayout {
  fn new(info: &gst_video::VideoInfo, max_dimension: u32) -> Result<Self, PlaybackError> {
    if !info.is_valid() || info.format() != gst_video::VideoFormat::Rgba || info.n_planes() != 1 {
      return Err(frame_error("expected valid single-plane RGBA video"));
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
    let row_bytes = width
      .checked_mul(4)
      .ok_or_else(|| frame_error("RGBA row overflow"))?;
    let stride = *info
      .stride()
      .first()
      .ok_or_else(|| frame_error("missing RGBA stride"))?;
    let stride = u32::try_from(stride)
      .ok()
      .filter(|stride| *stride >= row_bytes)
      .ok_or_else(|| frame_error("RGBA stride must be positive and cover the row"))?;
    // bindings 0.25.2 plane_data multiplies u32 stride by height before creating a slice.
    let plane_bytes = stride
      .checked_mul(height)
      .ok_or_else(|| frame_error("RGBA plane overflow"))? as usize;
    Ok(Self {
      width,
      height,
      stride,
      row_bytes,
      plane_bytes,
      aspect: f64::from(width) * f64::from(par.numer())
        / (f64::from(height) * f64::from(par.denom())),
    })
  }
}

struct RgbaFrame {
  mapped: gst_video::VideoFrame<gst_video::video_frame::Readable>,
  layout: RgbaLayout,
}

impl RgbaFrame {
  fn map(sample: &gst::Sample, max_dimension: u32) -> Result<Self, PlaybackError> {
    let caps = sample
      .caps()
      .ok_or_else(|| frame_error("sample has no video caps"))?;
    let info =
      gst_video::VideoInfo::from_caps(caps).map_err(|error| frame_error(error.to_string()))?;
    RgbaLayout::new(&info, max_dimension)?;
    let buffer = sample
      .buffer()
      .ok_or_else(|| frame_error("sample has no video buffer"))?;
    let mapped = gst_video::VideoFrame::from_buffer_readable(buffer.to_owned(), &info)
      .map_err(|_| frame_error("cannot map RGBA video buffer"))?;
    // GstVideoMeta may override caps-derived stride/offset. Validate the mapped layout too.
    let layout = RgbaLayout::new(mapped.info(), max_dimension)?;
    let offset = *mapped
      .plane_offset()
      .first()
      .ok_or_else(|| frame_error("missing RGBA plane offset"))?;
    let end = offset
      .checked_add(layout.plane_bytes)
      .ok_or_else(|| frame_error("RGBA offset overflow"))?;
    if end > mapped.buffer().size() {
      return Err(frame_error("RGBA plane exceeds mapped buffer storage"));
    }
    Ok(Self { mapped, layout })
  }

  fn upload_bytes<'a>(
    &'a self,
    scratch: &'a mut Vec<u8>,
  ) -> Result<(&'a [u8], u32), PlaybackError> {
    // plane_data already applies the plane offset: never apply it a second time.
    let plane = self
      .mapped
      .plane_data(0)
      .map_err(|error| frame_error(error.to_string()))?;
    if plane.len() < self.layout.plane_bytes {
      return Err(frame_error(
        "RGBA mapped plane is shorter than its stride layout",
      ));
    }
    if self.layout.stride.is_multiple_of(4) {
      return Ok((&plane[..self.layout.plane_bytes], self.layout.stride));
    }
    // wgpu requires a whole texel row stride, but GStreamer permits byte padding.
    let row_bytes = self.layout.row_bytes as usize;
    let packed_bytes = row_bytes
      .checked_mul(self.layout.height as usize)
      .ok_or_else(|| frame_error("packed RGBA size overflow"))?;
    scratch.resize(packed_bytes, 0);
    for (destination, source) in scratch
      .chunks_exact_mut(row_bytes)
      .zip(plane.chunks_exact(self.layout.stride as usize))
    {
      destination.copy_from_slice(&source[..row_bytes]);
    }
    Ok((scratch, self.layout.row_bytes))
  }
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
      RgbaFrame::map(&sample, 4096),
      Err(PlaybackError::Frame(_))
    ));
  }

  #[test]
  fn mapped_offset_and_padding_preserve_both_pixel_rows() {
    let sample = padded_sample(12);
    let rgba = RgbaFrame::map(&sample, 4096).expect("mapped padded frame");
    let mut scratch = Vec::new();
    let (bytes, stride) = rgba.upload_bytes(&mut scratch).expect("upload bytes");
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
    let rgba = RgbaFrame::map(&sample, 4096).expect("mapped byte-padded frame");
    let mut scratch = Vec::with_capacity(32);
    let allocation = scratch.as_ptr();
    let (bytes, stride) = rgba.upload_bytes(&mut scratch).expect("packed bytes");
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
    let rgba = RgbaFrame::map(&sample, 4096).expect("PAR frame");
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
