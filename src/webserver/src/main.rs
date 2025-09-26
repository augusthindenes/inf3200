// Declare our modules
mod chord_handler;
mod storage_handler;

use actix_web::{App, HttpServer, HttpResponse, Responder, get, put, web};
use std::env::args;
use crate::storage_handler::StorageHandler;

#[derive(Clone)]
struct HostConfig {
    hostname: String,
    port: u16,
}

// Fetch host configuration based on process arguments or default values
fn get_config() -> HostConfig {
    // Get the command line arguments
    let args: Vec<String> = args().collect();
    // Attempt to parse hostname from arguments, exit if not provided
    let Some(hostname) = args.get(1).cloned()
        else {
            eprintln!("hostname argument is required");
            eprintln!("Usage: {} <hostname> [port]. Example: {} localhost 8080", args[0], args[0]);
            std::process::exit(1);
        };
    // Attempt to parse port from arguments, exit if not provided or invalid
    let Some(port) = args.get(2).and_then(|p| p.parse().ok())
        else {
            eprintln!("port argument is required");
            eprintln!("Usage: {} <hostname> [port]. Example: {} localhost 8080", args[0], args[0]);
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
async fn helloworld(config: web::Data<HostConfig>) -> impl Responder {
    HttpResponse::Ok().body(format!("{}:{}", config.hostname, config.port))
}

#[get("/storage/{key}")]
async fn get_storage(key: web::Path<String>, storage: web::Data<StorageHandler>) -> impl Responder {
    match storage.get(&key) {
        Some(value) => HttpResponse::Ok().body(value),
        None => HttpResponse::NotFound().body("Key not found"),
    }
}

#[get("/network")]
async fn get_network() -> impl Responder {
    // TODO: Return list of known nodes (finger table)
    HttpResponse::Ok().body("Network info not implemented")
}

// Takes the key from the path and the value from the request body as UTF-8 string
#[put("/storage/{key}")]
async fn put_storage(key: web::Path<String>, value: String, storage: web::Data<StorageHandler>) -> impl Responder {
    storage.put(key.into_inner(), value);
    HttpResponse::Ok().body("Value stored")
}

// Main function to start the Actix web server
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Get the configuration
    let config = get_config();
    let server_config = config.clone(); // Clone for use in the server closure

    // Initialize the storage handler
    let _storage_handler = StorageHandler::new();

    // Start the HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(server_config.clone()))
            .app_data(web::Data::new(_storage_handler.clone()))
            .service(helloworld)
            .service(get_storage)
            .service(put_storage)
    })
    .bind((config.hostname.clone(), config.port))? // Bind to the specified hostname and port
    .run()
    .await
}