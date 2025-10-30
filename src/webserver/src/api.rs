use std::sync::atomic::Ordering;

use actix_web::{get, post, put, HttpRequest, HttpResponse, Responder, web};

use crate::AppState;
use crate::chord::{Node, NodeAddr};
use crate::network::{forward_get, forward_put};
use crate::utils::{in_interval_open_closed, in_interval_open_open};

// Define a handler for the /helloworld route
#[get("/helloworld")]
// The handler uses the HostConfig to respond with the hostname and port it is running on
async fn helloworld(state: web::Data<AppState>) -> impl Responder {
    let chord = state.chord.read().unwrap();
    HttpResponse::Ok().body(chord.nodes.me.addr.label())
}

#[get("/storage/{key}")]
async fn get_storage(
    req: HttpRequest,
    key: web::Path<String>,
    state: web::Data<AppState>,
) -> impl Responder {
    if !state.initialized.load(Ordering::Relaxed) {
        return HttpResponse::ServiceUnavailable().body("Distributed Hashtable not initialized");
    }

    // get the key from the path and hop count from headers
    let key = key.into_inner();
    let hops = req
        .headers()
        .get("X-Chord-Hop-Count")
        .and_then(|h| h.to_str().ok().and_then(|s| s.parse::<u32>().ok()))
        .unwrap_or(0);

    // Aquire read lock on chord handler
    let chord = state.chord.read().unwrap();

    if chord.responsible_for(&key) {
        match state.storage.read().unwrap().get(&key) {
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
    if !state.initialized.load(Ordering::Relaxed) {
        return HttpResponse::ServiceUnavailable().body("Distributed Hashtable not initialized");
    }

    // get the key from the path and hop count from headers
    let key = key.into_inner();
    let hops = req
        .headers()
        .get("X-Chord-Hop-Count")
        .and_then(|h| h.to_str().ok().and_then(|s| s.parse::<u32>().ok()))
        .unwrap_or(0);

    // Aquire read lock on chord handler
    let chord = state.chord.read().unwrap();

    if chord.responsible_for(&key) {
        let value = match std::str::from_utf8(&body) {
            Ok(v) => v.to_string(),
            Err(_) => return HttpResponse::BadRequest().body("Value must be valid UTF-8"),
        };
        state.storage.write().unwrap().put(key, value);
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
    let chord = state.chord.read().unwrap();
    // Get current node info
    let node_info = chord.nodes.to_viewmodel();
    HttpResponse::Ok().json(node_info)
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
                HttpResponse::NotImplemented().body(format!("Join with nprime: {}", addr.label()))
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
    // Return not implemented for now
    HttpResponse::NotImplemented().body("Not implemented yet")
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
    let chord = state.chord.read().unwrap();
    HttpResponse::Ok().json(chord.nodes.successor.clone())
}

// Get current node's predecessor
#[get("/internal/predecessor")]
async fn get_predecessor(state: web::Data<AppState>) -> impl Responder {
    let chord = state.chord.read().unwrap();
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

    let chord = state.chord.read().unwrap();
    let me = chord.nodes.me.clone();
    let successor = chord.nodes.successor.clone();

    // Check if id is in (n, successor]
    if in_interval_open_closed(id, me.id, successor.id) {
        return HttpResponse::Ok().json(successor);
    }

    // Otherwise, find closest preceding node and forward the request
    let n0 = chord.closest_preceding_node(id);
    if n0.addr.label() == me.addr.label() {
        let successor = chord.nodes.successor.clone();
        return HttpResponse::Ok().json(successor); // Prevent infinite loop
    }

    // Forward the request to n0
    let url = format!("{}/internal/find-successor?id={}", n0.addr.to_url(), id);        // Construct the URL
    match chord.client.get(&url).send().await {                                         // Make the GET request
        Ok(resp) => match resp.json::<Node>().await {                     // Parse the response
            Ok(node) => HttpResponse::Ok().json(node),                                  // Return the successor node
            Err(_) => HttpResponse::BadGateway().body("Error parsing successor response"),
        },
        Err(_) => HttpResponse::BadGateway().body("Error forwarding find_successor request"),   
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
    let mut chord_write = state.chord.write().unwrap();
    let predecessor = &chord_write.nodes.predecessor;

    // Check if predecessor is null (we don't have a predecessor) or n' ∈ (predecessor, n)
    if predecessor.id == chord_write.nodes.me.id || in_interval_open_open(n0.id, predecessor.id, chord_write.nodes.me.id) {
        chord_write.nodes.predecessor = n0;
    }
    HttpResponse::Ok().finish()
}

// Update the current node's successor
// Body: Node (the new successor)
#[post("/internal/set-successor")]
async fn set_successor(
    state: web::Data<AppState>,
    body: web::Json<Node>,
) -> impl Responder {
    let mut chord_write = state.chord.write().unwrap();
    chord_write.nodes.successor = body.into_inner();
    HttpResponse::Ok().finish()
}

// Update the current node's predecessor
// Body: Node (the new predecessor)
#[post("/internal/set-predecessor")]
async fn set_predecessor(
    state: web::Data<AppState>,
    body: web::Json<Node>,
) -> impl Responder {
    let mut chord_write = state.chord.write().unwrap();
    chord_write.nodes.predecessor = body.into_inner();
    HttpResponse::Ok().finish()
}
