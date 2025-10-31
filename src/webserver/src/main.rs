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
use chord::{NodeAddr, ChordNode};
use simulate::{CrashState, CrashSimulator};
use config::{IDLE_LIMIT, MAINTENANCE_INTERVAL_MS};

// Import everything we need from external crates
use actix_web::dev::Service;
use actix_web::{App, HttpServer, web};
use std::env::args;
use std::sync::Arc;
use tokio::sync::RwLock;

struct AppState {
    storage: RwLock<Storage>,
    chord: SharedChordHolder,
    activity: ActivityTimer,
    crash_state: Arc<CrashState>,
}

type SharedChordHolder = Arc<RwLock<ChordNode>>;

// Fetch host configuration based on process arguments
fn get_config() -> NodeAddr {
    // Get the command line arguments
    let args: Vec<String> = args().collect();
    // Attempt to parse hostname from arguments, exit if not provided
    let Some(host) = args.get(1).cloned() else {
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
    println!("Starting server at {}:{}", host, port);
    // Return the configuration
    NodeAddr { host, port }
}

// Main function to start the Actix web server
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Get the configuration
    let config = get_config();
    let storage = Storage::new();
    let chord: SharedChordHolder = Arc::new(RwLock::new(ChordNode::new(config.clone())));
    let activity = ActivityTimer::new(IDLE_LIMIT); // set idle limit from config
    let crash_state = Arc::new(CrashState::new());

    // After creating the chord node, start the maintenance tasks
    ChordNode::maintenance(
        Arc::clone(&chord),
        MAINTENANCE_INTERVAL_MS,
        Arc::clone(&crash_state),
    );

    let state = web::Data::new(AppState {
        storage: RwLock::new(storage),
        chord: chord,
        activity: activity.clone(),
        crash_state: Arc::clone(&crash_state),
    });

    // Start HTTP server and obtain a server handle
    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(CrashSimulator::new(Arc::clone(&crash_state)))
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
            .service(api::get_node_info)
            .service(api::post_join)
            .service(api::post_leave)
            .service(api::post_sim_crash)
            .service(api::post_sim_recover)
            .service(api::ping_handler)
            .service(api::get_successor)
            .service(api::get_predecessor)
            .service(api::find_successor)
            .service(api::notify)
            .service(api::set_successor)
            .service(api::set_predecessor)
            .service(api::get_known_nodes)
    })
    .bind((config.host.as_str(), config.port))?
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
