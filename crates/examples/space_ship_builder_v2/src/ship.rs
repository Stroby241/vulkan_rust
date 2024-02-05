use std::array::IntoIter;
use std::collections::VecDeque;
use std::iter::Filter;
use std::iter::Map;
use std::time::Duration;

use app::anyhow::*;
use app::glam::*;
use app::log;
use app::vulkan::Context;

use crate::math::to_1d;
use crate::math::to_1d_i;
use crate::node;
use crate::node::BlockIndex;
use crate::node::NodeController;
use crate::node::NodeID;
use crate::node::Pattern;
use crate::node::BLOCK_INDEX_NONE;
use crate::node::NODE_INDEX_NONE;
use crate::pattern_config::Config;
use crate::ship_mesh::ShipMesh;

pub type WaveIndex = usize;

pub const MIN_TICK_LENGTH: Duration = Duration::from_millis(20);
pub const MAX_TICK_LENGTH: Duration = Duration::from_millis(25);

pub struct Ship {
    pub block_size: UVec3,
    pub wave_size: UVec3,

    pub blocks: Vec<BlockIndex>,
    pub wave: Vec<Wave>,
    pub to_propergate: VecDeque<UVec3>,

    pub actions_per_tick: usize,
    pub full_tick: bool,

    pub mesh: ShipMesh,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Wave {
    pub possible_pattern: Vec<Pattern>,
    pub all_possible_pattern: Vec<Pattern>,
}

impl Ship {
    pub fn new(context: &Context, node_controller: &NodeController) -> Result<Ship> {
        let block_size = uvec3(10, 10, 10);
        let wave_size = block_size + uvec3(1, 1, 1);

        let max_block_index = (block_size.x * block_size.y * block_size.z) as usize;
        let max_wave_index = (wave_size.x * wave_size.y * wave_size.z) as usize;
        let mesh = ShipMesh::new(context, max_wave_index + 1)?;

        let mut ship = Ship {
            block_size,
            wave_size,
            blocks: vec![BLOCK_INDEX_NONE; max_block_index],
            wave: vec![Wave::new(node_controller); max_wave_index],
            to_propergate: VecDeque::new(),
            actions_per_tick: 4,
            full_tick: false,

            mesh,
        };

        ship.place_block(uvec3(5, 5, 5), 0, node_controller)?;
        //ship.fill_all(0, node_controller)?;

        Ok(ship)
    }

    fn pos_in_bounds(pos: IVec3, size: UVec3) -> bool {
        pos.cmpge(IVec3::ZERO).all() && pos.cmplt(size.as_ivec3()).all()
    }

    pub fn get_block(&self, pos: UVec3) -> Result<BlockIndex> {
        self.get_block_i(pos.as_ivec3())
    }

    pub fn get_block_i(&self, pos: IVec3) -> Result<BlockIndex> {
        if !Self::pos_in_bounds(pos, self.block_size) {
            bail!("Pos not in ship")
        }

        let index = to_1d_i(pos, self.block_size.as_ivec3());
        Ok(self.blocks[index as usize])
    }

    pub fn fill_all(
        &mut self,
        block_index: BlockIndex,
        node_controller: &NodeController,
    ) -> Result<()> {
        for x in 0..self.block_size.x {
            for y in 0..self.block_size.y {
                for z in 0..self.block_size.z {
                    self.place_block(uvec3(x, y, z), block_index, node_controller)?;
                }
            }
        }

        Ok(())
    }

    pub fn place_block(
        &mut self,
        pos: UVec3,
        block_index: BlockIndex,
        node_controller: &NodeController,
    ) -> Result<()> {
        log::info!("Place: {pos:?}");

        let cell_index = to_1d(pos, self.block_size);
        self.blocks[cell_index] = block_index;

        self.update_wave(pos, node_controller);
        self.reset_wave();

        Ok(())
    }

    pub fn get_wave_poses_of_block_pos(pos: IVec3) -> impl Iterator<Item = UVec3> {
        [
            pos + ivec3(0, 0, 0),
            pos + ivec3(1, 0, 0),
            pos + ivec3(0, 1, 0),
            pos + ivec3(1, 1, 0),
            pos + ivec3(0, 0, 1),
            pos + ivec3(1, 0, 1),
            pos + ivec3(0, 1, 1),
            pos + ivec3(1, 1, 1),
        ]
        .into_iter()
        .map(|pos| pos.as_uvec3())
    }

    pub fn get_block_poses_of_wave_pos(
        &mut self,
        pos: IVec3,
    ) -> impl Iterator<Item = (usize, UVec3)> + '_ {
        [
            pos + ivec3(0, 0, 0),
            pos + ivec3(-1, 0, 0),
            pos + ivec3(0, -1, 0),
            pos + ivec3(-1, -1, 0),
            pos + ivec3(0, 0, -1),
            pos + ivec3(-1, 0, -1),
            pos + ivec3(0, -1, -1),
            pos + ivec3(-1, -1, -1),
        ]
        .into_iter()
        .enumerate()
        .filter(|(_, pos)| Self::pos_in_bounds(*pos, self.block_size))
        .map(|(i, pos)| (i, pos.as_uvec3()))
    }

    pub fn get_neigbor_poses_of_wave_pos(
        &mut self,
        pos: IVec3,
    ) -> impl Iterator<Item = UVec3> + '_ {
        [
            pos + ivec3(-1, -1, -1),
            pos + ivec3(0, -1, -1),
            pos + ivec3(1, -1, -1),
            pos + ivec3(-1, 0, -1),
            pos + ivec3(0, 0, -1),
            pos + ivec3(1, 0, -1),
            pos + ivec3(-1, 1, -1),
            pos + ivec3(0, 1, -1),
            pos + ivec3(1, 1, -1),
            pos + ivec3(-1, -1, 0),
            pos + ivec3(0, -1, 0),
            pos + ivec3(1, -1, 0),
            pos + ivec3(-1, 0, 0),
            pos + ivec3(1, 0, 0),
            pos + ivec3(-1, 1, 0),
            pos + ivec3(0, 1, 0),
            pos + ivec3(1, 1, 0),
            pos + ivec3(-1, -1, 1),
            pos + ivec3(0, -1, 1),
            pos + ivec3(1, -1, 1),
            pos + ivec3(-1, 0, 1),
            pos + ivec3(0, 0, 1),
            pos + ivec3(1, 0, 1),
            pos + ivec3(-1, 1, 1),
            pos + ivec3(0, 1, 1),
            pos + ivec3(1, 1, 1),
        ]
        .into_iter()
        .filter(|pos| Self::pos_in_bounds(*pos, self.wave_size))
        .map(|pos| pos.as_uvec3())
    }

    fn update_wave(&mut self, pos: UVec3, node_controller: &NodeController) {
        for wave_pos in Self::get_wave_poses_of_block_pos(pos.as_ivec3()) {
            let wave_index = to_1d(wave_pos, self.wave_size) as usize;

            let config = self.get_wave_config(wave_pos);
            let config_index: usize = config.into();

            self.wave[wave_index].all_possible_pattern =
                node_controller.pattern[config_index].to_owned();
            self.wave[wave_index].possible_pattern =
                node_controller.pattern[config_index].to_owned();

            self.to_propergate.push_back(wave_pos);
        }
    }

    fn reset_wave(&mut self) {
        for wave in self.wave.iter_mut() {
            wave.possible_pattern = wave.all_possible_pattern.to_owned();
        }
    }

    fn get_wave_config(&mut self, wave_pos: UVec3) -> Config {
        let mut config = [false; 8];
        let block_poses: Vec<_> = self
            .get_block_poses_of_wave_pos(wave_pos.as_ivec3())
            .collect();
        for (i, block_pos) in block_poses {
            let block_index = self.get_block(block_pos).unwrap();
            config[i] = block_index != BLOCK_INDEX_NONE;
        }
        config.into()
    }

    pub fn tick(&mut self, delta_time: Duration) -> Result<()> {
        if self.to_propergate.is_empty() {
            return Ok(());
        }

        if self.full_tick {
            if delta_time < MIN_TICK_LENGTH && self.actions_per_tick < usize::MAX / 2 {
                self.actions_per_tick *= 2;
            } else if delta_time > MAX_TICK_LENGTH && self.actions_per_tick > 4 {
                self.actions_per_tick /= 2;
            }
        }

        log::info!("Tick: {}", self.actions_per_tick);

        self.full_tick = false;
        for i in 0..self.actions_per_tick {
            if self.to_propergate.is_empty() {
                break;
            }

            let pos = self.to_propergate.pop_front().unwrap();
            self.propergate(pos);

            if i == 0 {
                self.full_tick = true;
            }
        }

        self.mesh.update(self.wave_size, &self.wave)?;

        Ok(())
    }

    fn propergate(&mut self, pos: UVec3) {
        let wave_index = to_1d(pos, self.wave_size);
        let mut wave = self.wave[wave_index].to_owned();

        let mut changed = false;
        for i in (0..wave.possible_pattern.len()).rev() {
            let pattern = &wave.possible_pattern[i];
            if pattern.req.is_empty() {
                continue;
            }

            for (offset, node_id) in pattern.req.iter() {
                let req_pos = pos + *offset;
                let req_index = to_1d(req_pos, self.wave_size);

                if !(node_id.contains(&self.wave[req_index].possible_pattern[0].id)) {
                    wave.possible_pattern.remove(i);
                    changed = true;
                    break;
                }
            }
        }

        if changed {
            self.wave[wave_index] = wave;

            let mut neigbors = self.get_neigbor_poses_of_wave_pos(pos.as_ivec3()).collect();
            self.to_propergate.append(&mut neigbors)
        }
    }
}

impl Wave {
    pub fn new(node_controller: &NodeController) -> Self {
        Self {
            possible_pattern: node_controller.pattern[0].to_owned(),
            all_possible_pattern: node_controller.pattern[0].to_owned(),
        }
    }
}
