use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// A thread-safe storage handler using RwLock for concurrent read/write access.
// Allows multiple readers or one writer at a time.
#[derive(Clone)]
pub struct StorageHandler {
    storage: Arc<RwLock<HashMap<String, String>>>,
}

impl StorageHandler {
    // Create a new StorageHandler with an empty HashMap
    pub fn new() -> Self {
        StorageHandler {
            storage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // Get a value by key
    pub fn get(&self, key: &str) -> Option<String> {
        // Acquire a read lock to safely access the storage
        let storage = self.storage.read().unwrap();
        // Clone the value to return it
        storage.get(key).cloned()
    }

    // Put a key-value pair into the storage
    pub fn put(&self, key: String, value: String) {
        // Acquire a write lock to safely modify the storage
        let mut storage = self.storage.write().unwrap();
        storage.insert(key, value);
    }
}
