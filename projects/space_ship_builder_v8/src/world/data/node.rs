use dot_vox::Color;
use octa_force::glam::{ivec3, uvec3, IVec3, Mat4, UVec3};

use crate::math::rotation::Rot;
use crate::math::{to_1d, to_1d_i, to_3d, to_3d_i};
use octa_force::glam::Mat3;
use std::hash::Hash;
use std::iter::repeat;

pub type NodeIndex = usize;
pub type Voxel = u8;

pub const VOXEL_EMPTY: Voxel = 0;
pub const VOXEL_PER_NODE_SIDE: i32 = 4;
pub const NODE_SIZE: IVec3 = ivec3(
    VOXEL_PER_NODE_SIDE,
    VOXEL_PER_NODE_SIDE,
    VOXEL_PER_NODE_SIDE,
);
pub const NODE_VOXEL_LENGTH: usize = (NODE_SIZE.x * NODE_SIZE.y * NODE_SIZE.z) as usize;

pub const NODE_INDEX_EMPTY: NodeIndex = 0;
pub const NODE_INDEX_ANY: NodeIndex = NodeIndex::MAX;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Node {
    pub voxels: [Voxel; NODE_VOXEL_LENGTH],
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

impl Node {
    pub fn new(voxels: [Voxel; NODE_VOXEL_LENGTH]) -> Self {
        Node { voxels }
    }

    fn rotate_voxel_pos(pos: IVec3, mat: Mat4, rot_offset: IVec3) -> IVec3 {
        let p = pos - (NODE_SIZE / 2);
        let new_pos_f = mat.transform_vector3(p.as_vec3());
        new_pos_f.round().as_ivec3() + (NODE_SIZE / 2) - rot_offset
    }

    pub fn get_rotated_voxels(&self, rot: Rot) -> impl Iterator<Item = (IVec3, Voxel)> {
        let mat: Mat4 = rot.into();
        let rot_offset = rot.rot_offset();

        self.voxels
            .into_iter()
            .enumerate()
            .zip(repeat((mat, rot_offset)))
            .map(|((i, v), (mat, rot_offset))| {
                let pos = to_3d_i(i as i32, NODE_SIZE);
                let new_pos = Self::rotate_voxel_pos(pos, mat, rot_offset);
                (new_pos, v)
            })
    }

    pub fn is_duplicate_node_id(&self, rot: Rot, other_node: &Node, other_rot: Rot) -> bool {
        let mut same = true;

        let mat: Mat3 = rot.into();
        let inv_rot: Rot = mat.inverse().into();
        let combined_rot = inv_rot * other_rot;

        for (rotated_pos, voxel) in other_node.get_rotated_voxels(combined_rot) {
            let voxel_index = to_1d_i(rotated_pos, NODE_SIZE) as usize;

            if self.voxels[voxel_index] != voxel {
                same = false;
                break;
            }
        }

        same
    }

    pub fn shares_side_voxels(
        &self,
        rot: Rot,
        other_node: &Node,
        other_rot: Rot,
        side: IVec3,
    ) -> bool {
        let mat: Mat4 = rot.into();
        let other_mat: Mat4 = other_rot.into();

        let rot_offset = rot.rot_offset();
        let other_rot_offset = other_rot.rot_offset();

        let (index_i, index_j, index_k, k_pos, k_neg) = if side.x == 1 {
            (1, 2, 0, 3, 0)
        } else if side.x == -1 {
            (1, 2, 0, 0, 3)
        } else if side.y == 1 {
            (1, 0, 2, 0, 3)
        } else if side.y == -1 {
            (1, 0, 2, 3, 0)
        } else if side.z == 1 {
            (0, 1, 2, 0, 3)
        } else if side.z == -1 {
            (0, 1, 2, 3, 0)
        } else {
            unreachable!()
        };

        let mut same = true;
        for i in 0..4 {
            for j in 0..4 {
                let mut p = [0, 0, 0];
                p[index_i] = i;
                p[index_j] = j;
                p[index_k] = k_pos;
                let pos = IVec3::from(p);

                p[index_k] = k_neg;
                let other_pos = IVec3::from(p);

                let rotated_pos = Self::rotate_voxel_pos(pos, mat, rot_offset);
                let rotated_other_pos =
                    Self::rotate_voxel_pos(other_pos, other_mat, other_rot_offset);
                let voxel = self.voxels[to_1d_i(rotated_pos, NODE_SIZE)];
                let other_voxel = other_node.voxels[to_1d_i(rotated_other_pos, NODE_SIZE)];

                if voxel != other_voxel {
                    same = false;
                    break;
                }
            }
        }

        same
    }
}

impl Default for Node {
    fn default() -> Self {
        Self {
            voxels: [VOXEL_EMPTY; NODE_VOXEL_LENGTH],
        }
    }
}

impl NodeID {
    pub fn new(index: NodeIndex, rot: Rot) -> NodeID {
        NodeID { index, rot }
    }

    pub fn empty() -> NodeID {
        NodeID {
            index: NODE_INDEX_EMPTY,
            rot: Default::default(),
        }
    }
    pub fn any() -> NodeID {
        NodeID {
            index: NODE_INDEX_ANY,
            rot: Default::default(),
        }
    }

    pub fn is_empty(self) -> bool {
        self.index == NODE_INDEX_EMPTY
    }

    pub fn is_any(self) -> bool {
        self.index == NODE_INDEX_ANY
    }

    pub fn is_some(self) -> bool {
        self.index != NODE_INDEX_EMPTY && self.index != NODE_INDEX_ANY
    }
}

impl Default for NodeID {
    fn default() -> Self {
        Self::empty()
    }
}

impl Into<u32> for NodeID {
    fn into(self) -> u32 {
        if self.is_empty() {
            0
        } else {
            ((self.index as u32) << 7) + <Rot as Into<u8>>::into(self.rot.to_glsl()) as u32
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
