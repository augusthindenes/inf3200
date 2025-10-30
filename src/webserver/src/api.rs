use std::sync::atomic::Ordering;

use actix_web::{get, post, put, HttpRequest, HttpResponse, Responder, web};

use crate::AppState;
use crate::chord::NodeAddr;
use crate::network::{forward_get, forward_put};

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
    let nodes = chord.nodes.to_viewmodel();
    HttpResponse::Ok().json(nodes)
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
    // Return not implemented for now
    HttpResponse::NotImplemented().body("Not implemented yet")
}

#[post("/sim-recover")]
async fn post_sim_recover(state: web::Data<AppState>) -> impl Responder {
    // Return not implemented for now
    HttpResponse::NotImplemented().body("Not implemented yet")
}