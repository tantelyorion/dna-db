//! Exemple d'utilisation de DNA DB : `cargo run --example basic`

use dna_db::*;

fn main() -> std::io::Result<()> {
    println!("🧬 DNA DB - Base de données évolutive");
    println!("=====================================\n");

    let db = DNADatabase::builder()
        .data_dir("./test_dna_data")
        .cache_size(100)
        .build()?;

    // Gènes et chromosome
    let genes = vec![
        Gene::new(1, "name".to_string(), Value::from("Alice"), 0.8),
        Gene::new(2, "age".to_string(), Value::Int(30), 0.6),
        Gene::new(3, "active".to_string(), Value::Bool(true), 0.4),
    ];
    let chromosome = Chromosome::new(1, "user_profile".to_string(), vec![1, 2, 3], 0.01);

    let genome_id = db.create_genome(vec![chromosome], genes)?;
    println!("✅ Genome créé avec ID: {}", genome_id);

    if let Some(genome) = db.get_genome(genome_id)? {
        println!("📖 Génération {}, fitness {:.2}", genome.generation, genome.fitness.value());
        if let Some(gene) = genome.get_gene_by_name("name") {
            println!("   Nom: {:?}", gene.value);
        }
    }

    // Mutation : le parent reste intact, un enfant est créé
    let new_id = db.mutate(Mutation::SubstituteGene {
        genome_id,
        chromosome_id: 1,
        gene_id: 1,
        new_value: Value::from("Bob"),
    })?;
    println!("🔬 Mutation appliquée, nouveau genome ID: {}", new_id);

    db.update_fitness(new_id, Some(0.8))?;
    if let Some(genome) = db.get_genome(new_id)? {
        println!("📊 Nouveau fitness: {:.2}", genome.fitness.value());
        if let Some(gene) = genome.get_gene_by_name("name") {
            println!("   Nom après mutation: {:?}", gene.value);
        }
    }

    // Lignée et delta
    println!("🌳 Lignée: {:?}", db.history(new_id)?);
    if let Some(delta) = db.get_delta(new_id)? {
        println!("🧾 Delta: {} opération(s)", delta.operations.len());
    }

    // Évolution automatique
    let mut current = new_id;
    for _ in 0..3 {
        current = db.evolve_random(current)?;
    }
    println!("🎲 Après 3 mutations aléatoires: genome {}", current);

    println!("🔎 Similaires à {}: {:?}", genome_id, db.search_similar(genome_id, 0.6)?);
    println!("🏆 Top genomes: {:?}", db.top_genomes(5));

    println!("\n✅ DNA DB opérationnelle !");
    Ok(())
}
