use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::mem::size_of;
use std::ops::Mul;

use app::anyhow::{anyhow, bail, ensure, Result};
use app::glam::{ivec3, uvec3, IVec3, Mat3, Mat4, UVec3, Vec3};
use app::log;
use app::vulkan::ash::vk::ExtTransformFeedbackFn;
use dot_vox::Color;
use serde_json::Value;

use crate::pattern_config::{BlockConfig, Config};
use crate::voxel_loader;
use crate::{rotation::Rot, voxel_loader::VoxelLoader};

pub type NodeIndex = usize;
pub type BlockIndex = usize;
pub type Voxel = u8;

pub const NODE_INDEX_NONE: NodeIndex = NodeIndex::MAX;
pub const BLOCK_INDEX_NONE: BlockIndex = BlockIndex::MAX;
pub const VOXEL_EMPTY: Voxel = 0;

pub const NODE_SIZE: UVec3 = uvec3(4, 4, 4);
pub const NODE_VOXEL_LENGTH: usize = (NODE_SIZE.x * NODE_SIZE.y * NODE_SIZE.z) as usize;

#[derive(Clone, Debug)]
pub struct NodeController {
    pub nodes: Vec<Node>,
    pub mats: [Material; 256],
    pub patterns: [Vec<Pattern>; 256],
    pub blocks: Vec<Block>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Node {
    voxels: [Voxel; NODE_VOXEL_LENGTH],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct NodeID {
    pub index: NodeIndex,
    pub rot: Rot,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Material {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Block {
    pub name: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Pattern {
    pub prio: usize,
    pub block_config: BlockConfig,
    pub nodes: [NodeID; 8],
    pub req: HashMap<IVec3, Vec<NodeIndex>>,
}

impl NodeController {
    pub fn new(voxel_loader: VoxelLoader) -> Result<NodeController> {
        let patterns = Self::make_patterns(&voxel_loader)?;

        Ok(NodeController {
            nodes: voxel_loader.nodes,
            mats: voxel_loader.mats,
            blocks: voxel_loader.blocks,
            patterns: patterns,
        })
    }

    fn make_patterns(voxel_loader: &VoxelLoader) -> Result<[Vec<Pattern>; 256]> {
        let mut patterns = core::array::from_fn(|_| Vec::new());

        for pattern in voxel_loader.patterns.iter() {
            if pattern.block_config
                == BlockConfig::from([
                    1,
                    NODE_INDEX_NONE,
                    NODE_INDEX_NONE,
                    NODE_INDEX_NONE,
                    NODE_INDEX_NONE,
                    NODE_INDEX_NONE,
                    NODE_INDEX_NONE,
                    NODE_INDEX_NONE,
                ])
            {
                log::info!("Test")
            }

            let possibilities = pattern.block_config.get_possibilities(pattern.nodes);

            for (bc, nodes) in possibilities.into_iter() {
                let config: Config = bc.into();
                let index: usize = config.into();

                let new_pattern = Pattern::new(bc, nodes, HashMap::new(), 0);
                if patterns[index].contains(&new_pattern) {
                    continue;
                }

                patterns[index].push(new_pattern);
            }
        }

        Ok(patterns)
    }
}

impl Node {
    pub fn new(voxels: [Voxel; NODE_VOXEL_LENGTH]) -> Self {
        Node { voxels }
    }
}

impl NodeID {
    pub fn new(index: NodeIndex, rot: Rot) -> NodeID {
        NodeID { index, rot }
    }

    pub fn none() -> NodeID {
        NodeID::default()
    }

    pub fn is_none(self) -> bool {
        self.index == NODE_INDEX_NONE
    }

    pub fn is_some(self) -> bool {
        self.index != NODE_INDEX_NONE
    }
}

impl Default for NodeID {
    fn default() -> Self {
        Self {
            index: NODE_INDEX_NONE,
            rot: Default::default(),
        }
    }
}

impl Into<u32> for NodeID {
    fn into(self) -> u32 {
        if self.is_none() {
            log::warn!("None Node Id was converted!");
            0
        } else {
            ((self.index as u32) << 7) + <Rot as Into<u8>>::into(self.rot) as u32
        }
    }
}

impl From<NodeIndex> for NodeID {
    fn from(value: NodeIndex) -> Self {
        NodeID::new(value, Rot::default())
    }
}

impl From<Material> for [u8; 4] {
    fn from(color: Material) -> Self {
        [color.r, color.g, color.b, color.a]
    }
}
impl From<&Material> for [u8; 4] {
    fn from(color: &Material) -> Self {
        [color.r, color.g, color.b, color.a]
    }
}
impl From<Color> for Material {
    fn from(value: Color) -> Self {
        Material {
            r: value.r,
            g: value.g,
            b: value.b,
            a: value.a,
        }
    }
}
impl From<&Color> for Material {
    fn from(value: &Color) -> Self {
        Material {
            r: value.r,
            g: value.g,
            b: value.b,
            a: value.a,
        }
    }
}

impl Block {
    pub fn new(name: String) -> Self {
        Block { name }
    }
}

impl Pattern {
    pub fn new(
        block_config: BlockConfig,
        nodes: [NodeID; 8],
        req: HashMap<IVec3, Vec<NodeIndex>>,
        prio: usize,
    ) -> Self {
        Pattern {
            block_config,
            nodes,
            req: req,
            prio: prio,
        }
    }
}
