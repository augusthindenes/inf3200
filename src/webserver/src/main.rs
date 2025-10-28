// Declare our modules
mod chord;
mod storage;
mod activity;
mod api;
mod simulate;
mod network;
mod utils;
mod config;

// Import everything we need from our modules
use storage::Storage;
use activity::ActivityTimer;

// Import everything we need from external crates
use actix_web::dev::Service;
use actix_web::{App, HttpServer, web};
use serde::Deserialize;
use std::env::args;
use std::sync::atomic::{AtomicBool};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
struct HostConfig {
    hostname: String,
    port: u16,
}

struct AppState {
    storage: RwLock<Storage>,
    chord: SharedChordHolder,
    initialized: AtomicBool,
    host_config: HostConfig,
    activity: ActivityTimer,
}

type SharedChordHolder = Arc<RwLock<Option<chord::ChordNode>>>;

#[derive(Deserialize)]
struct InitReq {
    nodes: Vec<String>, // List of known nodes in "host:port" format
}

#[derive(Deserialize)]
struct ReconfigReq {
    nodes: Vec<String>, // List of known nodes in "host:port" format
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

// Main function to start the Actix web server
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Get the configuration
    let config = get_config();
    let storage = Storage::new();
    let chord: SharedChordHolder = Arc::new(RwLock::new(None));
    let activity = ActivityTimer::new(15); // 15 minutes idle limit

    let state = web::Data::new(AppState {
        storage: RwLock::new(storage),
        chord: chord,
        initialized: AtomicBool::new(false),
        host_config: config.clone(),
        activity: activity.clone(),
    });

    // Start HTTP server and obtain a server handle
    let server = HttpServer::new(move || {
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
            .service(api::helloworld)
            .service(api::get_storage)
            .service(api::put_storage)
            .service(api::get_network)
            .service(api::post_storage_init)
            .service(api::post_reconfigure)
    })
    .bind((config.hostname.as_str(), config.port))?
    .run();

    // Background idle monitor using server handle
    let srv_handle = server.handle();
    actix_rt::spawn({
        let activity = activity.clone();
        async move {
            loop {
                actix_rt::time::sleep(std::time::Duration::from_secs(60)).await;
                if activity.is_idle() {
                    println!("No activity for 15 minutes, shutting down.");
                    srv_handle.stop(true).await;
                    break;
                }
            }
        }
    });

    // Await server termination
    server.await
}
