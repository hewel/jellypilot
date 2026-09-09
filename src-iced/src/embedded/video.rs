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
});

pub(super) struct Frame {
  pub binding: Option<wgpu::BindGroup>,
  pub layout: Option<wgpu::BindGroupLayout>,
  pub size: (u32, u32),
  pub shell: Option<Shell>,
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
  render: wgpu::RenderPipeline,
  binding: Option<wgpu::BindGroup>,
}

impl primitive::Pipeline for Pipeline {
  fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
    let binding_layout = FRAME
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .layout
      .clone()
      .unwrap_or_else(|| device.create_bind_group_layout(&layout_descriptor()));
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("JellyPilot video pipeline layout"),
      bind_group_layouts: &[Some(&binding_layout)],
      immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::include_wgsl!("video.wgsl"));
    let render = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("JellyPilot 10-bit SDR video"),
      layout: Some(&pipeline_layout),
      vertex: wgpu::VertexState {
        module: &shader,
        entry_point: Some("vertex_main"),
        buffers: &[],
        compilation_options: Default::default(),
      },
      fragment: Some(wgpu::FragmentState {
        module: &shader,
        entry_point: Some("fragment_main"),
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
    });
    Self {
      render,
      binding: None,
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
    if frame.size != size {
      frame.size = size;
      if let Some(shell) = &frame.shell {
        shell.request_redraw();
      }
    }
  }

  fn draw(&self, pipeline: &Pipeline, pass: &mut wgpu::RenderPass<'_>) -> bool {
    if let Some(binding) = &pipeline.binding {
      // iced already applies the widget's viewport and clipping rectangle.
      pass.set_pipeline(&pipeline.render);
      pass.set_bind_group(0, binding, &[]);
      pass.draw(0..3, 0..1);
    }
    true
  }
}
