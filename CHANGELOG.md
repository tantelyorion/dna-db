# Changelog

## 0.2.0 — audit et réécriture

> ⚠️ Cette version a été relue et écrite sans compilateur Rust disponible : lancez
> `cargo build && cargo test` en premier et corrigez d'éventuelles erreurs de frappe.

### Bugs bloquants corrigés (le projet d'origine ne pouvait pas compiler / fonctionner)

1. `main.rs` était à la racine : jamais compilé → déplacé en `examples/basic.rs`.
2. `rand` utilisé mais absent de `Cargo.toml` → remplacé par un PRNG interne (SplitMix64).
3. `FitnessScore` utilisé comme clé de `BTreeMap` sans implémenter `Ord`, et importé depuis
   un module qui ne l'exporte pas → `Ord` total, champ privé (jamais NaN / -0.0).
4. `std_storage::load_genome` retournait une référence vers un verrou temporaire.
5. `mutate()` utilisait `child.id` après avoir déplacé `child`.
6. `MutationEngine` contenait un `ThreadRng` (`!Send`) : la base n'était pas `Sync`.
7. `mutation/delta.rs` importait un module privé (`file_format::DeltaOperation`).

### Bugs de données corrigés

8. **Relecture décalée de 8 octets** : l'en-tête était écrit deux fois mais lu une seule ;
   tout genome relu depuis le disque était corrompu → un seul en-tête, format versionné (v2).
9. **Gènes perdus** : seuls les gènes hors chromosome étaient sérialisés. Tous le sont
   maintenant, dans un ordre déterministe, avec un compteur explicite (plus de
   `while let Ok(..)` qui avalait les erreurs).
10. **Collisions d'identifiants** : le compteur repartait de 1 à chaque ouverture et écrasait
    des genomes existants → il reprend après le plus grand identifiant du disque.
11. **Version périmée** : `SmartStorage` pouvait garder deux copies (mmap et std) et relire
    l'ancienne → un seul backend, `FileStorage`.
12. **Cache LRU** : boucle infinie possible à l'éviction, `get` ne mettait pas l'ordre à jour,
    taille de cache de la config ignorée (1000 en dur) → réécrit en O(log n).
13. **Accès jamais comptés** : `get_genome` incrémentait une copie jetée.
14. **Index** : `top_fitness` triait par *identifiant* au lieu du fitness ; l'index dupliquait
    tous les genomes en mémoire ; il était vide après réouverture ; `generation_index`
    écrasait les entrées → métadonnées légères, reconstruction à l'ouverture.
15. **Statut mémoire incohérent** (Active en mémoire, Passive après rechargement).
16. `InsertChromosome` / `DeleteChromosome` n'étaient ni sérialisés ni appliqués ; insertion à
    une position hors limites → panique ; suppression d'un gène d'un autre chromosome.
17. Chaque mutation créait un enfant même si elle était invalide → erreurs explicites.
18. Config ignorée (`fitness_config`, WAL), `search_similar` et `gc` vides.
19. Signature « 16 bits » : un bit par hash aléatoire, aucune notion de proximité → SimHash
    64 bits pondéré, hash stable (indépendant de la version de Rust).

### Nouveautés

- **WAL réel** : mutations entièrement sérialisées, CRC32, replay à l'ouverture, troncature.
- **Intégrité** : CRC32 sur chaque fichier, écriture atomique, `try_new` / `validate`.
- **Lignée** : `history()`, `get_delta()`, deltas persistés pour chaque enfant.
- **API** : `evolve_random`, `search_similar`, `find_by_gene`, `gc`, `contains`, `len`, `rebuild_index`.
- **Requêtes** : `find_by_gene` s'appuie sur un index secondaire (nom de gène + valeur), maintenu à chaque mutation, suppression et `gc`.
- **CLI** : binaire `dna` (new, list, show, set, history, find, similar, evolve, top, fitness, gc).
- **Tests** : unitaires par module + `tests/integration.rs` (reopen, WAL, crash simulé…).

### Changements d'API (cassants)

- `MutationEngine::apply_mutation(&self, …) -> io::Result<(Genome, Delta)>`.
- `DNADatabaseConfig` : `mmap_threshold_bytes` et `auto_checkpoint_delta_count` supprimés
  (`memmap2` n'était pas réellement utilisé) ; ajout de `wal_sync`, `gc_threshold`,
  `rebuild_index_on_open`.
- `Mutation` gagne `InsertChromosome` et `DeleteChromosome`.

## Feuille de route (v0.3 et après)

1. **Stockage delta réel** : ne stocker que les deltas entre deux checkpoints (déjà
   persistés) et reconstruire par la chaîne ; compaction périodique.
2. **Requêtes** : `find_by_gene` utilise déjà un index secondaire ; ajouter des filtres d'intervalle (`age > 30`) et des combinaisons (ET / OU).
3. **Croisement** (`crossover(a, b)`) et **sélection** : évolution à partir de populations,
   pas seulement de mutations unitaires.
4. **LSH multi-bandes** sur la signature pour passer la similarité de O(n) à sous-linéaire.
5. **mmap** optionnel (feature `mmap`) pour les très gros genomes.
6. **Concurrence** : verrou par genome plutôt qu'un verrou d'écriture global.
7. **Serveur** : une API réseau au-dessus de la CLI (le binaire `dna` existe déjà).
8. **Fitness adaptatif** : poids appris selon l'usage réel des requêtes.
9. **Compression** des fossiles (dictionnaire de noms de gènes, zstd).
