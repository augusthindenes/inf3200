use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

use crate::utils::{hash_key, in_interval_open_closed, in_interval_open_open};
use crate::config::M;

// Define a custom result type for Chord operations
type ChordResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

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
        for entry in self.finger_table.iter().skip(1) { // Skip index 0
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
        for entry in self.finger_table.iter().skip(1) { // Skip index 0
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

pub struct ChordNode {
    pub nodes: KnownNodes,
    pub client: Client,
    fix_next: AtomicUsize, // Stores the current next finger index to fix in [1, M]
}

// Manual clone implementation for ChordNode
impl Clone for ChordNode {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            client: self.client.clone(),
            fix_next: AtomicUsize::new(self.fix_next.load(Ordering::Relaxed)),
        }
    }
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
            finger_table: Vec::with_capacity(M as usize + 1),
        };
        // Push first finger entry - index 0 (not used)
        known_nodes.finger_table.push(FingerEntry { start: node.id, node: node.clone() });
        
        // Fill finger table with self references
        for i in 1..=M {
            let start = node.id.wrapping_add(1u64 << ((i - 1) as u32));
            known_nodes.finger_table.push(FingerEntry { start, node: node.clone() });
        }

        // Return the ChordNode
        ChordNode {
            nodes: known_nodes,
            client: Client::default(),
            fix_next: AtomicUsize::new(0),
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
        for finger in self.nodes.finger_table.iter().skip(1).rev() {
            if in_interval_open_open(finger.node.id, self.nodes.me.id, id) {
                return finger.node.clone();
            }
        }
        // If none found, return successor (as per Chord protocol)
        self.nodes.successor.clone()
    }

    // Join a Chord network via a known node (seed node)
    pub async fn join(&mut self, seed: NodeAddr) -> ChordResult<()> {

        // Check if seed node is self
        if seed.label() == self.nodes.me.addr.label() {
            return Ok(());
        }

        // Successor := n'.find_successor(me.id)
        let successor = self.rpc_find_successor(&seed, self.nodes.me.id).await?;

        // Predecessor := nil/me
        // Update our known nodes
        {
            self.nodes.successor = successor;
            self.nodes.predecessor = self.nodes.me.clone(); // Temporarily set to self
        }
        
        Ok(())
    }

    // Gracefully leave the Chord network
    pub async fn leave(&mut self) -> ChordResult<()> {
        // If predecessor or successor is self, nothing to do (single node network)
        if self.nodes.predecessor.id == self.nodes.me.id || self.nodes.successor.id == self.nodes.me.id {
            return Ok(());
        }

        // Notify predecessor and successor to update their pointers, link pred <-> succ
        self.rpc_set_successor(&self.nodes.predecessor.addr, &self.nodes.successor).await?;
        self.rpc_set_predecessor(&self.nodes.successor.addr, &self.nodes.predecessor).await?;

        // Reset to single node network
        self.nodes.predecessor = self.nodes.me.clone();
        self.nodes.successor = self.nodes.me.clone();
        
        // Reset finger table entries to self
        let me_id = self.nodes.me.id;
        let me_node = self.nodes.me.clone();

        for i in 1..=M {
            let start = me_id.wrapping_add(1u64 << ((i - 1) as u32));
            self.nodes.finger_table.push(FingerEntry { start, node: me_node.clone()});
        }

        Ok(())
    
    }

    // --- Periodic maintenance tasks ---
    // Run the maintenance tasks periodically
    pub fn maintenance(&self, period_ms: u64) {
        let mut chord_clone = self.clone();
        tokio::spawn(async move {
            loop {
                // stabilize -> fix_fingers -> check_predecessor
                let _ = chord_clone.stabilize().await;
                let _ = chord_clone.fix_fingers(None).await;
                let _ = chord_clone.check_predecessor().await;
                sleep(Duration::from_millis(period_ms)).await;
            }
        });
    }

    // Stabilize verifies n's immediate successor and tells the successor about n
    // n. stabilize()
    pub async fn stabilize(&mut self) -> ChordResult<()> {
        
        // Get current node and successor
        let (me, successor) = {
            (self.nodes.me.clone(), self.nodes.successor.clone())
        };

        // x = successor.predecessor
        let x = self.rpc_get_predecessor(&successor.addr).await?;

        // if x ∈ (n, successor) then successor = x
        if in_interval_open_open(x.id, me.id, successor.id) {
            self.nodes.successor = x;
        }

        // successor.notify(n)
        let current_successor = { // Capture current successor
            self.nodes.successor.clone()
        };
        self.rpc_notify(&current_successor.addr, &me).await?;

        Ok(())
    }

    // Fix finger table entries. Next stores the index of the next finger to fix.
    // n. fix_fingers()
    pub async fn fix_fingers(&mut self, seed_hint: Option<&NodeAddr>) -> ChordResult<()> {
        let m = M as usize;
        
        // next := next + 1 ; if next > m then next := 1
        let mut next = self.fix_next.load(Ordering::Relaxed) + 1;
        if next > m {
            next = 1;
        }
        self.fix_next.store(next, Ordering::Relaxed);

        // Current node info
        let me_id = self.nodes.me.id;
        let default_seed = self.nodes.successor.addr.clone();
        let seed = seed_hint.cloned().unwrap_or(default_seed);       
        
        // finger[next] := find_successor(n + 2^(next-1))
        let start = me_id.wrapping_add(1u64 << ((next - 1) as u32));
        let finger_node = self.rpc_find_successor(&seed, start).await?;

        // Update finger table
        self.nodes.finger_table[next].start = start;
        self.nodes.finger_table[next].node = finger_node;
        
        Ok(())
    }

    // Check if predecessor is alive
    // n. check_predecessor()
    pub async fn check_predecessor(&mut self) -> ChordResult<()> {
        // Get current node and predecessor
        let (me, predecessor) = {
            (self.nodes.me.clone(), self.nodes.predecessor.clone())
        };
        // If predecessor is self, nothing to check
        if predecessor.id == me.id {
            return Ok(());
        }
        // if predecessor.ping() fails then predecessor = nil
        let alive = self.rpc_ping(&predecessor.addr).await;
        if !alive {
            self.nodes.predecessor = me; // Set to self (nil)
        }

        Ok(())
    }

    // --- RPC methods to interact with other nodes ---

    // Ping another node to check if it's alive
    async fn rpc_ping(&self, node: &NodeAddr) -> bool {
        let url = format!("{}/internal/ping", node.to_url());
        self.client.get(url).send().await.is_ok()
    }

    // Find the successor for the current node
    async fn rpc_get_successor(&self, node: &NodeAddr) -> ChordResult<Node> {
        let url = format!("{}/internal/successor", node.to_url());
        let response = self.client.get(&url).send().await?;
        let successor = response.json::<Node>().await?;
        Ok(successor)
    }

    // Find the predecessor for the current node
    async fn rpc_get_predecessor(&self, node: &NodeAddr) -> ChordResult<Node> {
        let url = format!("{}/internal/predecessor", node.to_url());
        let response = self.client.get(&url).send().await?;
        let predecessor = response.json::<Node>().await?;
        Ok(predecessor)
    }

    // Find the successor for a given node ID
    async fn rpc_find_successor(&self, seed: &NodeAddr, id: u64) -> ChordResult<Node> {
        let url = format!("{}/internal/find-successor?id={}", seed.to_url(), id);
        let response = self.client.get(url).send().await?;
        let successor = response.json::<Node>().await?;
        Ok(successor)
    } 

    // Notify a node that we might be its predecessor
    async fn rpc_notify(&self, node: &NodeAddr, me: &Node) -> ChordResult <()> {
        let url = format!("{}/internal/notify", node.to_url());
        self.client.post(&url).json(me).send().await?;
        Ok(())
    }

    // Set the successor of a node
    async fn rpc_set_successor(&self, node: &NodeAddr, successor: &Node) -> ChordResult<()> {
        let url = format!("{}/internal/set-successor", node.to_url());
        self.client.post(&url).json(successor).send().await?;
        Ok(())
    }

    // Set the predecessor of a node
    async fn rpc_set_predecessor(&self, node: &NodeAddr, predecessor: &Node) -> ChordResult<()> {
        let url = format!("{}/internal/set-predecessor", node.to_url());
        self.client.post(&url).json(predecessor).send().await?;
        Ok(())  
    }
}

