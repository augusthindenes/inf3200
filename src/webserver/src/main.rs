use actix_web::{App, HttpServer, HttpResponse, Responder, get, web};
use std::env::args;

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

// Main function to start the Actix web server
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Get the configuration
    let config = get_config();
    let server_config = config.clone(); // Clone for use in the server closure

    // Start the HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(server_config.clone()))
            .service(helloworld)
    })
    .bind((config.hostname.clone(), config.port))? // Bind to the specified hostname and port
    .run()
    .await
}