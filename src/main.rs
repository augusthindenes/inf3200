use actix_web::{App, HttpServer, HttpResponse, Responder, get};
use std::env::args;

static mut HOSTNAME: &str = "";
static mut PORT: u16 = 0;



#[get("/helloworld")]
async fn helloworld() -> impl Responder {
    HttpResponse::Ok().body(format!("{HOSTNAME}:{PORT}"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Fetch hostname and port from command line arguments
    let args: Vec<String> = args().collect();

    for arg in args {
        println!("{arg}");
    }

    HttpServer::new(|| {
        App::new()
            .service(helloworld())
    })
    .bind((HOSTNAME, PORT))?
    .run()
    .await
}