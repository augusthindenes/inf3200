use actix_web::{get, post, put, HttpRequest, HttpResponse, Responder, web};
use std::time::Duration;

use crate::AppState;
use crate::chord::{Node, NodeAddr};
use crate::network::{forward_get, forward_put};
use crate::utils::{in_interval_open_closed, in_interval_open_open};
use crate::ChordNode;
use crate::config::HOP_LIMIT;

// Define a handler for the /helloworld route
#[get("/helloworld")]
// The handler uses the HostConfig to respond with the hostname and port it is running on
async fn helloworld(state: web::Data<AppState>) -> impl Responder {
    let chord = state.chord.read().await;
    HttpResponse::Ok().body(chord.nodes.me.addr.label())
}

#[get("/storage/{key}")]
async fn get_storage(
    req: HttpRequest,
    key: web::Path<String>,
    state: web::Data<AppState>,
) -> impl Responder {
    // get the key from the path and hop count from headers
    let key = key.into_inner();
    let hops = req
        .headers()
        .get("X-Chord-Hop-Count")
        .and_then(|h| h.to_str().ok().and_then(|s| s.parse::<u32>().ok()))
        .unwrap_or(0);

    // Aquire read lock on chord handler
    let chord = state.chord.read().await;

    if chord.responsible_for(&key) {
        match state.storage.read().await.get(&key) {
            Some(value) => HttpResponse::Ok().body(value),
            None => HttpResponse::NotFound().body("Key not found"),
        }
    } else {
        match forward_get(&chord, &key, hops).await {
            Ok(response) => response,
            Err(_) => HttpResponse::BadGateway().body("Error forwarding request"),
        }
    }
}

// Takes the key from the path and the value from the request body as UTF-8 string
#[put("/storage/{key}")]
async fn put_storage(
    req: HttpRequest,
    key: web::Path<String>,
    body: web::Bytes,
    state: web::Data<AppState>,
) -> impl Responder {
    // get the key from the path and hop count from headers
    let key = key.into_inner();
    let hops = req
        .headers()
        .get("X-Chord-Hop-Count")
        .and_then(|h| h.to_str().ok().and_then(|s| s.parse::<u32>().ok()))
        .unwrap_or(0);

    // Aquire read lock on chord handler
    let chord = state.chord.read().await;

    if chord.responsible_for(&key) {
        let value = match std::str::from_utf8(&body) {
            Ok(v) => v.to_string(),
            Err(_) => return HttpResponse::BadRequest().body("Value must be valid UTF-8"),
        };
        state.storage.write().await.put(key, value);
        HttpResponse::Ok().body("Value stored")
    } else {
        match forward_put(&chord, &key, body, hops).await {
            Ok(response) => response,
            Err(_) => HttpResponse::BadGateway().body("Error forwarding request"),
        }
    }
}

#[get("/node-info")]
async fn get_node_info(state: web::Data<AppState>) -> impl Responder {
    // Aquire read lock on chord handler
    let chord = state.chord.read().await;
    // Get current node info
    let node_info = chord.nodes.to_viewmodel();
    HttpResponse::Ok().json(node_info)
}

#[get("/known-nodes")]
async fn get_known_nodes(state: web::Data<AppState>) -> impl Responder {
    // Aquire read lock on chord handler
    let chord = state.chord.read().await;
    // Get known nodes info
    let known_nodes = chord.nodes.get_all_nodes();
    HttpResponse::Ok().json(known_nodes)
}

#[post("/join")]
async fn post_join(
    query: web::Query<std::collections::HashMap<String, String>>,
    state: web::Data<AppState>,
) -> impl Responder {
    // Get nprime parameter from query string
    if let Some(nprime) = query.get("nprime") {
        // Create a NodeAddr from the nprime string
        let parts: Vec<&str> = nprime.split(':').collect();
        if parts.len() == 2 {
            let host = parts[0].to_string();
            if let Ok(port) = parts[1].parse::<u16>() {
                let addr = NodeAddr { host, port };
                
                // Prepare join and do RPCs without holding write lock
                let join_result = {
                    let chord = state.chord.read().await;
                    chord.join_prepare(addr).await
                };
                
                // Apply state changes, with write lock
                match join_result {
                    Ok(Some((successor, finger_updates))) => {
                        let mut chord = state.chord.write().await;
                        chord.join_apply(successor, finger_updates);
                        HttpResponse::Ok().body("Joined the DHT successfully")
                    },
                    Ok(None) => HttpResponse::Ok().body("Already in network"),
                    Err(e) => HttpResponse::BadGateway().body(format!("Error joining DHT: {}", e)),
                }
            } else {
                HttpResponse::BadRequest().body("Invalid port number")
            }
        } else {
            HttpResponse::BadRequest().body("Invalid nprime format")
        }
    } else {
        HttpResponse::BadRequest().body("Missing nprime parameter")
    }
}

#[post("/leave")]
async fn post_leave(state: web::Data<AppState>) -> impl Responder {
    // Prepare leave and do RPCs without holding write lock
    let should_leave = {
        let chord = state.chord.read().await;
        chord.leave_prepare().await
    };
    
    // Apply leave if needed, with write lock
    match should_leave {
        Ok(true) => {
            let mut chord = state.chord.write().await;
            chord.leave_apply();
            HttpResponse::Ok().body("Left the DHT successfully")
        },
        Ok(false) => HttpResponse::Ok().body("Already a single node"),
        Err(e) => HttpResponse::BadGateway().body(format!("Error leaving DHT: {}", e)),
    }
}

#[post("/reset")]
async fn post_reset(state: web::Data<AppState>) -> impl Responder {
    let mut chord = state.chord.write().await;
    // Create a completely new ChordNode with the same address
    let addr = chord.nodes.me.addr.clone();
    *chord = ChordNode::new(addr);
    drop(chord); // Release lock before clearing storage
    
    // Also clear storage
    state.storage.write().await.clear();
    HttpResponse::Ok().body("Node reset to initial state")
}

#[post("/sim-crash")]
async fn post_sim_crash(state: web::Data<AppState>) -> impl Responder {
    state.crash_state.crash();
    HttpResponse::Ok().body("Node crashed - all responses disabled")
}

#[post("/sim-recover")]
async fn post_sim_recover(state: web::Data<AppState>) -> impl Responder {
    state.crash_state.recover();
    HttpResponse::Ok().body("Node recovered - responses enabled")
}

// --- Internal RPC endpoints ...

// Ping another node to check if it's alive
#[get("/internal/ping")]
async fn ping_handler() -> impl Responder {
    HttpResponse::Ok().finish()
}

// Get current node's successor
#[get("/internal/successor")]
async fn get_successor(state: web::Data<AppState>) -> impl Responder {
    // Aquire read lock on chord handler
    let chord = state.chord.read().await;
    HttpResponse::Ok().json(chord.nodes.successor.clone())
}

// Get current node's predecessor
#[get("/internal/predecessor")]
async fn get_predecessor(state: web::Data<AppState>) -> impl Responder {
    let chord = state.chord.read().await;
    HttpResponse::Ok().json(chord.nodes.predecessor.clone())
}

// Find the successor for a given ID
// n.find_successor(id)
//  if id ∈ (n, successor]
//      return successor
//  else
//      n0 = closest_preceding_node(id)
//      return n0.find_successor(id)
#[get("/internal/find-successor")]
async fn find_successor(
    query: web::Query<std::collections::HashMap<String, String>>,
    state: web::Data<AppState>,
) -> impl Responder {
    let id = query
        .get("id")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    
    // Get hop count to prevent infinite forwarding loops
    let hops = query
        .get("hops")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    
    // If we've exceeded hop limit, return successor to break the chain
    if hops >= HOP_LIMIT {
        let chord = state.chord.read().await;
        return HttpResponse::Ok().json(chord.nodes.successor.clone());
    }

    // Get node info WITHOUT holding lock during RPC
    let (me, successor, n0, client) = {
        let chord = state.chord.read().await;
        let me = chord.nodes.me.clone();
        let successor = chord.nodes.successor.clone();
        
        // Check if id is in (n, successor]
        if in_interval_open_closed(id, me.id, successor.id) {
            return HttpResponse::Ok().json(successor);
        }
        
        // Otherwise, find closest preceding node
        let n0 = chord.closest_preceding_node(id);
        let client = chord.client.clone();
        (me, successor, n0, client)
    };
    
    // Now we can do RPC without any lock held
    if n0.addr.label() == me.addr.label() {
        return HttpResponse::Ok().json(successor); // Prevent infinite loop
    }

    // Forward the request to n0 with incremented hop count
    let url = format!("{}/internal/find-successor?id={}&hops={}", n0.addr.to_url(), id, hops + 1);
    match client.get(&url).send().await {
        Ok(resp) => {
            // Check if the forwarded node is crashed (503)
            if resp.status() == 503 {
                // Forwarded node is crashed, return our successor instead
                return HttpResponse::Ok().json(successor);
            }
            
            match resp.json::<Node>().await {
                Ok(node) => HttpResponse::Ok().json(node),
                Err(_) => {
                    // JSON decode failed, likely a crashed node returned HTML/text
                    // Return our successor as fallback
                    HttpResponse::Ok().json(successor)
                }
            }
        },
        Err(_) => {
            // Network error, return our successor as fallback
            HttpResponse::Ok().json(successor)
        }
    }
}

// Notify n' that n might be its predecessor
// n.notify(n')
//  if predecessor is null or n' ∈ (predecessor, n)
//      predecessor = n'
// Body: Node (the notifying node n')
#[post("/internal/notify")]
async fn notify(
    state: web::Data<AppState>,
    body: web::Json<Node>,
) -> impl Responder {
    let n0 = body.into_inner();
    
    // Try to acquire write lock with timeout
    match tokio::time::timeout(
        Duration::from_millis(200),
        state.chord.write()
    ).await {
        Ok(mut chord_write) => {
            let me = chord_write.nodes.me.clone();
            let predecessor = chord_write.nodes.predecessor.clone();
            let successor = chord_write.nodes.successor.clone();

            // If we're alone (successor is self), the notifying node becomes our successor too
            if successor.id == me.id {
                chord_write.nodes.successor = n0.clone();
                chord_write.nodes.predecessor = n0.clone();
                // Update first finger table entry
                if chord_write.nodes.finger_table.len() > 1 {
                    chord_write.nodes.finger_table[1].node = n0;
                }
            } 
            // Otherwise check if predecessor should be updated
            else if predecessor.id == me.id || in_interval_open_open(n0.id, predecessor.id, me.id) {
                chord_write.nodes.predecessor = n0;
            }
            HttpResponse::Ok().finish()
        },
        Err(_) => {
            // Timeout - return success anyway, next stabilize will retry
            HttpResponse::Ok().finish()
        }
    }
}

// Update the current node's successor
// Body: Node (the new successor)
#[post("/internal/set-successor")]
async fn set_successor(
    state: web::Data<AppState>,
    body: web::Json<Node>,
) -> impl Responder {
    // Use timeout to prevent deadlock
    match tokio::time::timeout(
        Duration::from_millis(200),
        state.chord.write()
    ).await {
        Ok(mut chord_write) => {
            chord_write.nodes.successor = body.into_inner();
            HttpResponse::Ok().finish()
        },
        Err(_) => {
            HttpResponse::RequestTimeout().body("Timeout acquiring lock")
        }
    }
}

// Update the current node's predecessor
// Body: Node (the new predecessor)
#[post("/internal/set-predecessor")]
async fn set_predecessor(
    state: web::Data<AppState>,
    body: web::Json<Node>,
) -> impl Responder {
    // Use timeout to prevent deadlock
    match tokio::time::timeout(
        Duration::from_millis(200),
        state.chord.write()
    ).await {
        Ok(mut chord_write) => {
            chord_write.nodes.predecessor = body.into_inner();
            HttpResponse::Ok().finish()
        },
        Err(_) => {
            HttpResponse::RequestTimeout().body("Timeout acquiring lock")
        }
    }
}
