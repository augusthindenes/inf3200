use actix_web::{App, HttpServer, HttpResponse, Responder, get, web};
use std::env::args;

#[derive(Clone)]
struct HostConfig {
    hostname: String,
    port: u16,
}

// Fetch host configuration based on process arguments or default values
fn get_config() -> HostConfig {
    let args: Vec<String> = args().collect();
    let Some(hostname) = args.get(1).cloned()
        else {
            eprintln!("hostname argument is required");
            eprintln!("Usage: {} <hostname> [port]. Example: {} localhost 8080", args[0], args[0]);
            std::process::exit(1);
        };
    let Some(port) = args.get(2).and_then(|p| p.parse().ok())
        else {
            eprintln!("port argument is required");
            eprintln!("Usage: {} <hostname> [port]. Example: {} localhost 8080", args[0], args[0]);
            std::process::exit(1);
        };
    println!("Starting server at {}:{}", hostname, port);
    HostConfig { hostname, port }
}
#[get("/helloworld")]
async fn helloworld(config: web::Data<HostConfig>) -> impl Responder {
    HttpResponse::Ok().body(format!("{}:{}", config.hostname, config.port))
}
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = get_config();
    let server_config = config.clone();

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(server_config.clone()))
            .service(helloworld)
    })
    .bind((config.hostname.clone(), config.port))?
    .run()
    .await
}