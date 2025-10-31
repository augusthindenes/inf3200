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

        // Create HTTP client optimized for cluster network
        let client = Client::builder()
            .timeout(Duration::from_secs(3))  // Reduced from 5s for cluster network
            .connect_timeout(Duration::from_millis(500))  // Fast connection for cluster
            .pool_idle_timeout(Duration::from_secs(30))  // Keep connections longer
            .pool_max_idle_per_host(10)  // More connections per host for concurrent requests
            .build()
            .unwrap_or_else(|_| Client::default());

        // Return the ChordNode
        ChordNode {
            nodes: known_nodes,
            client,
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
    // This performs RPCs without holding locks, then returns the state updates to apply
    pub async fn join_prepare(&self, seed: NodeAddr) -> ChordResult<Option<(Node, Vec<(usize, Node)>)>> {

        // Check if seed node is self
        if seed.label() == self.nodes.me.addr.label() {
            return Ok(None);
        }

        // Successor := n'.find_successor(me.id)
        let successor = rpc_find_successor(&self.client, &seed, self.nodes.me.id).await?;
        
        // Initialize multiple finger table entries on join
        let id_space_mask = if M == 64 { u64::MAX } else { (1u64 << M) - 1 };
        let powers = [2, 4, 8]; // Skip 1 (already done), initialize key fingers
        
        let mut finger_updates = vec![(1, successor.clone())];
        
        for &i in &powers {
            if i <= M as usize && i < self.nodes.finger_table.len() {
                let offset = 1u64 << ((i - 1) as u32);
                let target_id = (self.nodes.me.id.wrapping_add(offset)) & id_space_mask;
                
                // Try to find successor, but don't fail join if this fails
                if let Ok(finger) = rpc_find_successor(&self.client, &seed, target_id).await {
                    finger_updates.push((i, finger));
                }
            }
        }
        
        // Notify our successor that we might be its predecessor
        let _ = rpc_notify(&self.client, &successor.addr, &self.nodes.me).await;
        
        Ok(Some((successor, finger_updates)))
    }
    
    // Apply join state updates (quick, can hold write lock)
    pub fn join_apply(&mut self, successor: Node, finger_updates: Vec<(usize, Node)>) {
        // Update our successor and finger table
        self.nodes.successor = successor.clone();
        self.nodes.predecessor = self.nodes.me.clone();
        
        for (index, node) in finger_updates {
            if index < self.nodes.finger_table.len() {
                self.nodes.finger_table[index].node = node;
            }
        }
    }

    // Gracefully leave the Chord network, performing necessary RPCs without holding locks
    pub async fn leave_prepare(&self) -> ChordResult<bool> {
        // If predecessor or successor is self, nothing to do (single node network)
        if self.nodes.predecessor.id == self.nodes.me.id || self.nodes.successor.id == self.nodes.me.id {
            return Ok(false);
        }

        // Notify predecessor and successor to update their pointers, link pred <-> succ
        rpc_set_successor(&self.client, &self.nodes.predecessor.addr, &self.nodes.successor).await?;
        rpc_set_predecessor(&self.client, &self.nodes.successor.addr, &self.nodes.predecessor).await?;

        Ok(true)
    }
    
    // Apply leave state changes (quick, holds write lock)
    pub fn leave_apply(&mut self) {
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
    }

    // Reset node to initial single-node state (without notifying other nodes)
    // This is useful for benchmarks and testing
    pub fn reset(&mut self) {
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

        // Reset fix_next counter
        self.fix_next.store(1, Ordering::Relaxed);
    }

    // --- Periodic maintenance tasks ---
    // Run the maintenance tasks periodically
    pub fn maintenance(
        node: std::sync::Arc<tokio::sync::RwLock<Self>>,
        period_ms: u64,
        crash_state: std::sync::Arc<CrashState>,
    ) {
        // Spawn individual long-running tasks for each maintenance operation
        // This prevents task explosion and ensures only one of each type runs at a time
        
        // Add jitter to prevent all nodes running tasks simultaneously
        use rand::Rng;
        let jitter_base = rand::thread_rng().gen_range(0..200);
        
        // Stabilize task
        tokio::spawn({
            let node = std::sync::Arc::clone(&node);
            let crash_state = std::sync::Arc::clone(&crash_state);
            let jitter = jitter_base;
            async move {
                // Initial jitter delay
                tokio::time::sleep(Duration::from_millis(jitter)).await;
                
                let mut interval = tokio::time::interval(Duration::from_millis(period_ms));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    if crash_state.is_crashed() {
                        continue;
                    }
                    
                    let should_run = {
                        let guard = node.read().await;
                        guard.nodes.successor.id != guard.nodes.me.id
                    };
                    
                    if should_run {
                        let _ = tokio::time::timeout(
                            Duration::from_secs(10),
                            ChordNode::stabilize(std::sync::Arc::clone(&node))
                        ).await;
                    }
                }
            }
        });
        
        // Fix fingers task - offset by 1/3 period
        tokio::spawn({
            let node = std::sync::Arc::clone(&node);
            let crash_state = std::sync::Arc::clone(&crash_state);
            let jitter = jitter_base + (period_ms / 3);
            async move {
                // Initial jitter delay
                tokio::time::sleep(Duration::from_millis(jitter)).await;
                
                let mut interval = tokio::time::interval(Duration::from_millis(period_ms));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    if crash_state.is_crashed() {
                        continue;
                    }
                    
                    let should_run = {
                        let guard = node.read().await;
                        guard.nodes.successor.id != guard.nodes.me.id
                    };
                    
                    if should_run {
                        let _ = tokio::time::timeout(
                            Duration::from_secs(10),
                            ChordNode::fix_fingers(std::sync::Arc::clone(&node))
                        ).await;
                    }
                }
            }
        });
        
        // Check predecessor task - offset by 2/3 period
        tokio::spawn({
            let node = std::sync::Arc::clone(&node);
            let crash_state = std::sync::Arc::clone(&crash_state);
            let jitter = jitter_base + (2 * period_ms / 3);
            async move {
                // Initial jitter delay
                tokio::time::sleep(Duration::from_millis(jitter)).await;
                
                let mut interval = tokio::time::interval(Duration::from_millis(period_ms));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    if crash_state.is_crashed() {
                        continue;
                    }
                    
                    let _ = tokio::time::timeout(
                        Duration::from_secs(5),
                        ChordNode::check_predecessor(std::sync::Arc::clone(&node))
                    ).await;
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
        
        // x = successor.predecessor (RPC call without holding lock)
        let x_result = rpc_get_predecessor(&client, &successor.addr).await;
        
        // If we can't get predecessor, successor might be down
        let x = match x_result {
            Ok(pred) => pred,
            Err(_) => {
                // Successor is down, find next alive node in finger table
                // First get the finger table entries without holding lock during ping
                let finger_entries = {
                    let guard = node.read().await;
                    guard.nodes.finger_table.iter().skip(2)
                        .filter(|e| e.node.id != me.id)
                        .map(|e| e.node.clone())
                        .collect::<Vec<_>>()
                };
                
                // Try to find alive node without holding any lock
                let mut next_alive: Option<Node> = None;
                for entry in finger_entries {
                    if rpc_ping(&client, &entry.addr).await {
                        next_alive = Some(entry);
                        break;
                    }
                }
                
                // Update successor to next alive node or self if none found
                let mut guard = node.write().await;
                if let Some(alive_node) = next_alive {
                    guard.nodes.successor = alive_node.clone();
                    if guard.nodes.finger_table.len() > 1 {
                        guard.nodes.finger_table[1].node = alive_node;
                    }
                } else {
                    guard.nodes.successor = me.clone();
                    if guard.nodes.finger_table.len() > 1 {
                        guard.nodes.finger_table[1].node = me.clone();
                    }
                }
                return Ok(());
            }
        };
        
        // Update state - determine if we need to update successor
        let (should_update, new_successor, current_successor, me_clone) = {
            let guard = node.read().await;
            let should_update = in_interval_open_open(x.id, me.id, successor.id);
            let new_succ = if should_update { x.clone() } else { successor.clone() };
            let curr_succ = guard.nodes.successor.clone();
            (should_update, new_succ, curr_succ, me.clone())
        };
        
        // Apply update if needed (quick write lock)
        if should_update || current_successor.id != new_successor.id {
            let mut guard = node.write().await;
            guard.nodes.successor = new_successor.clone();
            if guard.nodes.finger_table.len() > 1 {
                guard.nodes.finger_table[1].node = new_successor.clone();
            }
        }
        
        // Notify successor WITHOUT holding any lock
        let _ = rpc_notify(&client, &new_successor.addr, &me_clone).await;
        
        Ok(())
    }
    
    // Fix finger table entries. Next stores the index of the next finger to fix.
    // n. fix_fingers()
    async fn fix_fingers(node: std::sync::Arc<tokio::sync::RwLock<Self>>) -> ChordResult<()> {
        let m = M as usize;
        
        for _ in 0..2 {
            // Get data and increment counter - use read lock for most of this
            let (me_id, successor_node, seed, next, client) = {
                let guard = node.read().await;
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
            let id_space_mask = if M == 64 { u64::MAX } else { (1u64 << M) - 1 };
            let offset = 1u64 << ((next - 1) as u32);
            let start = (me_id.wrapping_add(offset)) & id_space_mask;
            
            // Try to find successor, but handle failures gracefully
            let finger_node = match rpc_find_successor(&client, &seed, start).await {
                Ok(node) => node,
                Err(_) => {
                    // Failed to find successor (dead nodes in chain), use successor
                    successor_node.clone()
                }
            };
            
            // Update finger table
            let mut guard = node.write().await;
            guard.nodes.finger_table[next].start = start;
            guard.nodes.finger_table[next].node = finger_node;
        }
        
        Ok(())
    }

    // Check if predecessor is alive
    // n. check_predecessor()
    async fn check_predecessor(node: std::sync::Arc<tokio::sync::RwLock<Self>>) -> ChordResult<()> {
        // Get current node and predecessor without holding lock during RPC
        let (me, predecessor, client) = {
            let guard = node.read().await;
            (guard.nodes.me.clone(), guard.nodes.predecessor.clone(), guard.client.clone())
        };
        
        // If predecessor is self, nothing to check
        if predecessor.id == me.id {
            return Ok(());
        }
        
        // Check if predecessor is alive (without holding lock)
        let alive = rpc_ping(&client, &predecessor.addr).await;
        
        // Update predecessor if it's dead
        if !alive {
            let mut guard = node.write().await;
            // Double-check predecessor hasn't changed while we were checking
            if guard.nodes.predecessor.id == predecessor.id {
                guard.nodes.predecessor = me;
            }
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
    let url = format!("{}/internal/find-successor?id={}&hops=0", seed.to_url(), id);
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