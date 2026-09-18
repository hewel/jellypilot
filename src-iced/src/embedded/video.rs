use std::sync::Mutex;

use iced::advanced::graphics::{Shell, Viewport};
use iced::widget::shader;
use iced::{mouse, Rectangle};
use iced_wgpu::{primitive, wgpu};

pub(super) static FRAME: Mutex<Frame> = Mutex::new(Frame {
  binding: None,
  layout: None,
  size: (1, 1),
  shell: None,
  hdr: false,
  valid: false,
});

pub(super) struct Frame {
  pub binding: Option<wgpu::BindGroup>,
  pub layout: Option<wgpu::BindGroupLayout>,
  pub size: (u32, u32),
  pub shell: Option<Shell>,
  pub hdr: bool,
  pub valid: bool,
}

pub(super) fn layout_descriptor() -> wgpu::BindGroupLayoutDescriptor<'static> {
  wgpu::BindGroupLayoutDescriptor {
    label: Some("JellyPilot embedded video"),
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
  }
}

pub(crate) struct Video;

impl<Message> shader::Program<Message> for Video {
  type State = ();
  type Primitive = VideoPrimitive;

  fn draw(&self, _state: &(), _cursor: mouse::Cursor, _bounds: Rectangle) -> VideoPrimitive {
    VideoPrimitive
  }
}

#[derive(Debug)]
pub(crate) struct VideoPrimitive;

pub(crate) struct Pipeline {
  sdr: wgpu::RenderPipeline,
  hdr: wgpu::RenderPipeline,
  empty: wgpu::RenderPipeline,
  binding: Option<wgpu::BindGroup>,
  is_hdr: bool,
  valid: bool,
}

impl primitive::Pipeline for Pipeline {
  fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
    let binding_layout = FRAME
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .layout
      .clone()
      .unwrap_or_else(|| device.create_bind_group_layout(&layout_descriptor()));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("JellyPilot video pipeline layout"),
      bind_group_layouts: &[Some(&binding_layout)],
      immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::include_wgsl!("video.wgsl"));
    let pipeline = |entry, layout| {
      device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout,
        vertex: wgpu::VertexState {
          module: &shader,
          entry_point: Some("vertex_main"),
          buffers: &[],
          compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
          module: &shader,
          entry_point: Some(entry),
          targets: &[Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
          })],
          compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
      })
    };
    Self {
      sdr: pipeline("fragment_main", Some(&layout)),
      hdr: pipeline("hdr_main", Some(&layout)),
      empty: pipeline("empty_main", None),
      binding: None,
      is_hdr: false,
      valid: false,
    }
  }
}

impl primitive::Primitive for VideoPrimitive {
  type Pipeline = Pipeline;

  fn prepare(
    &self,
    pipeline: &mut Pipeline,
    _device: &wgpu::Device,
    _queue: &wgpu::Queue,
    bounds: &Rectangle,
    viewport: &Viewport,
  ) {
    let size = (
      (bounds.width * viewport.scale_factor()).round().max(1.0) as u32,
      (bounds.height * viewport.scale_factor()).round().max(1.0) as u32,
    );
    let mut frame = FRAME
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    pipeline.binding = frame.binding.clone();
    pipeline.is_hdr = frame.hdr;
    pipeline.valid = frame.valid;
    if frame.size != size {
      frame.size = size;
      if let Some(shell) = &frame.shell {
        shell.request_redraw();
      }
    }
  }

  fn draw(&self, pipeline: &Pipeline, pass: &mut wgpu::RenderPass<'_>) -> bool {
    // Keep video in iced's normal scene order so clipping, backdrop effects
    // and screenshots include it. Invalidated target epochs render black.
    if let Some(binding) = pipeline.binding.as_ref().filter(|_| pipeline.valid) {
      pass.set_pipeline(if pipeline.is_hdr {
        &pipeline.hdr
      } else {
        &pipeline.sdr
      });
      pass.set_bind_group(0, binding, &[]);
    } else {
      pass.set_pipeline(&pipeline.empty);
    }
    pass.draw(0..3, 0..1);
    true
  }
}

pub(crate) struct Composite {
  hdr: wgpu::RenderPipeline,
  sdr: wgpu::RenderPipeline,
  layout: wgpu::BindGroupLayout,
  sampler: wgpu::Sampler,
}

impl Composite {
  /// Converts the complete extended-sRGB scene to the presentation encoding.
  /// UI blending retains iced's existing web-colors semantics in both modes.
  pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
    let layout = device.create_bind_group_layout(&layout_descriptor());
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("JellyPilot presentation layout"),
      bind_group_layouts: &[Some(&layout)],
      immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::include_wgsl!("composite.wgsl"));
    let pipeline = |entry| {
      device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
          module: &shader,
          entry_point: Some("vertex_main"),
          buffers: &[],
          compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
          module: &shader,
          entry_point: Some(entry),
          targets: &[Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
          })],
          compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
      })
    };
    let hdr = pipeline("hdr_main");
    let sdr = pipeline("sdr_main");
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      label: Some("JellyPilot presentation sampler"),
      mag_filter: wgpu::FilterMode::Nearest,
      min_filter: wgpu::FilterMode::Nearest,
      ..Default::default()
    });
    Self {
      hdr,
      sdr,
      layout,
      sampler,
    }
  }

  pub(crate) fn bind_scene(
    &self,
    device: &wgpu::Device,
    scene: &wgpu::TextureView,
  ) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("JellyPilot presentation bindings"),
      layout: &self.layout,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(scene),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::Sampler(&self.sampler),
        },
      ],
    })
  }

  /// Called under the shared device queue gate, after iced's scene submission.
  pub(crate) fn submit(
    &self,
    context: &jellypilot_mpv_host::DeviceContext,
    target: &wgpu::TextureView,
    binding: &wgpu::BindGroup,
    hdr: bool,
  ) {
    let mut encoder = context
      .device()
      .create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("JellyPilot presentation encoder"),
      });
    {
      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("JellyPilot presentation pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
          view: target,
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
      });
      pass.set_pipeline(if hdr { &self.hdr } else { &self.sdr });
      pass.set_bind_group(0, binding, &[]);
      pass.draw(0..3, 0..1);
    }
    context.queue().submit([encoder.finish()]);
  }
}
