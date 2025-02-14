use std::time::Duration;

use futures::executor::block_on;
use glutin::{
    dpi, ContextTrait, DeviceEvent, ElementState, Event, EventsLoop, GlProfile, GlRequest,
    MouseButton, MouseScrollDelta, Window, WindowBuilder, WindowEvent, WindowedContext,
};
use hedgewars_engine::instance::EngineInstance;
use integral_geometry::Point;
use std::error::Error;
use wgpu::{
    Adapter, BackendBit, Color, CommandEncoderDescriptor, Device, DeviceDescriptor, Features,
    LoadOp, Operations, PowerPreference, PresentMode, Queue, RenderPassColorAttachmentDescriptor,
    RenderPassDescriptor, RequestAdapterOptions, Surface, SwapChain, SwapChainDescriptor,
    TextureFormat, TextureUsage,
};

type HwGlRendererContext = WindowedContext;

struct HwWgpuRenderingContext {
    window: Window,
    surface: Surface,
    adapter: Adapter,
    device: Device,
    queue: Queue,
    swap_chain: SwapChain,
}

enum HwRendererContext {
    Gl(HwGlRendererContext),
    Wgpu(HwWgpuRenderingContext),
}

struct ErrorStub;

impl<T: Error> From<T> for ErrorStub {
    fn from(_: T) -> Self {
        ErrorStub
    }
}

impl HwRendererContext {
    fn get_framebuffer_size(window: &Window) -> (u32, u32) {
        let size = window.get_inner_size().unwrap();
        (size.to_physical(window.get_hidpi_factor())).into()
    }

    fn create_wpgu_swap_chain(window: &Window, surface: &Surface, device: &Device) -> SwapChain {
        let (width, height) = Self::get_framebuffer_size(window);
        device.create_swap_chain(
            &surface,
            &SwapChainDescriptor {
                usage: TextureUsage::OUTPUT_ATTACHMENT,
                format: TextureFormat::Bgra8Unorm,
                width,
                height,
                present_mode: PresentMode::Fifo,
            },
        )
    }

    fn init_wgpu(event_loop: &EventsLoop, size: dpi::LogicalSize) -> HwWgpuRenderingContext {
        let builder = WindowBuilder::new()
            .with_title("hwengine")
            .with_dimensions(size);
        let window = builder.build(event_loop).unwrap();

        let instance = wgpu::Instance::new(BackendBit::VULKAN);

        let surface = unsafe { instance.create_surface(&window) };

        let adapter = block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
        }))
        .unwrap();

        let (device, queue) = block_on(adapter.request_device(&Default::default(), None)).unwrap();

        let swap_chain = Self::create_wpgu_swap_chain(&window, &surface, &device);

        HwWgpuRenderingContext {
            window,
            surface,
            adapter,
            device,
            queue,
            swap_chain,
        }
    }

    fn init_gl(event_loop: &EventsLoop, size: dpi::LogicalSize) -> HwGlRendererContext {
        use glutin::ContextBuilder;

        let builder = WindowBuilder::new()
            .with_title("hwengine")
            .with_dimensions(size);

        let context = ContextBuilder::new()
            .with_gl(GlRequest::Latest)
            .with_gl_profile(GlProfile::Core)
            .build_windowed(builder, &event_loop)
            .ok()
            .unwrap();

        unsafe {
            context.make_current().unwrap();
            gl::load_with(|ptr| context.get_proc_address(ptr) as *const _);

            if let Some(sz) = context.get_inner_size() {
                let (width, height) = Self::get_framebuffer_size(context.window());
                gl::Viewport(0, 0, width as i32, height as i32);
            }
        }

        context
    }

    fn new(event_loop: &EventsLoop, size: dpi::LogicalSize, use_wgpu: bool) -> Self {
        if use_wgpu {
            Self::Wgpu(Self::init_wgpu(event_loop, size))
        } else {
            Self::Gl(Self::init_gl(event_loop, size))
        }
    }

    pub fn window(&self) -> &Window {
        match self {
            HwRendererContext::Gl(gl) => &gl.window(),
            HwRendererContext::Wgpu(wgpu) => &wgpu.window,
        }
    }

    pub fn update(&mut self) {
        match self {
            HwRendererContext::Gl(context) => unsafe {
                let (width, height) = Self::get_framebuffer_size(&context.window());
                gl::Viewport(0, 0, width as i32, height as i32);
            },
            HwRendererContext::Wgpu(context) => {
                context.swap_chain = Self::create_wpgu_swap_chain(
                    &context.window,
                    &context.surface,
                    &context.device,
                );
            }
        }
    }

    pub fn present(&mut self) -> Result<(), ErrorStub> {
        match self {
            HwRendererContext::Gl(context) => context.swap_buffers()?,
            HwRendererContext::Wgpu(context) => {
                let frame_view = &context.swap_chain.get_current_frame()?.output.view;

                let mut encoder =
                    context
                        .device
                        .create_command_encoder(&CommandEncoderDescriptor {
                            label: Some("Main encoder"),
                        });
                encoder.begin_render_pass(&RenderPassDescriptor {
                    color_attachments: &[RenderPassColorAttachmentDescriptor {
                        attachment: &frame_view,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(Color {
                                r: 0.7,
                                g: 0.4,
                                b: 0.2,
                                a: 1.0,
                            }),
                            store: false,
                        },
                    }],
                    depth_stencil_attachment: None,
                });
                let buffer = encoder.finish();
                context.queue.submit(std::iter::once(buffer));
            }
        }
        Ok(())
    }
}

fn main() {
    let use_wgpu = true;
    let mut event_loop = EventsLoop::new();
    let (w, h) = (1024.0, 768.0);

    let mut context = HwRendererContext::new(&event_loop, dpi::LogicalSize::new(w, h), use_wgpu);

    let mut engine = EngineInstance::new();
    if !use_wgpu {
        engine.world.create_renderer(w as u16, h as u16);
    }

    let mut dragging = false;

    use std::time::Instant;

    let mut now = Instant::now();
    let mut update_time = Instant::now();
    let mut render_time = Instant::now();

    let mut is_running = true;

    while is_running {
        let current_time = Instant::now();
        let delta = current_time - now;
        now = current_time;
        let ms = delta.as_secs() as f64 * 1000.0 + delta.subsec_millis() as f64;
        context.window().set_title(&format!("hwengine {:.3}ms", ms));

        if update_time.elapsed() > Duration::from_millis(10) {
            update_time = current_time;
            engine.world.step()
        }

        event_loop.poll_events(|event| match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    is_running = false;
                }
                WindowEvent::Resized(_) | WindowEvent::HiDpiFactorChanged(_) => context.update(),

                WindowEvent::MouseInput { button, state, .. } => {
                    if let MouseButton::Right = button {
                        dragging = state == ElementState::Pressed;
                    }
                }

                WindowEvent::MouseWheel { delta, .. } => {
                    let zoom_change = match delta {
                        MouseScrollDelta::LineDelta(x, y) => y as f32 * 0.1f32,
                        MouseScrollDelta::PixelDelta(delta) => {
                            let physical = delta.to_physical(context.window().get_hidpi_factor());
                            physical.y as f32 * 0.1f32
                        }
                    };
                    engine.world.move_camera(Point::ZERO, zoom_change);
                }
                _ => (),
            },
            Event::DeviceEvent { event, .. } => match event {
                DeviceEvent::MouseMotion { delta } => {
                    if dragging {
                        engine
                            .world
                            .move_camera(Point::new(delta.0 as i32, delta.1 as i32), 0.0)
                    }
                }
                _ => {}
            },
            _ => (),
        });

        if render_time.elapsed() > Duration::from_millis(16) {
            render_time = current_time;
            if !use_wgpu {
                engine.render();
            }
            context.present().ok().unwrap();
        }
    }
}
