use actix_web::HttpResponse;
use actix_web::web::Bytes;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::cmp::Ordering;
use std::sync::Arc;

const M: u32 = 64;
const HOP_LIMIT: u32 = 64;

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
pub struct NetworkConfig {
    pub me: Node,
    pub predecessor: Node,
    pub successor: Node,
    pub finger_table: Vec<FingerEntry>,
}

impl NetworkConfig {
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
pub struct ChordHandler {
    network: Arc<NetworkConfig>,
    client: Client,
}

// Helper functions

// Function to hash a key using SHA-1 and return a u64 identifier
pub fn hash_key(key: &str) -> u64 {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    let result = hasher.finalize();
    // Use the first 8 bytes of the SHA-1 hash as u64
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&result[..8]);
    u64::from_be_bytes(bytes.try_into().unwrap())
}

// Check if id is in the (start, end) interval on the identifier circle
pub fn in_interval_open_open(id: u64, start: u64, end: u64) -> bool {
    if start < end {
        id > start && id < end
    } else if start > end {
        id > start || id < end
    } else {
        false
    }
}

// Check if id is in the (start, end] interval on the identifier circle
pub fn in_interval_open_closed(id: u64, start: u64, end: u64) -> bool {
    if start < end {
        id > start && id <= end
    } else if start > end {
        id > start || id <= end
    } else {
        // start == end means the whole circle (only true with 1 node); treat as owned
        true
    }
}

// Static initialization of the identifier circle (calculate our node's ID and set up finger table)
pub fn init_chord(me: NodeAddr, mut all_nodes: Vec<NodeAddr>) -> ChordHandler {
    // Make sure our own address is included
    if !all_nodes
        .iter()
        .any(|n| n.host == me.host && n.port == me.port)
    {
        all_nodes.push(me.clone());
    }

    // Compute IDs for all nodes and create Node structs
    let mut nodes: Vec<Node> = all_nodes.into_iter().map(|addr| Node::new(addr)).collect();
    // Sort nodes clockwise by ID
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

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

    // Build finger table
    let mut finger_table = Vec::new();
    for i in 0..M {
        let start = me_id.wrapping_add(1u64 << i);
        // Find first node >= start
        let finger_node = match nodes.binary_search_by(|n| {
            if n.id < start {
                Ordering::Less
            } else if n.id > start {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }) {
            Ok(idx) => nodes[idx].clone(),
            Err(idx) => nodes[idx % nodes.len()].clone(), // wrap around
        };
        finger_table.push(FingerEntry {
            start,
            node: finger_node,
        });
    }

    // Create the network configuration
    let network = NetworkConfig {
        me,
        predecessor: predecessor,
        successor: successor,
        finger_table,
    };

    // Return the ChordHandler with initialized network and HTTP client
    ChordHandler {
        network: Arc::new(network),
        client: Client::default(),
    }
}

// Implement routing and Chord operations
impl ChordHandler {
    // Check if this node is responsible for the given key
    pub fn responsible_for(&self, key: &str) -> bool {
        in_interval_open_closed(
            hash_key(key),
            self.network.predecessor.id,
            self.network.me.id,
        )
    }

    fn closest_preceding_node(&self, id: u64) -> Node {
        // Search finger table in reverse order for the closest preceding node
        for finger in self.network.finger_table.iter().rev() {
            if in_interval_open_open(finger.node.id, self.network.me.id, id) {
                return finger.node.clone();
            }
        }
        // If none found, return successor (as per Chord protocol)
        self.network.successor.clone()
    }

    pub fn get_network_info(&self) -> &NetworkConfig {
        &self.network
    }
}

// Functions for forwarding HTTP request to next node
pub async fn forward_get(
    chord: &ChordHandler,
    key: &str,
    hop_count: u32,
) -> actix_web::Result<HttpResponse> {
    if hop_count >= HOP_LIMIT {
        return Ok(HttpResponse::BadGateway().body("Chord hop limit exceeded")); // Prevent infinite loops
    }

    // Hash the key to find its ID
    let key_id = hash_key(key);
    // Check if this node is responsible for the key
    if chord.responsible_for(key) {
        return Ok(HttpResponse::Ok().finish()); // Placeholder: actual value retrieval not implemented here
    }
    // Find the closest preceding node
    let next_node = chord.closest_preceding_node(key_id);
    // Construct the URL for the next node
    let url = format!("{}/storage/{}", next_node.addr.to_url(), key);

    // Forward the GET request to the next node
    let response = chord
        .client
        .get(url)
        .header("X-Chord-Hop-Count", (hop_count + 1).to_string())
        .timeout(std::time::Duration::from_millis(1000))
        .send()
        .await;

    match response {
        Ok(r) => {
            let status = actix_web::http::StatusCode::from_u16(r.status().as_u16()).unwrap();
            let body = r
                .bytes()
                .await
                .unwrap_or_else(|_| Bytes::from_static(b"Error reading body"));
            Ok(HttpResponse::build(status).body(body))
        }
        Err(e) => Ok(HttpResponse::BadGateway().body(format!("forward error: {}", e))),
    }
}

pub async fn forward_put(
    chord: &ChordHandler,
    key: &str,
    value: Bytes,
    hop_count: u32,
) -> actix_web::Result<HttpResponse> {
    if hop_count >= HOP_LIMIT {
        return Ok(HttpResponse::BadGateway().body("Chord hop limit exceeded")); // Prevent infinite loops
    }

    // Hash the key to find its ID
    let key_id = hash_key(key);
    // Check if this node is responsible for the key
    if chord.responsible_for(key) {
        return Ok(HttpResponse::Ok().finish()); // Placeholder: actual value storage not implemented here
    }
    // Find the closest preceding node
    let next_node = chord.closest_preceding_node(key_id);
    // Construct the URL for the next node
    let url = format!("{}/storage/{}", next_node.addr.to_url(), key);

    // Forward the PUT request to the next node
    let response = chord
        .client
        .put(url)
        .header("X-Chord-Hop-Count", (hop_count + 1).to_string())
        .timeout(std::time::Duration::from_millis(1000))
        .body(value.clone())
        .send()
        .await;

    match response {
        Ok(r) => {
            let status = actix_web::http::StatusCode::from_u16(r.status().as_u16()).unwrap();
            let body = r.bytes().await.unwrap_or_else(|_| Bytes::from_static(b""));
            Ok(HttpResponse::build(status).body(body))
        }
        Err(e) => Ok(HttpResponse::BadGateway().body(format!("forward error: {}", e))),
    }
}
