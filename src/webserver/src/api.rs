use std::sync::atomic::Ordering;

use actix_web::{get, post, put, HttpRequest, HttpResponse, Responder, web};

use crate::{AppState, InitReq, ReconfigReq, chord::{self, NodeAddr}, network::{forward_get, forward_put}, storage::Storage};

// Define a handler for the /helloworld route
#[get("/helloworld")]
// The handler uses the HostConfig to respond with the hostname and port it is running on
async fn helloworld(state: web::Data<AppState>) -> impl Responder {
    HttpResponse::Ok().body(format!(
        "{}:{}",
        state.host_config.hostname, state.host_config.port
    ))
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
    let chord_guard = state.chord.read().unwrap();
    let chord = chord_guard.as_ref().unwrap();

    if chord.responsible_for(&key) {
        match state.storage.read().unwrap().get(&key) {
            Some(value) => HttpResponse::Ok().body(value),
            None => HttpResponse::NotFound().body("Key not found"),
        }
    } else {
        match forward_get(chord, &key, hops).await {
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
    let chord_guard = state.chord.read().unwrap();
    let chord = chord_guard.as_ref().unwrap();

    if chord.responsible_for(&key) {
        let value = match std::str::from_utf8(&body) {
            Ok(v) => v.to_string(),
            Err(_) => return HttpResponse::BadRequest().body("Value must be valid UTF-8"),
        };
        state.storage.write().unwrap().put(key, value);
        HttpResponse::Ok().body("Value stored")
    } else {
        match forward_put(chord, &key, body, hops).await {
            Ok(response) => response,
            Err(_) => HttpResponse::BadGateway().body("Error forwarding request"),
        }
    }
}

#[get("/network")]
async fn get_network(state: web::Data<AppState>) -> impl Responder {
    if !state.initialized.load(Ordering::Relaxed) {
        return HttpResponse::ServiceUnavailable().body("Distributed Hashtable not initialized");
    }
    let chord_guard = state.chord.read().unwrap();
    let chord = chord_guard.as_ref().unwrap();
    let known_nodes = chord.get_known_nodes();
    HttpResponse::Ok().json(known_nodes)
}

#[post("/storage-init")]
async fn post_storage_init(state: web::Data<AppState>, body: web::Json<InitReq>) -> impl Responder {
    if state.initialized.load(Ordering::Relaxed) {
        return HttpResponse::BadRequest().body("Node already initialized");
    }

    // Parse nodes
    let mut nodes: Vec<NodeAddr> = Vec::new();
    for node in &body.nodes {
        let mut it = node.split(':');
        let host = it.next().unwrap_or("");
        let port = it
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);
        if host.is_empty() {
            continue;
        }
        nodes.push(NodeAddr {
            host: host.to_string(),
            port,
        });
    }

    // Ensure self is in the list
    let self_addr = NodeAddr {
        host: state.host_config.hostname.clone(),
        port: state.host_config.port,
    };
    if !nodes
        .iter()
        .any(|n| n.host == self_addr.host && n.port == self_addr.port)
    {
        // If not, reject the initialization
        return HttpResponse::BadRequest().body("Initialization list must include this node");
    }

    // Build chord handler
    let chord = chord::init_chord(self_addr, nodes.clone(), None, None);
    {
        let mut chord_guard = state.chord.write().unwrap();
        *chord_guard = Some(chord);
    }

    state.initialized.store(true, Ordering::Relaxed);

    HttpResponse::Ok().body("Node initialized")
}

#[post("/reconfigure")]
async fn post_reconfigure(state: web::Data<AppState>, body: web::Json<ReconfigReq>) -> impl Responder {
    if !state.initialized.load(Ordering::Relaxed) {
        return HttpResponse::ServiceUnavailable().body("Distributed Hashtable not initialized");
    }

    // Check if our node is in the new list if provided
    // Parse nodes
    let mut nodes: Vec<NodeAddr> = Vec::new();
    for node in &body.nodes {
        let mut it = node.split(':');
        let host = it.next().unwrap_or("");
        let port = it
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);
        if host.is_empty() {
            continue;
        }
        nodes.push(NodeAddr {
            host: host.to_string(),
            port,
        });
    }

    // Ensure self is in the list
    let self_addr = NodeAddr {
        host: state.host_config.hostname.clone(),
        port: state.host_config.port,
    };
    if !nodes
        .iter()
        .any(|n| n.host == self_addr.host && n.port == self_addr.port)
    {
        // If not, reject the reconfiguration
        return HttpResponse::BadRequest().body("Reconfiguration list must include this node");
    }

    let mut chord_guard = state.chord.write().unwrap();
    let chord = chord_guard.as_mut().unwrap();

    // Rerun initialization with new parameters if provided
    let self_addr = NodeAddr {
        host: state.host_config.hostname.clone(),
        port: state.host_config.port,
    };
    let new_chord = chord::init_chord(
        self_addr,
        nodes,
        body.finger_table_size,
        body.max_nodes
    );
    *chord = new_chord;

    // Reset storage (in a real implementation, we would need to redistribute data)
    *state.storage.write().unwrap() = Storage::new();

    // We have to compile a list of all 

    HttpResponse::Ok().body("Node reconfigured")
}