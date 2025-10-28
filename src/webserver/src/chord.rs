use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::utils::{hash_key, in_interval_open_closed, in_interval_open_open};
use crate::config::M;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeAddr {
    pub host: String,
    pub port: u16,
}

impl NodeAddr {
    pub fn to_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    pub fn label(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: u64,
    pub addr: NodeAddr,
}

impl Node {
    pub fn new(addr: NodeAddr) -> Self {
        let id = hash_key(&addr.label());
        Node { id, addr }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FingerEntry {
    pub start: u64,
    pub node: Node,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnownNodes {
    pub me: Node,
    pub predecessor: Node,
    pub successor: Node,
    pub finger_table: Vec<FingerEntry>,
}

impl KnownNodes {
    // for network endpoint
    pub fn get_known_nodes(&self) -> Vec<String> {
        // Known nodes include all nodes in the finger table, predecessor, and successor
        let mut known_nodes = Vec::new();
        for entry in &self.finger_table {
            known_nodes.push(entry.node.addr.label());
        }
        let pre = self.predecessor.addr.label();
        if !known_nodes.contains(&pre) {
            known_nodes.push(pre);
        }
        let suc = self.successor.addr.label();
        if !known_nodes.contains(&suc) {
            known_nodes.push(suc);
        }
        known_nodes
    }
}

#[derive(Clone)]
pub struct ChordNode {
    pub nodes: Arc<KnownNodes>,
    pub client: Client,
}

// Static initialization of the identifier circle (calculate our node's ID and set up finger table)
pub fn init_chord(
    me: NodeAddr, 
    mut all_nodes: Vec<NodeAddr>,
    finger_count: Option<u32>,
    max_nodes: Option<usize>
) -> ChordNode {
    // Optional cap on number of nodes used (purely for testing)
    if let Some(n) = max_nodes {
        // Limit to at most n nodes (max 64)
        all_nodes.truncate(n.min(64));
    }

    let m = if let Some(f) = finger_count {
        f as usize
    } else {
        // If no finger count is specified, use the default
        // m = ceil(log2(number_of_nodes))
        (all_nodes.len() as f32).log2().ceil() as usize
    };

    // Make sure we include ourselves
    if !all_nodes.iter().any(|n| n.host == me.host && n.port == me.port) {
        // Don't exceed max nodes after adding ourselves
        if let Some(n) = max_nodes {
            if all_nodes.len() >= n.min(64) {
                all_nodes.pop();
            }
        }
        // Add ourselves
        all_nodes.push(me.clone());
    }

    // Compute IDs for all nodes and create Node structs
    let mut nodes: Vec<Node> = all_nodes.into_iter().map(|addr| Node::new(addr)).collect();
    
    // Sort nodes clockwise by ID
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    // In case of duplicate IDs (very unlikely), remove duplicates
    nodes.dedup_by(|a, b| a.id == b.id); // Remove duplicates by ID (keep first)

    // Find our own node in the sorted list
    let me_id = hash_key(&me.label()); // calculate our own ID
    let me_index = nodes
        .iter()
        .position(|n| n.id == me_id)
        .expect("Failed to find own node"); // find our index in the list
    let me = nodes[me_index].clone(); // get our own Node struct by index

    // Determine predecessor and successor
    let successor = nodes[(me_index + 1) % nodes.len()].clone();
    let predecessor = nodes[(me_index + nodes.len() - 1) % nodes.len()].clone();

    let start_i = M - m as u32; // Start finger entries from 2^(M-m)

    // Build finger table
    let mut finger_table = Vec::with_capacity(m);
    for i in start_i..M {
        let start = me.id.wrapping_add(1u64 << i);
        let finger_node = match nodes.binary_search_by_key(&start, |n| n.id) {
            Ok(idx) => nodes[idx].clone(), // Exact match found
            Err(idx) => nodes[idx % nodes.len()].clone(), // Closest successor
        };
        finger_table.push(FingerEntry { start, node: finger_node });
    }
    

    // Create list of known nodes
    let known_nodes = KnownNodes {
        me,
        predecessor: predecessor,
        successor: successor,
        finger_table,
    };

    // Return the ChordHandler with initialized network and HTTP client
    ChordNode {
        nodes: Arc::new(known_nodes),
        client: Client::default(),
    }
}

// Implement routing and ChordNode operations
impl ChordNode {
    // Check if this node is responsible for the given key
    pub fn responsible_for(&self, key: &str) -> bool {
        in_interval_open_closed(
            hash_key(key),
            self.nodes.predecessor.id,
            self.nodes.me.id,
        )
    }

    pub fn closest_preceding_node(&self, id: u64) -> Node {
        // Search finger table in reverse order for the closest preceding node
        for finger in self.nodes.finger_table.iter().rev() {
            if in_interval_open_open(finger.node.id, self.nodes.me.id, id) {
                return finger.node.clone();
            }
        }
        // If none found, return successor (as per Chord protocol)
        self.nodes.successor.clone()
    }

    pub fn get_known_nodes(&self) -> Vec<String> {
        self.nodes.get_known_nodes()
    }
}