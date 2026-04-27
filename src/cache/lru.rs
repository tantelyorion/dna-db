//! Cache LRU (Least Recently Used) maison

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use parking_lot::RwLock;

/// Cache LRU avec capacité fixe
pub struct LRUCache<K, V> {
    inner: RwLock<LRUCacheInner<K, V>>,
}

struct LRUCacheInner<K, V> {
    capacity: usize,
    map: HashMap<K, (Arc<V>, usize)>,
    lru_list: Vec<K>,
    next_order: usize,
}

impl<K: Hash + Eq + Clone, V> LRUCache<K, V> {
    /// Crée un nouveau cache avec une capacité donnée
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: RwLock::new(LRUCacheInner {
                capacity,
                map: HashMap::new(),
                lru_list: Vec::new(),
                next_order: 0,
            }),
        }
    }

    /// Insère une valeur dans le cache
    pub fn insert(&self, key: K, value: V) -> Option<Arc<V>> {
        let mut inner = self.inner.write();
        
        let order = inner.next_order;
        inner.next_order += 1;
        
        let old = inner.map.insert(key.clone(), (Arc::new(value), order));
        inner.lru_list.push(key);
        
        // Éviction si nécessaire
        while inner.map.len() > inner.capacity {
            if let Some(oldest_key) = inner.lru_list.first() {
                if let Some((_, old_order)) = inner.map.get(oldest_key) {
                    // Garder le plus récent
                    let keep = inner.map.values().any(|(_, o)| *o > *old_order);
                    if !keep {
                        if let Some(key_to_remove) = inner.lru_list.first().cloned() {
                            inner.map.remove(&key_to_remove);
                            inner.lru_list.remove(0);
                        }
                    }
                }
            }
        }
        
        old.map(|(v, _)| v)
    }

    /// Récupère une valeur du cache
    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        let inner = self.inner.read();
        inner.map.get(key).map(|(v, _)| v.clone())
    }

    /// Supprime une valeur du cache
    pub fn remove(&self, key: &K) -> Option<Arc<V>> {
        let mut inner = self.inner.write();
        inner.map.remove(key).map(|(v, _)| v)
    }

    /// Vide le cache
    pub fn clear(&self) {
        let mut inner = self.inner.write();
        inner.map.clear();
        inner.lru_list.clear();
    }

    /// Retourne la taille actuelle
    pub fn len(&self) -> usize {
        self.inner.read().map.len()
    }

    /// Vérifie si le cache est vide
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_cache() {
        let cache = LRUCache::new(2);
        
        cache.insert(1, "one");
        cache.insert(2, "two");
        
        assert_eq!(cache.get(&1).map(|v| *v), Some("one"));
        
        cache.insert(3, "three");
        
        // La clé 2 devrait être évincée
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&3).map(|v| *v), Some("three"));
    }
}