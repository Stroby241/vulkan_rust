use std::{time::{Instant}, sync::mpsc::Sender};

use app::glam::{vec2, ivec2, IVec2, Vec2};
use app::anyhow::*;
use rapier2d::prelude::*;

use crate::{math::{*, transform::Transform}, settings::Settings, render::part::RenderParticle};

use super::{part::{ChunkPart, PartIdCounter}, particle::Particle, CHUNK_PART_SIZE, ChunkController};


#[derive(Clone)]
pub struct Chunk { 
    pub parts: Vec<ChunkPart>, 

    pub transform: Transform,
    pub center_of_mass: Vec2,

    pub particle_counter: usize,
    particle_pos_sum: Vec2,

    pub break_cool_down: Instant,


    pub rb_handle: RigidBodyHandle,
    pub forces: Vec2,
    pub mass: f32,

    
}

#[allow(dead_code)]
impl Chunk {
    pub fn new(
        transform: Transform, 
        velocity_transform: Transform, 
        particles: Vec<(Particle, IVec2)>, 
        part_id_counter: &mut PartIdCounter,
        new_spawn: bool,

        settings: Settings,
    ) -> Self {
        let mut chunk = Self { 
            parts: Vec::new(),
    
            transform,
            center_of_mass: Vec2::ZERO,

            particle_counter: 0,
            particle_pos_sum: Vec2::ZERO,

            break_cool_down: if new_spawn { 
                Instant::now() - settings.destruction_cool_down 
            } else { 
                Instant::now() 
            },

            rb_handle: RigidBodyHandle::default(),

            forces: Vec2::ZERO,
            mass: 0.0,
        };

        for (p, hex_pos) in particles {
            chunk.add_particle(p, hex_pos, part_id_counter)
        }

        if !new_spawn {
            chunk.transform.pos += chunk.center_of_mass - vec2(0.75, 0.5);
        }

        chunk.on_chunk_change();

        chunk
    }

    pub fn add_particle(
        &mut self, 
        p: Particle, 
        hex_pos: IVec2,
        part_id_counter: &mut PartIdCounter,
    ) {
        let part_pos = hex_to_chunk_part_pos(hex_pos);
        let mut part = self.get_part_by_pos_mut(part_pos);

        if part.is_none() {
            let part_id = part_id_counter.pop_free();
            if part_id.is_none() {
                println!("Part Id Counter maxed out!!!");
                return;
            }

            let new_part= ChunkPart::new(part_pos, part_id.unwrap());

            self.parts.push(new_part);

            part = self.parts.last_mut();
        }
        debug_assert!(part.is_some());

        let in_part_pos = hex_to_particle_index(hex_pos);
        part.unwrap().particles[in_part_pos] = p;

        self.mass += p.mass as f32;

        // Needed for center of mass
        let particle_pos = hex_to_coord(hex_pos);
        self.particle_pos_sum += particle_pos;

        self.particle_counter += 1;
    }


    pub fn on_chunk_change(&mut self) {
        self.center_of_mass = self.particle_pos_sum / Vec2::new(
            self.particle_counter as f32, 
            self.particle_counter as f32);

        self.update_part_tranforms();
    }

    pub fn update_part_tranforms(&mut self){
        for part in self.parts.iter_mut() {
            part.transform = part_pos_to_world(self.transform, part.pos, self.center_of_mass);
        }
    }

    
    pub fn send(&self, 
        to_render_transform: &Sender<(usize, Transform)>,
        to_render_particles: &Sender<(usize, [RenderParticle; (CHUNK_PART_SIZE * CHUNK_PART_SIZE) as usize])>
    ) -> Result<()>{
        for part in self.parts.iter() {
            to_render_transform.send((part.id, part.transform))?;

            let mut particles = [RenderParticle::default(); (CHUNK_PART_SIZE * CHUNK_PART_SIZE) as usize];
            for (i, particle) in part.particles.iter().enumerate() {
                particles[i] = particle.into();
            }

            to_render_particles.send((part.id, particles))?;
        }

        Ok(())
    }

    pub fn send_transform(&self, 
        to_render_transform: &Sender<(usize, Transform)>,
    ) -> Result<()>{
        for part in self.parts.iter() {
            to_render_transform.send((part.id, part.transform))?;
        }

        Ok(())
    }




    pub fn get_part_by_pos(&self, pos: IVec2) -> Option<&ChunkPart>{
        for p in &self.parts {
            if p.pos == pos  {
                return Some(p);
            }
        }

        return None;
    }

    pub fn get_part_index_by_pos(&self, pos: IVec2) -> Option<usize>{
        for (i, p) in self.parts.iter().enumerate() {
            if p.pos == pos  {
                return Some(i);
            }
        }

        return None;
    }

    pub fn get_part_by_pos_mut(&mut self, pos: IVec2) -> Option<&mut ChunkPart>{
        for p in &mut self.parts {
            if p.pos == pos {
                return Some(p);
            }
        }

        return None;
    }

    pub fn get_neigbor_particles_pos(&self, part_pos: IVec2, part_index: usize, hex: IVec2) -> Vec<Option<(IVec2, usize)>> {

        let mut neigbor_particles = Vec::new();

        for i in 0..6 {
            let res = self.get_neigbor_particle_pos(part_pos, part_index, hex, i);

            neigbor_particles.push(res);
        }

        neigbor_particles
    }

    pub fn get_neigbor_particles_pos_cleaned(&self, part_pos: IVec2, part_index: usize, hex: IVec2) -> Vec<(IVec2, usize)> {

        let mut neigbor_particles = Vec::new();

        for i in 0..6 {
            let res = self.get_neigbor_particle_pos(part_pos, part_index, hex, i);

            if res.is_none() {
                continue;
            }
            neigbor_particles.push(res.unwrap());
        }

        neigbor_particles
    }


    pub fn get_neigbor_particle_pos(&self, part_pos: IVec2, part_index: usize, hex: IVec2, neigbor_index: usize) -> Option<(IVec2, usize)> {

        let mut hex_neigbor = hex + neigbor_hex_offsets()[neigbor_index];
        let neigbor_part_index = match neigbor_index {
            0 => {
                if hex_neigbor.x < CHUNK_PART_SIZE { Some(part_index) }
                else { 
                    hex_neigbor.x -= CHUNK_PART_SIZE;
                    self.get_part_index_by_pos(part_pos + ivec2(1, 0)) 
                }
            },
            1 => {
                if hex_neigbor.y < CHUNK_PART_SIZE { Some(part_index) }
                else {
                    hex_neigbor.y -= CHUNK_PART_SIZE;
                    self.get_part_index_by_pos(part_pos + ivec2(0, 1)) 
                }
            },
            2 => {

                if hex_neigbor.x >= 0 && hex_neigbor.y < CHUNK_PART_SIZE { 
                    Some(part_index) 
                }
                else { 
                    let mut new_pos = part_pos;
                    if hex_neigbor.x < 0{ 
                        hex_neigbor.x += CHUNK_PART_SIZE;
                        new_pos += ivec2(-1, 0);
                    }
                    if hex_neigbor.y >= CHUNK_PART_SIZE {
                        hex_neigbor.y -= CHUNK_PART_SIZE;
                        new_pos += ivec2(0, 1);
                    }

                    self.get_part_index_by_pos(new_pos) 
                }
            },
            3 => {
                if hex_neigbor.x >= 0 { Some(part_index) }
                else { 
                    hex_neigbor.x += CHUNK_PART_SIZE;
                    self.get_part_index_by_pos(part_pos + ivec2(-1, 0)) 
                }
            },
            4 => {
                if hex_neigbor.y >= 0 { Some(part_index) }
                else { 
                    hex_neigbor.y += CHUNK_PART_SIZE;
                    self.get_part_index_by_pos(part_pos + ivec2(0, -1)) 
                }
            },
            5 => {
                if hex_neigbor.x < CHUNK_PART_SIZE && hex_neigbor.y >= 0 { 
                    Some(part_index) 
                }
                else { 
                    let mut new_pos = part_pos;
                    if  hex_neigbor.x >= CHUNK_PART_SIZE { 
                        hex_neigbor.x -= CHUNK_PART_SIZE;
                        new_pos += ivec2(1, 0);
                    }
                    if hex_neigbor.y < 0 {
                        hex_neigbor.y += CHUNK_PART_SIZE;
                        new_pos += ivec2(0, -1);
                    }

                    self.get_part_index_by_pos(new_pos) 
                }
            }
            _ => { None }
        };

        if neigbor_part_index.is_none() || self.parts[neigbor_part_index.unwrap()].particles[hex_to_particle_index(hex_neigbor)].mass == 0 {
            return None;
        }

        return Some((hex_neigbor, neigbor_part_index.unwrap()))
    }

}

impl ChunkController {
    pub fn remove_chunk(&mut self, chunk_index: usize) {
        let chunk = &self.chunks[chunk_index];
        for part in chunk.parts.iter() {
            self.part_id_counter.add_free(part.id);
        }

        self.chunks.remove(chunk_index);
    }
}




