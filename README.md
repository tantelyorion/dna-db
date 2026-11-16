# 🧬 DNA DB

Base de données **évolutive** écrite en Rust, inspirée de l'ADN : les données sont des
*genomes* (chromosomes → gènes) **immuables**. Chaque mutation produit un nouvel enfant, ce
qui donne un versionnement complet, une lignée, un score de *fitness* et une recherche par
similarité génétique.

## Concepts

| Biologie | Dans DNA DB |
|---|---|
| Gène | `Gene` : nom + `Value` + poids (0–1) |
| Chromosome | `Chromosome` : liste ordonnée de gènes qui mutent ensemble |
| Genome | `Genome` : une donnée vivante (parent, génération, fitness, statut) |
| Mutation | `Mutation` → nouveau genome enfant + `Delta` rejouable |
| Sélection naturelle | `FitnessCalculator` (accès, récence, pertinence, structure) |
| Mémoire Active / Passive / Fossile | `MemoryStatus`, nettoyage par `gc()` |
| Ressemblance | `GeneticSignature` (SimHash 64 bits) + `search_similar` |

## Utilisation

```rust
use dna_db::*;

let db = DNADatabase::builder().data_dir("./data").build()?;

let genes = vec![
    Gene::new(1, "name".into(), Value::from("Alice"), 0.8),
    Gene::new(2, "age".into(), Value::Int(30), 0.6),
];
let chromosome = Chromosome::new(1, "profile".into(), vec![1, 2], 0.01);
let id = db.create_genome(vec![chromosome], genes)?;

// Le parent reste intact, un enfant est créé
let child = db.mutate(Mutation::SubstituteGene {
    genome_id: id, chromosome_id: 1, gene_id: 1, new_value: Value::from("Bob"),
})?;

db.update_fitness(child, Some(0.9))?;
println!("{:?}", db.history(child)?);          // [child, id]
println!("{:?}", db.search_similar(id, 0.7)?); // genomes proches
```

Exemple complet : `cargo run --example basic` — tests : `cargo test`.

## Ligne de commande

```
cargo run --bin dna -- new name=Alice age=30     # crée le genome 1
cargo run --bin dna -- set 1 name Bob            # crée le genome enfant 2
cargo run --bin dna -- history 2                 # 2 <- 1
cargo run --bin dna -- find name Bob             # requête par valeur de gène
cargo run --bin dna -- similar 1 0.7
cargo run --bin dna -- evolve 2 5                # 5 mutations aléatoires
cargo run --bin dna -- help
```

Le dossier de données est `./dna_data` (ou `$DNA_DIR`, ou `--dir <dossier>`).

## Architecture

```
src/
├── types/      Value, Gene, Chromosome, Genome, FitnessScore, MemoryStatus
├── mutation/   Mutation, MutationEngine, Delta / DeltaOperation / DeltaBuilder
├── storage/    FileStorage (atomique + CRC32), WriteAheadLog, format de fichier
├── index/      GlobalIndex (256 shards), GenomeMeta, SimHash, GeneIndex (nom+valeur)
├── fitness/    FitnessCalculator, FitnessConfig
├── cache/      LRUCache
├── bin/dna.rs  CLI
└── db.rs       DNADatabase + builder
```

Sur disque : `checkpoints/<id>.bin` (genome complet), `deltas/<id>.delta` (mutation qui
l'a produit), `wal/segment_*.log` (journal). Tout fichier porte un en-tête `DNAD`, une
version, une longueur et un CRC32 : une corruption est détectée à la lecture.

## Garanties

- **Écriture atomique** : fichier temporaire + `fsync` + `rename`.
- **WAL** : la mutation est journalisée avant d'être écrite ; après un crash elle est
  rejouée à l'ouverture. Une entrée tronquée en fin de journal est ignorée.
- **Cohérence** : un genome incohérent (gène référencé inexistant, doublons) est refusé.
- **Thread-safe** : toutes les méthodes de `DNADatabase` prennent `&self`.
- **Zéro `unsafe`**, une seule dépendance (`parking_lot`).

Voir `CHANGELOG.md` pour les corrections de la v0.2 et la feuille de route.
