pub mod block_preview;
pub mod empty;
pub mod hull;
pub mod solver;

use crate::node::{Material, Node, NodeID, NodeIndex, NODE_INDEX_ANY, NODE_INDEX_EMPTY};
use crate::rotation::Rot;
use crate::rules::block_preview::BlockPreview;
use crate::rules::solver::Solver;
use crate::voxel_loader::VoxelLoader;
use octa_force::anyhow::Result;
use octa_force::glam::UVec3;
use std::ops::Mul;

const NODE_ID_MAP_INDEX_NONE: usize = NODE_INDEX_EMPTY;
const NODE_ID_MAP_INDEX_ANY: usize = NODE_INDEX_ANY;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Default, Debug)]
pub enum Prio {
    #[default]
    ZERO,
    BASE,

    HULL0,
    HULL1,
    HULL2,
    HULL3,
    HULL4,
    HULL5,
    HULL6,
    HULL7,
    HULL8,
    HULL9,
    HULL10,
}

pub struct Rules {
    pub materials: [Material; 256],
    pub nodes: Vec<Node>,

    pub block_names: Vec<String>,
    pub block_previews: Vec<BlockPreview>,

    pub duplicate_node_ids: Vec<Vec<Vec<NodeID>>>,

    pub solvers: Vec<Box<dyn Solver>>,
}

impl Rules {
    pub fn new(voxel_loader: VoxelLoader) -> Result<Self> {
        let mut rules = Rules {
            materials: voxel_loader.load_materials(),
            nodes: vec![],
            block_names: vec![],
            block_previews: vec![],
            duplicate_node_ids: vec![vec![vec![NodeID::default()]]],
            solvers: vec![],
        };

        rules.make_empty();
        rules.make_hull(&voxel_loader)?;

        Ok(rules)
    }

    pub fn get_duplicate_node_id(&mut self, node_id: NodeID) -> NodeID {
        let node = &self.nodes[node_id.index];

        while self.duplicate_node_ids.len() <= node_id.index {
            self.duplicate_node_ids.push(vec![])
        }

        let mut new_node_id = None;
        for ids in self.duplicate_node_ids[node_id.index].iter_mut() {
            if ids.contains(&node_id) {
                new_node_id = Some(ids[0]);
                break;
            }

            if node.is_duplicate_node_id(node_id.rot, node, ids[0].rot) {
                ids.push(node_id);
                new_node_id = Some(ids[0]);
                break;
            }
        }

        if new_node_id.is_none() {
            self.duplicate_node_ids[node_id.index].push(vec![node_id]);
            new_node_id = Some(node_id);
        }

        new_node_id.unwrap()
    }

    pub fn add_node(&mut self, node: Node) -> NodeID {
        let rots = Rot::IDENTITY.get_all_permutations();

        let mut id = None;
        for (i, test_node) in self.nodes.iter().enumerate() {
            for rot in rots.iter() {
                if node.is_duplicate_node_id(*rot, test_node, Rot::IDENTITY) {
                    id = Some(NodeID::new(i, *rot));
                }
            }
        }

        if id.is_none() {
            id = Some(NodeID::new(self.nodes.len(), Rot::IDENTITY));
            self.nodes.push(node);
        }

        id.unwrap()
    }
}

// Helper functions
impl Rules {
    fn load_node(&mut self, name: &str, voxel_loader: &VoxelLoader) -> Result<NodeID> {
        let (model_index, _) = voxel_loader.find_model(name)?;
        let node = voxel_loader.load_node_model(model_index)?;

        let id = self.add_node(node);
        let dup_id = self.get_duplicate_node_id(id);

        Ok(dup_id)
    }

    fn load_multi_node(
        &mut self,
        name: &str,
        voxel_loader: &VoxelLoader,
    ) -> Result<(UVec3, Vec<NodeID>)> {
        let (model_index, _) = voxel_loader.find_model(name)?;
        let (size, nodes) = voxel_loader.load_multi_node_model(model_index)?;

        let mut node_ids = vec![];
        for node in nodes {
            let id = self.add_node(node);
            let dup_id = self.get_duplicate_node_id(id);
            node_ids.push(dup_id);
        }

        Ok((size, node_ids))
    }
}
