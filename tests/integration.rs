//! Tests d'intégration : persistance, lignée, WAL, similarité.

use dna_db::storage::{WalEntry, WriteAheadLog};
use dna_db::*;
use std::path::Path;

fn open(dir: &Path) -> DNADatabase {
    DNADatabase::builder()
        .data_dir(dir)
        .wal_sync(false)
        .build()
        .unwrap()
}

fn profile(name: &str) -> (Vec<Chromosome>, Vec<Gene>) {
    let genes = vec![
        Gene::new(1, "name".to_string(), Value::from(name), 0.8),
        Gene::new(2, "age".to_string(), Value::Int(30), 0.6),
        Gene::new(3, "active".to_string(), Value::Bool(true), 0.4),
    ];
    let chromosome = Chromosome::new(1, "user_profile".to_string(), vec![1, 2, 3], 0.01);
    (vec![chromosome], genes)
}

fn name_of(db: &DNADatabase, id: GenomeId) -> Value {
    db.get_genome(id)
        .unwrap()
        .unwrap()
        .get_gene_by_name("name")
        .unwrap()
        .value
        .clone()
}

fn rename(genome_id: GenomeId, to: &str) -> Mutation {
    Mutation::SubstituteGene {
        genome_id,
        chromosome_id: 1,
        gene_id: 1,
        new_value: Value::from(to),
    }
}

#[test]
fn genome_survives_reopen_with_all_genes() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let db = open(dir.path());
        let (c, g) = profile("Alice");
        db.create_genome(c, g).unwrap()
    };

    let db = open(dir.path());
    let genome = db.get_genome(id).unwrap().unwrap();
    assert_eq!(genome.genes.len(), 3);
    assert_eq!(genome.chromosomes.len(), 1);
    assert_eq!(name_of(&db, id), Value::from("Alice"));
}

#[test]
fn mutation_creates_child_and_keeps_parent_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(dir.path());
    let (c, g) = profile("Alice");
    let parent = db.create_genome(c, g).unwrap();

    let child = db.mutate(rename(parent, "Bob")).unwrap();
    assert_ne!(child, parent);
    assert_eq!(name_of(&db, parent), Value::from("Alice"));
    assert_eq!(name_of(&db, child), Value::from("Bob"));

    let child_genome = db.get_genome(child).unwrap().unwrap();
    assert_eq!(child_genome.parent_id, Some(parent));
    assert_eq!(child_genome.generation, 2);

    assert_eq!(db.history(child).unwrap(), vec![child, parent]);
    assert_eq!(db.get_delta(child).unwrap().unwrap().operations.len(), 1);
    assert!(db.get_delta(parent).unwrap().is_none());
}

#[test]
fn invalid_mutations_are_rejected_and_write_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(dir.path());
    let (c, g) = profile("Alice");
    let id = db.create_genome(c, g).unwrap();

    let unknown_gene = Mutation::SubstituteGene {
        genome_id: id,
        chromosome_id: 1,
        gene_id: 99,
        new_value: Value::Null,
    };
    assert!(db.mutate(unknown_gene).is_err());
    assert!(db.mutate(Mutation::Noop).is_err());
    assert!(db.mutate(rename(9999, "X")).is_err());
    assert_eq!(db.len(), 1);
}

#[test]
fn inconsistent_genome_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(dir.path());
    let dangling = Chromosome::new(1, "c".to_string(), vec![42], 0.0);
    assert!(db.create_genome(vec![dangling], vec![]).is_err());
}

#[test]
fn ids_are_not_reused_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let first = {
        let db = open(dir.path());
        let (c, g) = profile("Alice");
        db.create_genome(c, g).unwrap()
    };

    let db = open(dir.path());
    let (c, g) = profile("Carol");
    let second = db.create_genome(c, g).unwrap();
    assert!(second > first);
    assert_eq!(name_of(&db, first), Value::from("Alice"));
    assert_eq!(name_of(&db, second), Value::from("Carol"));
}

#[test]
fn wal_replays_a_mutation_lost_in_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let parent = {
        let db = open(dir.path());
        let (c, g) = profile("Alice");
        db.create_genome(c, g).unwrap()
    };

    // Crash simulé : la mutation est dans le WAL mais le genome enfant n'a jamais été écrit.
    let lost_id = 500;
    {
        let wal = WriteAheadLog::new(&dir.path().join("wal"), 1 << 20, false).unwrap();
        wal.append(&WalEntry {
            sequence: 0,
            timestamp: 0,
            mutation: rename(parent, "Recovered"),
            result_id: lost_id,
        })
        .unwrap();
    }

    let db = open(dir.path());
    let recovered = db.get_genome(lost_id).unwrap().expect("genome rejoué depuis le WAL");
    assert_eq!(recovered.parent_id, Some(parent));
    assert_eq!(name_of(&db, lost_id), Value::from("Recovered"));

    // les nouveaux identifiants ne rentrent pas en collision avec l'identifiant rejoué
    let (c, g) = profile("New");
    assert!(db.create_genome(c, g).unwrap() > lost_id);
}

#[test]
fn similar_genomes_are_found_and_top_genomes_is_populated() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(dir.path());
    let (c1, g1) = profile("Alice");
    let (c2, g2) = profile("Alice");
    let (c3, g3) = profile("Zoé");
    let a = db.create_genome(c1, g1).unwrap();
    let b = db.create_genome(c2, g2).unwrap();
    let other = db.create_genome(c3, g3).unwrap();

    let identical = db.search_similar(a, 1.0).unwrap();
    assert!(identical.contains(&b));
    assert!(!identical.contains(&a));

    assert!(db.search_similar(a, 0.0).unwrap().contains(&other));

    db.update_fitness(a, Some(1.0)).unwrap();
    assert!(db.top_genomes(10).contains(&a));
    assert_eq!(db.top_genomes(1).len(), 1);
}

#[test]
fn index_is_rebuilt_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let db = open(dir.path());
        let (c, g) = profile("Alice");
        db.create_genome(c, g).unwrap()
    };
    let db = open(dir.path());
    assert_eq!(db.top_genomes(5), vec![id]);
}

#[test]
fn delete_genome_removes_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(dir.path());
    let (c, g) = profile("Alice");
    let id = db.create_genome(c, g).unwrap();
    db.delete_genome(id).unwrap();
    assert!(db.get_genome(id).unwrap().is_none());
    assert!(db.top_genomes(5).is_empty());
}

#[test]
fn random_evolution_keeps_every_generation_readable() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(dir.path());
    let (c, g) = profile("Alice");
    let mut current = db.create_genome(c, g).unwrap();
    for _ in 0..25 {
        current = db.evolve_random(current).unwrap();
    }
    let history = db.history(current).unwrap();
    assert_eq!(history.len(), 26);
    for id in history {
        assert!(db.get_genome(id).unwrap().is_some());
    }
}

#[test]
fn find_by_gene_matches_exact_values() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(dir.path());
    let (c1, g1) = profile("Alice");
    let (c2, g2) = profile("Zoé");
    let alice = db.create_genome(c1, g1).unwrap();
    let _zoe = db.create_genome(c2, g2).unwrap();

    assert_eq!(db.find_by_gene("name", &Value::from("Alice")).unwrap(), vec![alice]);

    let bob = db.mutate(rename(alice, "Bob")).unwrap();
    assert_eq!(db.find_by_gene("name", &Value::from("Bob")).unwrap(), vec![bob]);
    assert_eq!(db.find_by_gene("name", &Value::from("Alice")).unwrap(), vec![alice]);
    assert!(db.find_by_gene("name", &Value::from("Nobody")).unwrap().is_empty());
    assert!(db.find_by_gene("unknown_gene", &Value::Int(30)).unwrap().is_empty());
}

#[test]
fn find_by_gene_works_after_reopen_and_without_index_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let db = open(dir.path());
        let (c, g) = profile("Alice");
        db.create_genome(c, g).unwrap()
    };

    // index reconstruit à l'ouverture
    let db = open(dir.path());
    assert_eq!(db.find_by_gene("name", &Value::from("Alice")).unwrap(), vec![id]);
    drop(db);

    // sans reconstruction : repli sur un parcours du stockage
    let db = DNADatabase::builder()
        .data_dir(dir.path())
        .wal_sync(false)
        .rebuild_index_on_open(false)
        .build()
        .unwrap();
    assert_eq!(db.find_by_gene("name", &Value::from("Alice")).unwrap(), vec![id]);
}

#[test]
fn deleted_genomes_disappear_from_gene_queries() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(dir.path());
    let (c, g) = profile("Alice");
    let id = db.create_genome(c, g).unwrap();
    db.delete_genome(id).unwrap();
    assert!(db.find_by_gene("name", &Value::from("Alice")).unwrap().is_empty());
}
