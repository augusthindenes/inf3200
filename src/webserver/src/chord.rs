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

#[derive(Debug, Clone, Serialize)]
pub struct KnownNodesViewmodel {
    pub node_hash: String,
    pub successor: String,
    pub others: Vec<String>,
}

impl KnownNodes {
    // get list of known node addresses as "host:port" strings
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
    // Convert to viewmodel for API response
    pub fn to_viewmodel(&self) -> KnownNodesViewmodel {
        // Get addresses of all known nodes except self and successor
        let mut others: Vec<String> = Vec::new();
        for entry in &self.finger_table {
            if entry.node.id != self.successor.id && entry.node.id != self.me.id {
                others.push(entry.node.addr.label());
            }
        }
        others.push(self.predecessor.addr.label());
        // We only want distinct addresses
        others.dedup();
        KnownNodesViewmodel {
            node_hash: format!("{:016x}", self.me.id),
            successor: self.successor.addr.label(),
            others,
        }
    }
}

#[derive(Clone)]
pub struct ChordNode {
    pub nodes: Arc<KnownNodes>,
    pub client: Client,
}

// Implement routing and ChordNode operations
impl ChordNode {
    // Init single node network on startup
    pub fn new (addr: NodeAddr) -> Self {
        // Create a Node for ourselves
        let node = Node::new(addr);
        // Set predecessor and successor to ourselves
        let mut known_nodes = KnownNodes {
            me: node.clone(),
            predecessor: node.clone(),
            successor: node.clone(),
            finger_table: Vec::with_capacity(M as usize),
        };

        // Fill finger table with self references
        for i in 0..M {
            let start = node.id.wrapping_add(1u64 << i);
            known_nodes.finger_table.push(FingerEntry { start, node: node.clone() });
        }

        // Return the ChordNode
        ChordNode {
            nodes: Arc::new(known_nodes),
            client: Client::default(),
        }
    }

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
}