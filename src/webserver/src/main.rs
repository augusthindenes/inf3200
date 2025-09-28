// Declare our modules
mod chord_handler;
mod storage_handler;
mod activity;

use crate::chord_handler::NodeAddr;
use crate::storage_handler::StorageHandler;
use crate::activity::ActivityTimer;
use actix_web::dev::Service;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, get, post, put, web};
use serde::Deserialize;
use std::env::args;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
struct HostConfig {
    hostname: String,
    port: u16,
}

struct AppState {
    storage: RwLock<StorageHandler>,
    chord: SharedChordHolder,
    initialized: AtomicBool,
    host_config: HostConfig,
    all_nodes: RwLock<Option<Vec<NodeAddr>>>,
    activity: ActivityTimer,
}

type SharedChordHolder = Arc<RwLock<Option<chord_handler::ChordHandler>>>;

#[derive(Deserialize)]
struct InitReq {
    nodes: Vec<String>, // List of known nodes in "host:port" format
}

#[derive(Deserialize)]
struct ReconfigReq {
    max_nodes: Option<usize>, // Optional maximum number of nodes to keep
    finger_table_size: Option<u32>, // Optional finger table size
}

// Fetch host configuration based on process arguments
fn get_config() -> HostConfig {
    // Get the command line arguments
    let args: Vec<String> = args().collect();
    // Attempt to parse hostname from arguments, exit if not provided
    let Some(hostname) = args.get(1).cloned() else {
        eprintln!("hostname argument is required");
        eprintln!(
            "Usage: {} <hostname> [port]. Example: {} localhost 8080",
            args[0], args[0]
        );
        std::process::exit(1);
    };
    // Attempt to parse port from arguments, exit if not provided or invalid
    let Some(port) = args.get(2).and_then(|p| p.parse().ok()) else {
        eprintln!("port argument is required");
        eprintln!(
            "Usage: {} <hostname> [port]. Example: {} localhost 8080",
            args[0], args[0]
        );
        std::process::exit(1);
    };
    // Log the starting configuration
    println!("Starting server at {}:{}", hostname, port);
    // Return the configuration
    HostConfig { hostname, port }
}

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
        match chord_handler::forward_get(chord, &key, hops).await {
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
        match chord_handler::forward_put(chord, &key, body, hops).await {
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
    let known_nodes = chord.get_network_info().get_known_nodes();
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
        nodes.push(self_addr.clone());
    }

    // Build chord handler
    let chord_handler = chord_handler::init_chord(self_addr, nodes.clone(), None, None);
    {
        let mut chord_guard = state.chord.write().unwrap();
        *chord_guard = Some(chord_handler);
    }

    state.initialized.store(true, Ordering::Relaxed);
    
    *state.all_nodes.write().unwrap() = Some(nodes);

    HttpResponse::Ok().body("Node initialized")
}

#[post("/reconfigure")]
async fn post_reconfigure(state: web::Data<AppState>, body: web::Json<ReconfigReq>) -> impl Responder {
    if !state.initialized.load(Ordering::Relaxed) {
        return HttpResponse::ServiceUnavailable().body("Distributed Hashtable not initialized");
    }

    let mut chord_guard = state.chord.write().unwrap();
    let chord = chord_guard.as_mut().unwrap();

    // Rerun initialization with new parameters if provided
    let all_nodes = state.all_nodes.read().unwrap();
    let nodes = all_nodes.as_ref().unwrap().clone();
    let self_addr = NodeAddr {
        host: state.host_config.hostname.clone(),
        port: state.host_config.port,
    };
    let new_chord = chord_handler::init_chord(
        self_addr,
        nodes,
        body.finger_table_size,
        body.max_nodes
    );
    *chord = new_chord;

    // Reset storage (in a real implementation, we would need to redistribute data)
    *state.storage.write().unwrap() = StorageHandler::new();

    HttpResponse::Ok().body("Node reconfigured")
}

// Main function to start the Actix web server
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Get the configuration
    let config = get_config();
    let storage_handler = StorageHandler::new();
    let chord_handler: SharedChordHolder = Arc::new(RwLock::new(None));
    let activity = ActivityTimer::new(15); // 15 minutes idle limit

    let state = web::Data::new(AppState {
        storage: RwLock::new(storage_handler),
        chord: chord_handler,
        initialized: AtomicBool::new(false),
        host_config: config.clone(),
        all_nodes: RwLock::new(None),
        activity: activity.clone(),
    });

    // Background idle monitor
    actix_rt::spawn({
        let activity = activity.clone();
        async move {
            loop {
                actix_rt::time::sleep(std::time::Duration::from_secs(60)).await;
                if activity.is_idle() {
                    println!("No activity for 15 minutes, shutting down.");
                    actix_rt::System::current().stop();
                    break;
                }
            }
        }
    });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap_fn({
                let st = state.clone();
                move |req, srv| {
                    // Touch activity timer on each request
                    st.activity.touch();
                    let fut = srv.call(req);
                    async move { fut.await }
                }
            })
            // All routes are present from start, but DHT operations return 503 if not initialized
            .service(helloworld)
            .service(get_storage)
            .service(put_storage)
            .service(get_network)
            .service(post_storage_init)
    })
    .bind((config.hostname.as_str(), config.port))?
    .run()
    .await
}
