//! Cache LRU (Least Recently Used) à capacité fixe.

use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::sync::Arc;

struct Inner<K, V> {
    capacity: usize,
    /// clé -> (valeur, tick de dernier usage)
    map: HashMap<K, (Arc<V>, u64)>,
    /// tick -> clé ; le plus petit tick est l'entrée la moins récemment utilisée
    order: BTreeMap<u64, K>,
    tick: u64,
}

/// Cache LRU thread-safe : `get` et `insert` sont en O(log n).
pub struct LRUCache<K, V> {
    inner: Mutex<Inner<K, V>>,
}

impl<K: Hash + Eq + Clone, V> LRUCache<K, V> {
    /// Crée un cache (la capacité est au minimum de 1).
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                capacity: capacity.max(1),
                map: HashMap::new(),
                order: BTreeMap::new(),
                tick: 0,
            }),
        }
    }

    /// Insère (ou remplace) une valeur ; évince les plus anciennes si la capacité est dépassée.
    pub fn insert(&self, key: K, value: V) -> Option<Arc<V>> {
        let mut guard = self.inner.lock();
        let inner = &mut *guard;
        inner.tick += 1;
        let tick = inner.tick;

        let old = inner.map.insert(key.clone(), (Arc::new(value), tick));
        if let Some((_, old_tick)) = &old {
            inner.order.remove(old_tick);
        }
        inner.order.insert(tick, key);

        while inner.map.len() > inner.capacity {
            let oldest = inner.order.keys().next().copied();
            match oldest {
                Some(t) => {
                    if let Some(k) = inner.order.remove(&t) {
                        inner.map.remove(&k);
                    }
                }
                None => break,
            }
        }
        old.map(|(v, _)| v)
    }

    /// Récupère une valeur et la marque comme récemment utilisée.
    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        let mut guard = self.inner.lock();
        let inner = &mut *guard;
        inner.tick += 1;
        let tick = inner.tick;
        match inner.map.get_mut(key) {
            Some(entry) => {
                inner.order.remove(&entry.1);
                entry.1 = tick;
                inner.order.insert(tick, key.clone());
                Some(entry.0.clone())
            }
            None => None,
        }
    }

    /// Supprime une valeur.
    pub fn remove(&self, key: &K) -> Option<Arc<V>> {
        let mut guard = self.inner.lock();
        let inner = &mut *guard;
        match inner.map.remove(key) {
            Some((value, tick)) => {
                inner.order.remove(&tick);
                Some(value)
            }
            None => None,
        }
    }

    /// Vide le cache.
    pub fn clear(&self) {
        let mut guard = self.inner.lock();
        guard.map.clear();
        guard.order.clear();
    }

    /// Nombre d'entrées.
    pub fn len(&self) -> usize {
        self.inner.lock().map.len()
    }

    /// Vrai si le cache est vide.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Capacité maximale.
    pub fn capacity(&self) -> usize {
        self.inner.lock().capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used() {
        let cache = LRUCache::new(2);
        cache.insert(1, "one");
        cache.insert(2, "two");
        assert_eq!(cache.get(&1).map(|v| *v), Some("one"));

        cache.insert(3, "three");
        assert!(cache.get(&2).is_none());
        assert_eq!(cache.get(&3).map(|v| *v), Some("three"));
        assert_eq!(cache.get(&1).map(|v| *v), Some("one"));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn reinsert_and_remove_keep_bookkeeping_consistent() {
        let cache = LRUCache::new(2);
        cache.insert(1, "a");
        cache.insert(1, "b");
        cache.insert(2, "c");
        assert_eq!(cache.len(), 2);
        assert!(cache.remove(&1).is_some());
        assert!(cache.remove(&1).is_none());
        cache.insert(3, "d");
        cache.insert(4, "e");
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&2).is_none());
    }

    #[test]
    fn capacity_zero_is_clamped() {
        let cache = LRUCache::new(0);
        cache.insert(1, 1);
        assert_eq!(cache.capacity(), 1);
        assert_eq!(cache.len(), 1);
    }
}
