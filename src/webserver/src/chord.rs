use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::utils::{hash_key, in_interval_open_closed, in_interval_open_open};
use crate::config::M;
use crate::simulate::CrashState;

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
pub struct KnowNodesLabel {
    pub me: String,
    pub predecessor: String,
    pub successor: String,
    pub fingers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnownNodesViewmodel {
    pub node_hash: String,
    pub successor: String,
    pub others: Vec<String>,
}

impl KnownNodes {
    pub fn get_all_nodes(&self) -> KnowNodesLabel {
        let mut fingers: Vec<String> = Vec::new();
        for entry in self.finger_table.iter().skip(1) { // Skip index 0
            fingers.push(entry.node.addr.label());
        }
        KnowNodesLabel {
            me: self.me.addr.label(),
            predecessor: self.predecessor.addr.label(),
            successor: self.successor.addr.label(),
            fingers,
        }
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
        // finger[i] should point to successor of (n + 2^(i-1)) mod 2^M
        let id_space_mask = if M == 64 { u64::MAX } else { (1u64 << M) - 1 };
        for i in 1..=M {
            let offset = 1u64 << ((i - 1) as u32);
            let start = (node.id.wrapping_add(offset)) & id_space_mask;
            known_nodes.finger_table.push(FingerEntry { start, node: node.clone() });
        }

        // Return the ChordNode
        ChordNode {
            nodes: known_nodes,
            client: Client::default(),
            fix_next: AtomicUsize::new(1), // Start at 1 since finger table is 1-indexed now
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
        let successor = rpc_find_successor(&self.client, &seed, self.nodes.me.id).await?;

        // Update our successor and first finger table entry
        self.nodes.successor = successor.clone();
        if self.nodes.finger_table.len() > 1 {
            self.nodes.finger_table[1].node = successor.clone();
        }
        
        // Predecessor := nil (set to self temporarily, will be updated by stabilization)
        self.nodes.predecessor = self.nodes.me.clone();
        
        // Notify our successor that we might be its predecessor
        let _ = rpc_notify(&self.client, &successor.addr, &self.nodes.me).await;
        
        Ok(())
    }

    // Gracefully leave the Chord network
    pub async fn leave(&mut self) -> ChordResult<()> {
        // If predecessor or successor is self, nothing to do (single node network)
        if self.nodes.predecessor.id == self.nodes.me.id || self.nodes.successor.id == self.nodes.me.id {
            return Ok(());
        }

        // Notify predecessor and successor to update their pointers, link pred <-> succ
        rpc_set_successor(&self.client, &self.nodes.predecessor.addr, &self.nodes.successor).await?;
        rpc_set_predecessor(&self.client, &self.nodes.successor.addr, &self.nodes.predecessor).await?;

        // Reset to single node network
        self.nodes.predecessor = self.nodes.me.clone();
        self.nodes.successor = self.nodes.me.clone();
        
        // Reset finger table entries to self
        let me_id = self.nodes.me.id;
        let me_node = self.nodes.me.clone();
        let id_space_mask = if M == 64 { u64::MAX } else { (1u64 << M) - 1 };

        for i in 1..=M {
            let offset = 1u64 << ((i - 1) as u32);
            let start = (me_id.wrapping_add(offset)) & id_space_mask;
            self.nodes.finger_table[i as usize] = FingerEntry { start, node: me_node.clone() };
        }

        Ok(())
    
    }

    // --- Periodic maintenance tasks ---
    // Run the maintenance tasks periodically
    pub fn maintenance(
        node: std::sync::Arc<tokio::sync::RwLock<Self>>,
        period_ms: u64,
        crash_state: std::sync::Arc<CrashState>,
    ) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(period_ms));
            let mut maintenance_paused = false;
            loop {
                interval.tick().await;

                if crash_state.is_crashed() {
                    if !maintenance_paused {
                        println!("Maintenance paused due to simulated crash");
                        maintenance_paused = true;
                    }
                    continue;
                }

                if maintenance_paused {
                    println!("Maintenance resumed after simulated crash");
                    maintenance_paused = false;
                }

                // Stabilize - use read lock first to check if we should skip
                let should_stabilize = {
                    let node_guard = node.read().await;
                    node_guard.nodes.successor.id != node_guard.nodes.me.id
                };

                if should_stabilize {
                    let stabilize_result = tokio::time::timeout(
                        Duration::from_secs(10),
                        ChordNode::stabilize(std::sync::Arc::clone(&node))
                    ).await;
                    match stabilize_result {
                        Ok(Ok(_)) => {},
                        Ok(Err(e)) => println!("Stabilize failed: {:?}", e),
                        Err(_) => println!("Stabilize timed out"),
                    }
                }

                // Fix fingers - use read lock first to check if we should skip
                let should_fix_fingers = {
                    let node_guard = node.read().await;
                    node_guard.nodes.successor.id != node_guard.nodes.me.id
                };
                
                if should_fix_fingers {
                    let fix_fingers_result = tokio::time::timeout(
                        Duration::from_secs(10),
                        ChordNode::fix_fingers(std::sync::Arc::clone(&node))
                    ).await;
                    match fix_fingers_result {
                        Ok(Ok(_)) => {},
                        Ok(Err(e)) => println!("Fix fingers failed: {:?}", e),
                        Err(_) => println!("Fix fingers timed out"),
                    }
                }

                // Check predecessor
                let check_pred_result = tokio::time::timeout(
                    Duration::from_secs(5),
                    async {
                        let mut node_guard = node.write().await;
                        node_guard.check_predecessor().await
                    }
                ).await;
                match check_pred_result {
                    Ok(Ok(_)) => {},
                    Ok(Err(e)) => println!("Check predecessor failed: {:?}", e),
                    Err(_) => println!("Check predecessor timed out"),
                }
            }
        });
    }
    
    // Stabilize verifies n's immediate successor and tells the successor about n
    // n. stabilize()
    async fn stabilize(node: std::sync::Arc<tokio::sync::RwLock<Self>>) -> ChordResult<()> {
        // Get current node and successor and release lock before RPC
        let (me, successor, client) = {
            let guard = node.read().await;
            (guard.nodes.me.clone(), guard.nodes.successor.clone(), guard.client.clone())
        };
        
        // Check if successor is alive first
        if !rpc_ping(&client, &successor.addr).await {
            println!("Successor {} is down, finding next alive node", successor.addr.label());
            // Successor is down, find next alive node in finger table
            let next_alive = {
                let guard = node.read().await;
                let mut found: Option<Node> = None;
                for entry in guard.nodes.finger_table.iter().skip(2) {
                    if entry.node.id != me.id && rpc_ping(&client, &entry.node.addr).await {
                        found = Some(entry.node.clone());
                        break;
                    }
                }
                found
            };
            
            // Update successor to next alive node or self if none found
            let mut guard = node.write().await;
            if let Some(alive_node) = next_alive {
                println!("Found alive node: {}", alive_node.addr.label());
                guard.nodes.successor = alive_node.clone();
                if guard.nodes.finger_table.len() > 1 {
                    guard.nodes.finger_table[1].node = alive_node;
                }
            } else {
                println!("No alive nodes found, setting successor to self");
                guard.nodes.successor = me.clone();
                if guard.nodes.finger_table.len() > 1 {
                    guard.nodes.finger_table[1].node = me.clone();
                }
            }
            return Ok(());
        }
        
        // x = successor.predecessor (RPC call without holding lock)
        let x_result = rpc_get_predecessor(&client, &successor.addr).await;
        
        // If we can't get predecessor (node might have crashed), just notify
        let x = match x_result {
            Ok(pred) => pred,
            Err(e) => {
                println!("Failed to get predecessor from successor: {:?}", e);
                // Try to notify anyway
                let _ = rpc_notify(&client, &successor.addr, &me).await;
                return Ok(());
            }
        };
        
        // Update state
        let mut guard = node.write().await;
        // if x ∈ (n, successor) then successor = x
        if in_interval_open_open(x.id, me.id, successor.id) {
            guard.nodes.successor = x.clone();
            if guard.nodes.finger_table.len() > 1 {
                guard.nodes.finger_table[1].node = x.clone();
            }
        }
        
        // successor.notify(n)
        let current_successor = guard.nodes.successor.clone();
        drop(guard); // Release lock before notify
        
        // Notify successor (ignore errors)
        let _ = rpc_notify(&client, &current_successor.addr, &me).await;
        
        Ok(())
    }
    
    // Fix finger table entries. Next stores the index of the next finger to fix.
    // n. fix_fingers()
    async fn fix_fingers(node: std::sync::Arc<tokio::sync::RwLock<Self>>) -> ChordResult<()> {
        let m = M as usize;
        
        // Get data and increment counter
        let (me_id, successor_node, seed, next, client) = {
            let guard = node.write().await;
            // next := next + 1 ; if next > m then next := 1
            let mut next = guard.fix_next.load(Ordering::Relaxed) + 1;
            if next > m {
                next = 1;
            }
            guard.fix_next.store(next, Ordering::Relaxed);
            
            // Current node info
            let seed = guard.nodes.successor.addr.clone();
            let successor = guard.nodes.successor.clone();
            let client = guard.client.clone();
            (guard.nodes.me.id, successor, seed, next, client)
        };
        
        // finger[next] := find_successor(n + 2^(next-1)) (without holding lock)
        // Make sure to mask to M-bit identifier space
        let id_space_mask = if M == 64 { u64::MAX } else { (1u64 << M) - 1 };
        let offset = 1u64 << ((next - 1) as u32);
        let start = (me_id.wrapping_add(offset)) & id_space_mask;
        
        // Try to find successor, but handle failures gracefully
        let finger_node = match rpc_find_successor(&client, &seed, start).await {
            Ok(node) => {
                // Verify the node is actually alive
                if rpc_ping(&client, &node.addr).await {
                    node
                } else {
                    println!("Found node {} is not responding, using successor", node.addr.label());
                    successor_node.clone()
                }
            },
            Err(e) => {
                println!("Failed to find successor for finger {}: {:?}, using successor", next, e);
                successor_node.clone()
            }
        };
        
        // Update finger table
        let mut guard = node.write().await;
        guard.nodes.finger_table[next].start = start;
        guard.nodes.finger_table[next].node = finger_node;
        
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
        let alive = rpc_ping(&self.client, &predecessor.addr).await;
        println!("Pinged predecessor {}: alive={}", predecessor.addr.label(), alive);
        if !alive {
            println!("Predecessor {} is down, setting to self", predecessor.addr.label());
            self.nodes.predecessor = me; // Set to self (nil)
        }

        Ok(())
    }

}

// --- RPC methods to interact with other nodes ---

// Ping another node to check if it's alive
// Returns false if node is crashed (503) or unreachable
async fn rpc_ping(client: &Client, node: &NodeAddr) -> bool {
    let url = format!("{}/internal/ping", node.to_url());
    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            // Node is alive only if status is 200-299 and not 503
            status.is_success() && status != 503
        },
        Err(_) => false,
    }
}

// Find the successor for the current node
async fn rpc_get_successor(client: &Client, node: &NodeAddr) -> ChordResult<Node> {
    let url = format!("{}/internal/successor", node.to_url());
    let response = client.get(&url).send().await?;
    let successor = response.json::<Node>().await?;
    Ok(successor)
}

// Find the predecessor for the current node
async fn rpc_get_predecessor(client: &Client, node: &NodeAddr) -> ChordResult<Node> {
    let url = format!("{}/internal/predecessor", node.to_url());
    let response = client.get(&url).send().await?;
    
    // Check for 503 (crashed node)
    if response.status() == 503 {
        return Err("Node is crashed (503)".into());
    }
    
    let predecessor = response.json::<Node>().await?;
    Ok(predecessor)
}

// Find the successor for a given node ID
async fn rpc_find_successor(client: &Client, seed: &NodeAddr, id: u64) -> ChordResult<Node> {
    let url = format!("{}/internal/find-successor?id={}", seed.to_url(), id);
    let response = client.get(url).send().await?;
    
    // Check for 503 (crashed node)
    if response.status() == 503 {
        return Err("Node is crashed (503)".into());
    }
    
    let successor = response.json::<Node>().await?;
    Ok(successor)
} 

// Notify a node that we might be its predecessor
async fn rpc_notify(client: &Client, node: &NodeAddr, me: &Node) -> ChordResult <()> {
    let url = format!("{}/internal/notify", node.to_url());
    client.post(&url).json(me).send().await?;
    Ok(())
}

// Set the successor of a node
async fn rpc_set_successor(client: &Client, node: &NodeAddr, successor: &Node) -> ChordResult<()> {
    let url = format!("{}/internal/set-successor", node.to_url());
    client.post(&url).json(successor).send().await?;
    Ok(())
}

// Set the predecessor of a node
async fn rpc_set_predecessor(client: &Client, node: &NodeAddr, predecessor: &Node) -> ChordResult<()> {
    let url = format!("{}/internal/set-predecessor", node.to_url());
    client.post(&url).json(predecessor).send().await?;
    Ok(())  
}