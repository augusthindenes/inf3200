// Network helpers

use crate::chord::ChordNode;
use crate::config::HOP_LIMIT;
use crate::utils::hash_key;
use actix_web::HttpResponse;
use actix_web::web::Bytes;


// Functions for forwarding HTTP request to next node
pub async fn forward_get(
    chord: &ChordNode,
    key: &str,
    hop_count: u32,
) -> actix_web::Result<HttpResponse> {
    if hop_count >= HOP_LIMIT {
        return Ok(HttpResponse::BadGateway().body("Chord hop limit exceeded")); // Prevent infinite loops
    }

    // Hash the key to find its ID
    let key_id = hash_key(key);
    // Check if this node is responsible for the key
    if chord.responsible_for(key) {
        return Ok(HttpResponse::Ok().finish()); // Placeholder: actual value retrieval not implemented here
    }
    // Find the closest preceding node
    let next_node = chord.closest_preceding_node(key_id);
    // Construct the URL for the next node
    let url = format!("{}/storage/{}", next_node.addr.to_url(), key);

    // Forward the GET request to the next node
    let response = chord
        .client
        .get(url)
        .header("X-Chord-Hop-Count", (hop_count + 1).to_string())
        .timeout(std::time::Duration::from_millis(1000))
        .send()
        .await;

    match response {
        Ok(r) => {
            let status = actix_web::http::StatusCode::from_u16(r.status().as_u16()).unwrap();
            let body = r
                .bytes()
                .await
                .unwrap_or_else(|_| Bytes::from_static(b"Error reading body"));
            Ok(HttpResponse::build(status).body(body))
        }
        Err(e) => Ok(HttpResponse::BadGateway().body(format!("forward error: {}", e))),
    }
}

pub async fn forward_put(
    chord: &ChordNode,
    key: &str,
    value: Bytes,
    hop_count: u32,
) -> actix_web::Result<HttpResponse> {
    if hop_count >= HOP_LIMIT {
        return Ok(HttpResponse::BadGateway().body("Chord hop limit exceeded")); // Prevent infinite loops
    }

    // Hash the key to find its ID
    let key_id = hash_key(key);
    // Check if this node is responsible for the key
    if chord.responsible_for(key) {
        return Ok(HttpResponse::Ok().finish()); // Placeholder: actual value storage not implemented here
    }
    // Find the closest preceding node
    let next_node = chord.closest_preceding_node(key_id);
    // Construct the URL for the next node
    let url = format!("{}/storage/{}", next_node.addr.to_url(), key);

    // Forward the PUT request to the next node
    let response = chord
        .client
        .put(url)
        .header("X-Chord-Hop-Count", (hop_count + 1).to_string())
        .timeout(std::time::Duration::from_millis(1000))
        .body(value.clone())
        .send()
        .await;

    match response {
        Ok(r) => {
            let status = actix_web::http::StatusCode::from_u16(r.status().as_u16()).unwrap();
            let body = r.bytes().await.unwrap_or_else(|_| Bytes::from_static(b""));
            Ok(HttpResponse::build(status).body(body))
        }
        Err(e) => Ok(HttpResponse::BadGateway().body(format!("forward error: {}", e))),
    }
}