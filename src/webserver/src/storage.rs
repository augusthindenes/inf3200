use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// A thread-safe storage handler using RwLock for concurrent read/write access.
// Allows multiple readers or one writer at a time.
#[derive(Clone)]
pub struct Storage {
    storage: Arc<RwLock<HashMap<String, String>>>,
}

impl Storage {
    // Create a new Storage with an empty HashMap
    pub fn new() -> Self {
        Storage {
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

    // Clear all key-value pairs from storage
    pub fn clear(&self) {
        // Acquire a write lock to safely modify the storage
        let mut storage = self.storage.write().unwrap();
        storage.clear();
    }
}

// Unit tests for Storage
#[cfg(test)]
mod tests {
    use super::Storage;
    #[test]
    fn test_storage_put_get() {
        let storage = Storage::new();
        storage.put("key1".to_string(), "value1".to_string());
        let value = storage.get("key1");
        assert_eq!(value, Some("value1".to_string()));
    }
    #[test]
    fn test_storage_get_nonexistent() {
        let storage = Storage::new();
        let value = storage.get("nonexistent");
        assert_eq!(value, None);
    }
}
