use std::time::Duration;

use app::anyhow::{ensure, Ok, Result};
use app::camera::Camera;
use app::controls::Controls;
use app::glam::Vec3;
use app::imgui::{Condition, Ui};
use app::vulkan::ash::vk::{self};
use app::vulkan::{CommandBuffer, WriteDescriptorSet, WriteDescriptorSetKind};
use app::{log, App, BaseApp};

mod octtree_controller;
use octtree::streamed_octtree::StreamedOcttree;
use octtree_controller::*;
mod octtree_builder;
use octtree_builder::*;
mod octtree_loader;
use octtree_loader::*;
mod materials;
use materials::*;
mod renderer;
use renderer::*;

mod debug;
use debug::*;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 576;
const APP_NAME: &str = "Ray Caster";

const PRINT_DEBUG_LOADING: bool = false;
const MOVEMENT_DEBUG_READ: bool = false;
const SAVE_FOLDER: &str = "../../libs/octtree/assets/octtree/";

fn main() -> Result<()> {
    ensure!(cfg!(target_pointer_width = "64"), "Target not 64 bit");

    app::run::<RayCaster>(APP_NAME, WIDTH, HEIGHT, true, true)?;
    Ok(())
}

#[allow(dead_code)]
pub struct RayCaster {
    total_time: Duration,
    frame_counter: usize,

    octtree_controller: OcttreeController<StreamedOcttree>,
    material_controller: MaterialController,
    renderer: Renderer,
    builder: OcttreeBuilder,
    loader: OcttreeLoader,

    movement_debug: MovementDebug,

    max_loaded_batches: usize,

    camera: Camera,
}

impl App for RayCaster {
    type Gui = Gui;

    fn new(base: &mut BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        let images = &base.swapchain.images;
        let images_len = images.len() as u32;

        log::info!("Creating Octtree");

        let max_loaded_batches = 100;
        let octtree = StreamedOcttree::new(SAVE_FOLDER, max_loaded_batches)?;
        let octtree_controller = OcttreeController::new(context, octtree, 50000, 1000, 10000)?;

        log::info!("Creating Materials");
        let material_controller = MaterialController::new(MaterialList::default(), context)?;

        log::info!("Creating Loader");
        let loader = OcttreeLoader::new(
            context,
            &octtree_controller,
            &octtree_controller.octtree_buffer,
            &octtree_controller.octtree_info_buffer,
        )?;

        log::info!("Creating Renderer");
        let renderer = Renderer::new(
            context,
            images_len,
            &base.storage_images,
            &octtree_controller.octtree_buffer,
            &octtree_controller.octtree_info_buffer,
            &material_controller.material_buffer,
        )?;

        log::info!("Creating Builder");
        let builder = OcttreeBuilder::new(
            context,
            &octtree_controller.octtree_buffer,
            &octtree_controller.octtree_info_buffer,
            octtree_controller.buffer_size,
        )?;

        log::info!("Setting inital camera pos");
        let mut camera = Camera::base(base.swapchain.extent);
        camera.position = Vec3::new(-50.0, 0.0, 0.0);
        camera.direction = Vec3::new(1.0, 0.0, 0.0).normalize();
        camera.speed = 50.0;

        log::info!("Init done");
        Ok(Self {
            total_time: Duration::ZERO,
            frame_counter: 0,

            octtree_controller,
            material_controller,
            renderer,
            builder,
            loader,

            movement_debug: MovementDebug::new(MOVEMENT_DEBUG_READ)?,

            max_loaded_batches,

            camera,
        })
    }

    fn update(
        &mut self,
        base: &mut BaseApp<Self>,
        gui: &mut Self::Gui,
        _: usize,
        delta_time: Duration,
        controls: &Controls,
    ) -> Result<()> {
        log::info!("Frame: {:?}", &self.frame_counter);

        self.total_time += delta_time;

        self.camera.update(controls, delta_time);

        self.octtree_controller
            .octtree_info_buffer
            .copy_data_to_buffer(&[self.octtree_controller.octtree_info])?;
        self.renderer.ubo_buffer.copy_data_to_buffer(&[ComputeUbo {
            screen_size: [
                base.swapchain.extent.width as f32,
                base.swapchain.extent.height as f32,
            ],
            mode: gui.mode,
            debug_scale: gui.debug_scale,

            pos: self.camera.position,
            step_to_root: gui.step_to_root as u32,

            dir: self.camera.direction,
            fill_2: 0,
        }])?;

        self.builder.build_tree = gui.build || self.frame_counter == 0;
        self.loader.load_tree = gui.load && self.frame_counter != 0;

        if self.loader.load_tree {
            let mut request_data: Vec<u32> = self.loader.request_buffer.get_data_from_buffer(
                REQUEST_STEP * self.octtree_controller.transfer_size + LOAD_DEBUG_DATA_SIZE,
            )?;

            // Debug data from load shader
            let render_counter = request_data[self.octtree_controller.transfer_size] as usize;
            let needs_children_counter =
                request_data[self.octtree_controller.transfer_size + 1] as usize;

            gui.render_counter = render_counter;
            gui.needs_children_counter = needs_children_counter;

            request_data.truncate(REQUEST_STEP * self.octtree_controller.transfer_size);

            let (requested_nodes, transfer_counter) =
                self.octtree_controller.get_requested_nodes(&request_data)?;
            self.loader
                .transfer_buffer
                .copy_data_to_buffer(&requested_nodes)?;

            gui.transfer_counter = transfer_counter;

            if PRINT_DEBUG_LOADING {
                log::debug!("Render Counter: {:?}", &render_counter);
                log::debug!("Needs Children Counter: {:?}", &needs_children_counter);
                log::debug!("Transfer Counter: {:?}", &transfer_counter);
                log::debug!("Request Data: {:?}", &request_data);
            }
        }

        // Updateing Gui
        gui.frame = self.frame_counter;
        gui.pos = self.camera.position;
        gui.dir = self.camera.direction;
        gui.octtree_buffer_size = self.octtree_controller.buffer_size;
        gui.transfer_buffer_size = self.octtree_controller.transfer_size;
        gui.loaded_batches = ((self.octtree_controller.octtree.get_loaded_size() * 100)
            / self.octtree_controller.octtree.get_loaded_max_size())
            as u32;

        self.octtree_controller.step();

        if MOVEMENT_DEBUG_READ {
            self.movement_debug
                .read(&mut self.camera, self.frame_counter)?;
        } else {
            self.movement_debug.write(&self.camera)?;
        }

        self.frame_counter += 1;

        Ok(())
    }

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
        if self.loader.load_tree {
            self.loader.render(base, buffer, image_index)?;
        }

        if self.builder.build_tree {
            self.builder.render(base, buffer, image_index)?;
        }

        self.renderer.render(base, buffer, image_index)?;

        Ok(())
    }

    fn on_recreate_swapchain(&mut self, base: &BaseApp<Self>) -> Result<()> {
        base.storage_images
            .iter()
            .enumerate()
            .for_each(|(index, img)| {
                let set = &self.renderer.descriptor_sets[index];

                set.update(&[WriteDescriptorSet {
                    binding: 0,
                    kind: WriteDescriptorSetKind::StorageImage {
                        layout: vk::ImageLayout::GENERAL,
                        view: &img.view,
                    },
                }]);
            });

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Gui {
    frame: usize,
    pos: Vec3,
    dir: Vec3,
    mode: u32,
    build: bool,
    load: bool,
    debug_scale: u32,

    render_counter: usize,
    needs_children_counter: usize,
    octtree_buffer_size: usize,

    transfer_counter: usize,
    transfer_buffer_size: usize,

    step_to_root: bool,

    loaded_batches: u32,
}

impl app::gui::Gui for Gui {
    fn new() -> Result<Self> {
        Ok(Gui {
            frame: 0,
            pos: Vec3::default(),
            dir: Vec3::default(),
            mode: 1,
            build: false,
            load: true,
            debug_scale: 1,

            render_counter: 0,
            needs_children_counter: 0,
            octtree_buffer_size: 0,
            transfer_counter: 0,
            transfer_buffer_size: 0,

            step_to_root: true,

            loaded_batches: 0,
        })
    }

    fn build(&mut self, ui: &Ui) {
        ui.window("Ray caster")
            .position([5.0, 150.0], Condition::FirstUseEver)
            .size([300.0, 300.0], Condition::FirstUseEver)
            .resizable(false)
            .movable(false)
            .build(|| {
                let frame = self.frame;
                ui.text(format!("Frame: {frame}"));

                let pos = self.pos;
                ui.text(format!("Pos: {pos}"));

                let dir = self.dir;
                ui.text(format!("Dir: {dir}"));

                let mut mode = self.mode as i32;
                ui.input_int("Mode", &mut mode).build();
                mode = mode.clamp(0, 4);
                self.mode = mode as u32;

                let mut debug_scale = self.debug_scale as i32;
                ui.input_int("Scale", &mut debug_scale).build();
                debug_scale = debug_scale.clamp(1, 100);
                self.debug_scale = debug_scale as u32;

                let mut build = self.build;
                if ui.radio_button_bool("Build Tree", build) {
                    build = !build;
                }
                self.build = build;

                let mut load = self.load;
                if ui.radio_button_bool("Load Tree", load) {
                    load = !load;
                }
                self.load = load;

                let render_counter = self.render_counter;
                let percent =
                    (self.render_counter as f32 / self.octtree_buffer_size as f32) * 100.0;
                ui.text(format!(
                    "Rendered Nodes: {render_counter} ({:.0}%)",
                    percent
                ));

                let needs_children = self.needs_children_counter;
                ui.text(format!("Needs Children: {needs_children}"));

                let transfer_counter = self.transfer_counter;
                let percent =
                    (self.transfer_counter as f32 / self.transfer_buffer_size as f32) * 100.0;
                ui.text(format!(
                    "Transfered Nodes: {transfer_counter} ({:.0}%)",
                    percent
                ));

                let mut step_to_root = self.step_to_root;
                if ui.radio_button_bool("Step to Root", step_to_root) {
                    step_to_root = !step_to_root;
                }
                self.step_to_root = step_to_root;

                let loaded_batches = self.loaded_batches;
                ui.text(format!("Loaded Batches: {loaded_batches}"));
            });
    }
}
