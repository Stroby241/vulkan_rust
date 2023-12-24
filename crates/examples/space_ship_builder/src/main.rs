use std::time::Duration;

use app::anyhow::Result;
use app::camera::Camera;
use app::controls::Controls;
use app::glam::Vec3;
use app::vulkan::ash::vk::{self, Format};
use app::vulkan::CommandBuffer;
use app::{log, App, BaseApp};
use mesh::Mesh;
use renderer::{RenderBuffer, Renderer};
use ship::SHIP_TICK_LENGTH;

use crate::rule::RuleSet;
use crate::ship::Ship;

pub mod math;
pub mod mesh;
pub mod renderer;
pub mod rule;
pub mod ship;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 576;
const APP_NAME: &str = "Space ship builder";

fn main() -> Result<()> {
    app::run::<SpaceShipBuilder>(APP_NAME, WIDTH, HEIGHT, false, false)
}
struct SpaceShipBuilder {
    total_time: Duration,
    last_ship_tick: Duration,

    ruleset: RuleSet,
    ship: Ship,
    mesh: Mesh,
    renderer: Renderer,
    camera: Camera,
}

impl App for SpaceShipBuilder {
    type Gui = ();

    fn new(base: &mut BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        fastrand::seed(42);

        let ruleset = RuleSet::new();
        let ship = Ship::new(&ruleset)?;

        let mesh = Mesh::from_ship(context, &ship)?;

        let renderer = Renderer::new(
            context,
            base.swapchain.images.len() as u32,
            base.swapchain.format,
            Format::D32_SFLOAT,
            base.swapchain.extent,
        )?;

        log::info!("Creating Camera");
        let mut camera = Camera::base(base.swapchain.extent);

        camera.position = Vec3::new(5.0, -5.0, -5.0);
        camera.direction = Vec3::new(0.0, 0.0, 1.0).normalize();
        camera.speed = 2.0;
        camera.z_far = 100.0;

        Ok(Self {
            total_time: Duration::ZERO,
            last_ship_tick: Duration::ZERO,

            ruleset,
            ship,
            mesh,
            renderer,
            camera,
        })
    }

    fn on_recreate_swapchain(&mut self, _: &BaseApp<Self>) -> Result<()> {
        Ok(())
    }

    fn update(
        &mut self,
        _: &mut BaseApp<Self>,
        _: &mut <Self as App>::Gui,
        _: usize,
        delta_time: Duration,
        controls: &Controls,
    ) -> Result<()> {
        self.total_time += delta_time;

        self.camera.update(controls, delta_time);

        self.renderer
            .render_buffer
            .copy_data_to_buffer(&[RenderBuffer {
                view_proj_matrix: self.camera.projection_matrix() * self.camera.view_matrix(),
            }])?;

        if self.last_ship_tick + SHIP_TICK_LENGTH < self.total_time {
            self.last_ship_tick = self.total_time;

            self.ship.tick(&self.ruleset)?;
            self.mesh.update(&self.ship)?;
        }

        Ok(())
    }

    fn record_raster_commands(
        &self,
        base: &BaseApp<Self>,
        buffer: &CommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        buffer.begin_rendering(
            &base.swapchain.views[image_index],
            Some(&self.renderer.depth_image_view),
            base.swapchain.extent,
            vk::AttachmentLoadOp::CLEAR,
            None,
        );
        buffer.bind_graphics_pipeline(&self.renderer.pipeline);

        buffer.bind_vertex_buffer(&self.mesh.vertex_buffer);
        buffer.bind_index_buffer(&self.mesh.index_buffer);

        buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::GRAPHICS,
            &self.renderer.pipeline_layout,
            0,
            &[&self.renderer.descriptor_sets[image_index]],
        );

        buffer.set_viewport(base.swapchain.extent);
        buffer.set_scissor(base.swapchain.extent);

        buffer.draw_indexed(self.mesh.indecies.len() as u32);
        buffer.end_rendering();

        Ok(())
    }
}