use octa_force::gui::InWorldGui;
use std::time::Duration;

use octa_force::glam::{ivec2, uvec2, IVec2, UVec3};
use octa_force::imgui::{Condition, Ui};
use octa_force::vulkan::{
    ash::vk::{self, Format},
    CommandBuffer,
};
use octa_force::{
    anyhow::Result,
    camera::Camera,
    controls::Controls,
    glam::{uvec3, vec3, Vec3},
};
use octa_force::{log, App, BaseApp};

#[cfg(debug_assertions)]
use crate::debug::{DebugController, DebugLineRenderer};

use crate::debug::DebugTextRenderer;
use crate::{
    builder::Builder, node::NodeController, ship::Ship, ship_renderer::ShipRenderer,
    voxel_loader::VoxelLoader,
};

pub mod builder;

#[cfg(debug_assertions)]
pub mod debug;
pub mod math;
pub mod node;
pub mod rotation;
pub mod ship;
pub mod ship_mesh;
pub mod ship_renderer;
pub mod voxel_loader;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 576;
const APP_NAME: &str = "Space ship builder";
const VOX_FILE_RELODE_INTERVALL: Duration = Duration::from_secs(1);
fn main() -> Result<()> {
    octa_force::run::<SpaceShipBuilder>(APP_NAME, uvec2(WIDTH, HEIGHT), false)
}
struct SpaceShipBuilder {
    total_time: Duration,
    last_vox_reloade: Duration,

    node_controller: NodeController,
    builder: Builder,
    renderer: ShipRenderer,

    #[cfg(debug_assertions)]
    debug_controller: DebugController,
    camera: Camera,
}

impl App for SpaceShipBuilder {
    fn new(base: &mut BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        //Rot::print_rot_permutations();

        let voxel_loader = VoxelLoader::new("./assets/models/space_ship_v3.vox")?;

        let node_controller =
            NodeController::new(voxel_loader, "./assets/models/space_ship_config_v3.json")?;

        let ship = Ship::new()?;

        let builder = Builder::new(ship, &node_controller, base.swapchain.images.len())?;

        let renderer = ShipRenderer::new(
            context,
            &node_controller,
            base.swapchain.images.len() as u32,
            base.swapchain.format,
            Format::D32_SFLOAT,
            base.swapchain.extent,
        )?;

        #[cfg(debug_assertions)]
        let debug_line_renderer = DebugLineRenderer::new(
            1000000,
            context,
            base.swapchain.images.len() as u32,
            base.swapchain.format,
            Format::D32_SFLOAT,
            &renderer,
        )?;

        #[cfg(debug_assertions)]
        let debug_gui_id = base.add_in_world_gui()?;

        let debug_text_renderer = DebugTextRenderer::new(debug_gui_id);

        #[cfg(debug_assertions)]
        let debug_controller = DebugController::new(debug_line_renderer, debug_text_renderer)?;

        log::info!("Creating Camera");
        let mut camera = Camera::base(base.swapchain.extent);

        camera.position = Vec3::new(1.0, -2.0, 1.0);
        camera.direction = Vec3::new(0.0, 1.0, 0.0).normalize();
        camera.speed = 2.0;
        camera.z_far = 100.0;
        camera.up = vec3(0.0, 0.0, 1.0);

        Ok(Self {
            total_time: Duration::ZERO,
            last_vox_reloade: Duration::ZERO,

            node_controller,
            builder,
            renderer,

            #[cfg(debug_assertions)]
            debug_controller,

            camera,
        })
    }

    fn update(
        &mut self,
        base: &mut BaseApp<Self>,
        image_index: usize,
        delta_time: Duration,
    ) -> Result<()> {
        self.total_time += delta_time;

        self.camera.update(&base.controls, delta_time);

        if base.controls.q && self.last_vox_reloade + VOX_FILE_RELODE_INTERVALL < self.total_time {
            self.last_vox_reloade = self.total_time;

            log::info!("reloading .vox File");
            let voxel_loader = VoxelLoader::new("./assets/models/space_ship_v3.vox")?;
            self.node_controller.load(voxel_loader)?;

            self.builder
                .on_node_controller_change(&self.node_controller)?;

            self.renderer = ShipRenderer::new(
                &base.context,
                &self.node_controller,
                base.swapchain.images.len() as u32,
                base.swapchain.format,
                Format::D32_SFLOAT,
                base.swapchain.extent,
            )?;
            log::info!(".vox File loaded");
        }

        self.builder.update(
            image_index,
            &base.context,
            &self.renderer.chunk_descriptor_layout,
            &self.renderer.descriptor_pool,
            &base.controls,
            &self.camera,
            &self.node_controller,
            delta_time,
            self.total_time,
            #[cfg(debug_assertions)]
            &mut self.debug_controller,
        )?;

        self.renderer.update(&self.camera, base.swapchain.extent)?;

        #[cfg(debug_assertions)]
        {
            self.debug_controller.update(
                &base.controls,
                self.total_time,
                &mut base.in_world_guis[self.debug_controller.text_renderer.gui_id],
            )?;
        }

        Ok(())
    }

    fn record_render_commands(
        &mut self,
        base: &mut BaseApp<Self>,
        image_index: usize,
    ) -> Result<()> {
        let buffer = &base.command_buffers[image_index];

        buffer.swapchain_image_render_barrier(&base.swapchain.images[image_index])?;
        buffer.begin_rendering(
            &base.swapchain.views[image_index],
            Some(&self.renderer.depth_image_view),
            base.swapchain.extent,
            vk::AttachmentLoadOp::CLEAR,
            None,
        );
        buffer.set_viewport(base.swapchain.extent);
        buffer.set_scissor(base.swapchain.extent);

        self.renderer.render(buffer, image_index, &self.builder);

        #[cfg(debug_assertions)]
        self.debug_controller.render(
            buffer,
            image_index,
            &self.camera,
            base.swapchain.extent,
            &mut base.in_world_guis[self.debug_controller.text_renderer.gui_id],
        )?;

        buffer.end_rendering();

        Ok(())
    }

    fn on_recreate_swapchain(&mut self, base: &BaseApp<Self>) -> Result<()> {
        self.renderer
            .on_recreate_swapchain(&base.context, base.swapchain.extent)?;

        Ok(())
    }
}
