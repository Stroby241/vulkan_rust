pub extern crate anyhow;
pub extern crate glam;
pub extern crate log;
pub extern crate vulkan;
pub extern crate imgui;
pub extern crate imgui_rs_vulkan_renderer;
pub extern crate imgui_winit_support;

pub mod camera;
pub mod logger;
pub mod controls;
pub mod gui;

use anyhow::Result;
use ash::vk::{self};
use controls::Controls;
use gpu_allocator::MemoryLocation;
use logger::log_init;
use std::{
    marker::PhantomData,
    thread,
    time::{Duration, Instant},
};
use glam::{Mat4, Vec3, vec3};
use imgui::{FontConfig, FontSource, SuspendedContext};
use imgui_rs_vulkan_renderer::{DynamicRendering, Options, Renderer};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use vulkan::*;
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};
use vulkan::ash::vk::Format;
use crate::camera::{Camera, perspective};
use crate::gui::{Gui, MainGui, StatsDisplayMode};

const IN_FLIGHT_FRAMES: u32 = 2;

pub struct BaseApp<B: App> {
    phantom: PhantomData<B>,
    raytracing_enabled: bool,
    compute_rendering_enabled: bool,
    pub swapchain: Swapchain,
    pub command_pool: CommandPool,
    pub storage_images: Vec<ImageAndView>,
    command_buffers: Vec<CommandBuffer>,
    in_flight_frames: InFlightFrames,
    pub context: Context,
}

pub trait App: Sized {
    type Gui: Gui;

    fn new(base: &mut BaseApp<Self>) -> Result<Self>;

    fn update(
        &mut self,
        base: &mut BaseApp<Self>,
        gui: &mut Self::Gui,
        image_index: usize,
        delta_time: Duration,
        controls: &Controls,
    ) -> Result<()>;

    fn record_raytracing_commands(
        &mut self,
        base: &BaseApp<Self>,
        buffer: &CommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        // prevents reports of unused parameters without needing to use #[allow]
        let _ = base;
        let _ = buffer;
        let _ = image_index;

        Ok(())
    }

    fn record_raster_commands(
        &mut self,
        base: &BaseApp<Self>,
        buffer: &CommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        // prevents reports of unused parameters without needing to use #[allow]
        let _ = base;
        let _ = buffer;
        let _ = image_index;

        Ok(())
    }

    fn record_compute_commands(
        &mut self,
        base: &BaseApp<Self>,
        buffer: &CommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        // prevents reports of unused parameters without needing to use #[allow]
        let _ = base;
        let _ = buffer;
        let _ = image_index;

        Ok(())
    }

    fn on_recreate_swapchain(&mut self, base: &BaseApp<Self>) -> Result<()>;
}

pub fn run<A: App + 'static>(
    app_name: &str,
    width: u32,
    height: u32,
    enable_raytracing: bool,
    enabled_compute_rendering: bool,
) -> Result<()> {
    log_init("app_log.log");

    let (window, event_loop) = create_window(app_name, width, height);
    
    let mut base_app = BaseApp::new(
        &window,
        app_name,
        enable_raytracing,
        enabled_compute_rendering,
    )?;
    
    let mut main_gui= MainGui::new(
        &base_app.context, 
        &base_app.command_pool,
        &window,
        base_app.swapchain.format,
        base_app.swapchain.images.len(),
    )?;
    
    let mut app = A::new(&mut base_app)?;
    let mut ui = Gui::new()?;
    
    let mut controls = Controls::default();
    let mut is_swapchain_dirty = false;

    let mut last_frame = Instant::now();
    let mut last_frame_start = Instant::now();

    let mut frame_stats = FrameStats::default();

    let fps_as_duration = Duration::from_secs_f64(1.0 / 60.0);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        let app = &mut app; // Make sure it is dropped before base_app
        
        main_gui.handle_event(&window, &event);
        controls = controls.handle_event(&event);

        match event {
            Event::NewEvents(_) => {
                let frame_start = Instant::now();
                let frame_time = frame_start - last_frame;
                let compute_time = frame_start - last_frame_start;
                last_frame = frame_start;

                if fps_as_duration > compute_time {
                    thread::sleep(fps_as_duration - compute_time)
                };
                last_frame_start = Instant::now();

                main_gui.update_delta_time(frame_time);
                frame_stats.set_frame_time(frame_time, compute_time);

                controls = controls.reset();
            }
            // On resize
            Event::WindowEvent {
                event: WindowEvent::Resized(..),
                ..
            } => {
                log::debug!("Window has been resized");
                is_swapchain_dirty = true;
            }
            // Draw
            Event::MainEventsCleared => {
                if is_swapchain_dirty {
                    let dim = window.inner_size();
                    if dim.width > 0 && dim.height > 0 {
                        base_app
                            .recreate_swapchain(dim.width, dim.height)
                            .expect("Failed to recreate swapchain");
                        app.on_recreate_swapchain(&base_app)
                            .expect("Error on recreate swapchain callback");
                    } else {
                        return;
                    }
                }

                is_swapchain_dirty = base_app
                    .draw(&window, app, &mut main_gui, &mut ui, &mut frame_stats, &controls)
                    .expect("Failed to tick");
            }
            // Keyboard
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state,
                                virtual_keycode: Some(key_code),
                                ..
                            },
                        ..
                    },
                ..
            } => {
                if key_code == VirtualKeyCode::R && state == ElementState::Pressed {
                    main_gui.toggle_stats();
                }
            }
            // Mouse
            Event::WindowEvent {
                event: WindowEvent::MouseInput { state, button, .. },
                ..
            } => {
                if button == MouseButton::Right {
                    if state == ElementState::Pressed {
                        window.set_cursor_visible(false);
                    } else {
                        window.set_cursor_visible(true);
                    }
                }
            }
            // Exit app on request to close window
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            // Wait for gpu to finish pending work before closing app
            Event::LoopDestroyed => base_app
                .wait_for_gpu()
                .expect("Failed to wait for gpu to finish work"),
            _ => (),
        }
    });
}

fn create_window(app_name: &str, width: u32, height: u32) -> (Window, EventLoop<()>) {
    log::info!("Creating window and event loop");
    let events_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(app_name)
        .with_inner_size(PhysicalSize::new(width, height))
        .with_resizable(true)
        .build(&events_loop)
        .unwrap();

    (window, events_loop)
}

impl<B: App> BaseApp<B> {
    fn new(
        window: &Window,
        app_name: &str,
        enable_raytracing: bool,
        enabled_compute_rendering: bool,
    ) -> Result<Self> {
        log::info!("Creating App");

        // Vulkan context
        let mut required_extensions = vec!["VK_KHR_swapchain"];
        if enable_raytracing {
            required_extensions.push("VK_KHR_ray_tracing_pipeline");
            required_extensions.push("VK_KHR_acceleration_structure");
            required_extensions.push("VK_KHR_deferred_host_operations");
        }

        #[cfg(debug_assertions)]
        required_extensions.push("VK_KHR_shader_non_semantic_info");

        let mut context = ContextBuilder::new(window, window)
            .vulkan_version(VERSION_1_3)
            .app_name(app_name)
            .required_extensions(&required_extensions)
            .required_device_features(DeviceFeatures {
                ray_tracing_pipeline: enable_raytracing,
                acceleration_structure: enable_raytracing,
                runtime_descriptor_array: enable_raytracing,
                buffer_device_address: enable_raytracing,
                dynamic_rendering: true,
                synchronization2: true,
            })
            .with_raytracing_context(enable_raytracing)
            .build()?;

        let command_pool = context.create_command_pool(
            context.graphics_queue_family,
            Some(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        )?;

        let swapchain = Swapchain::new(
            &context,
            window.inner_size().width,
            window.inner_size().height,
        )?;

        let storage_images = if enable_raytracing || enabled_compute_rendering {
            create_storage_images(
                &mut context,
                swapchain.format,
                swapchain.extent,
                swapchain.images.len(),
            )?
        } else {
            vec![]
        };

        let command_buffers = create_command_buffers(&command_pool, &swapchain)?;

        let in_flight_frames = InFlightFrames::new(&context, IN_FLIGHT_FRAMES)?;
        
        Ok(Self {
            phantom: PhantomData,
            raytracing_enabled: enable_raytracing,
            compute_rendering_enabled: enabled_compute_rendering,
            context,
            command_pool,
            swapchain,
            storage_images,
            command_buffers,
            in_flight_frames,
        })
    }

    fn recreate_swapchain(&mut self, width: u32, height: u32) -> Result<()> {
        log::debug!("Recreating the swapchain");

        self.wait_for_gpu()?;

        // Swapchain and dependent resources
        self.swapchain.resize(&self.context, width, height)?;

        if self.raytracing_enabled || self.compute_rendering_enabled {
            // Recreate storage image for RT and update descriptor set
            let storage_images = create_storage_images(
                &mut self.context,
                self.swapchain.format,
                self.swapchain.extent,
                self.swapchain.images.len(),
            )?;

            let _ = std::mem::replace(&mut self.storage_images, storage_images);
        }

        Ok(())
    }

    pub fn wait_for_gpu(&self) -> Result<()> {
        self.context.device_wait_idle()
    }

    fn draw(
        &mut self,
        window: &Window,
        base_app: &mut B,
        main_gui: &mut MainGui,
        gui: &mut B::Gui,
        frame_stats: &mut FrameStats,
        controls: &Controls,
    ) -> Result<bool> {
        // Drawing the frame
        self.in_flight_frames.next();
        self.in_flight_frames.fence().wait(None)?;

        // Can't get for gpu time on the first frames or vkGetQueryPoolResults gets stuck
        // due to VK_QUERY_RESULT_WAIT_BIT
        let gpu_time = (frame_stats.total_frame_count >= IN_FLIGHT_FRAMES)
            .then(|| self.in_flight_frames.gpu_frame_time_ms())
            .transpose()?
            .unwrap_or_default();
        frame_stats.set_gpu_time_time(gpu_time);
        frame_stats.tick();

        let next_image_result = self.swapchain.acquire_next_image(
            std::u64::MAX,
            self.in_flight_frames.image_available_semaphore(),
        );
        let image_index = match next_image_result {
            Ok(AcquiredImage { index, .. }) => index as usize,
            Err(err) => match err.downcast_ref::<vk::Result>() {
                Some(&vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
                _ => panic!("Error while acquiring next image. Cause: {}", err),
            },
        };
        self.in_flight_frames.fence().reset()?;

        base_app.update(self, gui, image_index, frame_stats.frame_time, controls)?;
        
        self.record_command_buffer(
            image_index,
            base_app,
            main_gui,
            gui,
            frame_stats,
            window,
        )?;

        self.context.graphics_queue.submit(
            &self.command_buffers[image_index],
            Some(SemaphoreSubmitInfo {
                semaphore: self.in_flight_frames.image_available_semaphore(),
                stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            }),
            Some(SemaphoreSubmitInfo {
                semaphore: self.in_flight_frames.render_finished_semaphore(),
                stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
            }),
            self.in_flight_frames.fence(),
        )?;

        let signal_semaphores = [self.in_flight_frames.render_finished_semaphore()];
        let present_result = self.swapchain.queue_present(
            image_index as _,
            &signal_semaphores,
            &self.context.present_queue,
        );
        match present_result {
            Ok(true) => return Ok(true),
            Err(err) => match err.downcast_ref::<vk::Result>() {
                Some(&vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
                _ => panic!("Failed to present queue. Cause: {}", err),
            },
            _ => {}
        }

        Ok(false)
    }
    
    fn record_command_buffer(
        &mut self,
        image_index: usize,
        base_app: &mut B,
        main_gui: &mut MainGui,
        gui: &mut B::Gui,
        frame_stats: &mut FrameStats,
        window: &Window,
    ) -> Result<()> {
        let swapchain_image = &self.swapchain.images[image_index];
        let swapchain_image_view = &self.swapchain.views[image_index];
        let buffer = &self.command_buffers[image_index];

        buffer.reset()?;

        buffer.begin(None)?;

        buffer.reset_all_timestamp_queries_from_pool(self.in_flight_frames.timing_query_pool());

        buffer.write_timestamp(
            vk::PipelineStageFlags2::NONE,
            self.in_flight_frames.timing_query_pool(),
            0,
        );

        if self.raytracing_enabled {
            base_app.record_raytracing_commands(self, buffer, image_index)?;
        }

        if self.compute_rendering_enabled {
            base_app.record_compute_commands(self, buffer, image_index)?;
        }

        if self.raytracing_enabled || self.compute_rendering_enabled {
            let storage_image = &self.storage_images[image_index].image;
            // Copy ray tracing result into swapchain
            buffer.pipeline_image_barriers(&[
                ImageBarrier {
                    image: swapchain_image,
                    old_layout: vk::ImageLayout::UNDEFINED,
                    new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    src_access_mask: vk::AccessFlags2::NONE,
                    dst_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                    src_stage_mask: vk::PipelineStageFlags2::NONE,
                    dst_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                },
                ImageBarrier {
                    image: storage_image,
                    old_layout: vk::ImageLayout::GENERAL,
                    new_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    src_access_mask: vk::AccessFlags2::SHADER_WRITE,
                    dst_access_mask: vk::AccessFlags2::TRANSFER_READ,
                    src_stage_mask: vk::PipelineStageFlags2::COMPUTE_SHADER
                        | vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
                    dst_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                },
            ]);

            buffer.copy_image(
                storage_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                swapchain_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );

            buffer.pipeline_image_barriers(&[
                ImageBarrier {
                    image: swapchain_image,
                    old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    src_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                    dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    src_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                    dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                },
                ImageBarrier {
                    image: storage_image,
                    old_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    new_layout: vk::ImageLayout::GENERAL,
                    src_access_mask: vk::AccessFlags2::TRANSFER_READ,
                    dst_access_mask: vk::AccessFlags2::NONE,
                    src_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                    dst_stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
                },
            ]);
        } else {
            buffer.pipeline_image_barriers(&[ImageBarrier {
                image: swapchain_image,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                src_access_mask: vk::AccessFlags2::NONE,
                dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                src_stage_mask: vk::PipelineStageFlags2::NONE,
                dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            }]);
        }

        // Rasterization
        base_app.record_raster_commands(self, buffer, image_index)?;

        // Main UI
        {
            buffer.begin_rendering(
                swapchain_image_view,
                None,
                self.swapchain.extent,
                vk::AttachmentLoadOp::DONT_CARE,
                None,
            );
            
            
            
            buffer.end_rendering();
        }
        
        buffer.pipeline_image_barriers(&[ImageBarrier {
            image: swapchain_image,
            old_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            new_layout: vk::ImageLayout::PRESENT_SRC_KHR,
            src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_READ,
            src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        }]);

        buffer.write_timestamp(
            vk::PipelineStageFlags2::ALL_COMMANDS,
            self.in_flight_frames.timing_query_pool(),
            1,
        );

        buffer.end()?;

        Ok(())
    }
}

fn create_storage_images(
    context: &mut Context,
    format: vk::Format,
    extent: vk::Extent2D,
    count: usize,
) -> Result<Vec<ImageAndView>> {
    let mut images = Vec::with_capacity(count);

    for _ in 0..count {
        let image = context.create_image(
            vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::STORAGE,
            MemoryLocation::GpuOnly,
            format,
            extent.width,
            extent.height,
        )?;

        let view = image.create_image_view(false)?;

        context.execute_one_time_commands(|cmd_buffer| {
            cmd_buffer.pipeline_image_barriers(&[ImageBarrier {
                image: &image,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::GENERAL,
                src_access_mask: vk::AccessFlags2::NONE,
                dst_access_mask: vk::AccessFlags2::NONE,
                src_stage_mask: vk::PipelineStageFlags2::NONE,
                dst_stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
            }]);
        })?;

        images.push(ImageAndView { image, view })
    }

    Ok(images)
}

fn create_command_buffers(pool: &CommandPool, swapchain: &Swapchain) -> Result<Vec<CommandBuffer>> {
    pool.allocate_command_buffers(vk::CommandBufferLevel::PRIMARY, swapchain.images.len() as _)
}

pub struct ImageAndView {
    pub view: ImageView,
    pub image: Image,
}

struct InFlightFrames {
    per_frames: Vec<PerFrame>,
    current_frame: usize,
}

struct PerFrame {
    image_available_semaphore: Semaphore,
    render_finished_semaphore: Semaphore,
    fence: Fence,
    timing_query_pool: TimestampQueryPool<2>,
}

impl InFlightFrames {
    fn new(context: &Context, frame_count: u32) -> Result<Self> {
        let sync_objects = (0..frame_count)
            .map(|_i| {
                let image_available_semaphore = context.create_semaphore()?;
                let render_finished_semaphore = context.create_semaphore()?;
                let fence = context.create_fence(Some(vk::FenceCreateFlags::SIGNALED))?;

                let timing_query_pool = context.create_timestamp_query_pool()?;

                Ok(PerFrame {
                    image_available_semaphore,
                    render_finished_semaphore,
                    fence,
                    timing_query_pool,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            per_frames: sync_objects,
            current_frame: 0,
        })
    }

    fn next(&mut self) {
        self.current_frame = (self.current_frame + 1) % self.per_frames.len();
    }

    fn image_available_semaphore(&self) -> &Semaphore {
        &self.per_frames[self.current_frame].image_available_semaphore
    }

    fn render_finished_semaphore(&self) -> &Semaphore {
        &self.per_frames[self.current_frame].render_finished_semaphore
    }

    fn fence(&self) -> &Fence {
        &self.per_frames[self.current_frame].fence
    }

    fn timing_query_pool(&self) -> &TimestampQueryPool<2> {
        &self.per_frames[self.current_frame].timing_query_pool
    }

    fn gpu_frame_time_ms(&self) -> Result<Duration> {
        let result = self.timing_query_pool().wait_for_all_results()?;
        let time = Duration::from_nanos(result[1].saturating_sub(result[0]));

        Ok(time)
    }
}

#[derive(Debug)]
struct FrameStats {
    // we collect gpu timings the frame after it was computed
    // so we keep frame times for the two last frames
    previous_frame_time: Duration,
    frame_time: Duration,
    previous_compute_time: Duration,
    compute_time: Duration,
    gpu_time: Duration,
    frame_time_ms_log: Queue<f32>,
    compute_time_ms_log: Queue<f32>,
    gpu_time_ms_log: Queue<f32>,
    total_frame_count: u32,
    frame_count: u32,
    fps_counter: u32,
    timer: Duration,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            previous_frame_time: Default::default(),
            frame_time: Default::default(),
            previous_compute_time: Default::default(),
            compute_time: Default::default(),
            gpu_time: Default::default(),
            frame_time_ms_log: Queue::new(FrameStats::MAX_LOG_SIZE),
            compute_time_ms_log: Queue::new(FrameStats::MAX_LOG_SIZE),
            gpu_time_ms_log: Queue::new(FrameStats::MAX_LOG_SIZE),
            total_frame_count: Default::default(),
            frame_count: Default::default(),
            fps_counter: Default::default(),
            timer: Default::default(),
        }
    }
}

impl FrameStats {
    const ONE_SEC: Duration = Duration::from_secs(1);
    const MAX_LOG_SIZE: usize = 1000;

    fn tick(&mut self) {
        // push log
        self.frame_time_ms_log
            .push(self.previous_frame_time.as_millis() as _);
        self.compute_time_ms_log
            .push(self.previous_compute_time.as_millis() as _);
        self.gpu_time_ms_log.push(self.gpu_time.as_millis() as _);

        // increment counter
        self.total_frame_count += 1;
        self.frame_count += 1;
        self.timer += self.frame_time;

        // reset counter if a sec has passed
        if self.timer > FrameStats::ONE_SEC {
            self.fps_counter = self.frame_count;
            self.frame_count = 0;
            self.timer -= FrameStats::ONE_SEC;
        }
    }

    fn set_frame_time(&mut self, frame_time: Duration, compute_time: Duration) {
        self.previous_frame_time = self.frame_time;
        self.previous_compute_time = self.compute_time;

        self.frame_time = frame_time;
        self.compute_time = compute_time;
    }

    fn set_gpu_time_time(&mut self, gpu_time: Duration) {
        self.gpu_time = gpu_time;
    }
}

#[derive(Debug)]
struct Queue<T>(Vec<T>, usize);

impl<T> Queue<T> {
    fn new(max_size: usize) -> Self {
        Self(Vec::with_capacity(max_size), max_size)
    }

    fn push(&mut self, value: T) {
        if self.0.len() == self.1 {
            self.0.remove(0);
        }
        self.0.push(value);
    }
}
