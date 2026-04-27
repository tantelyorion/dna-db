//! Exemple d'utilisation de DNA DB

use dna_db::*;

fn main() -> std::io::Result<()> {
    println!("🧬 DNA DB - Base de données évolutive");
    println!("=====================================\n");
    
    // Créer la base de données
    let db = DNADatabase::builder()
        .data_dir("./test_dna_data")
        .cache_size(100)
        .build()?;
    
    // Créer des gènes
    let gene1 = Gene::new(1, "name".to_string(), Value::String("Alice".to_string()), 0.8);
    let gene2 = Gene::new(2, "age".to_string(), Value::Int(30), 0.6);
    let gene3 = Gene::new(3, "active".to_string(), Value::Bool(true), 0.4);
    
    // Créer un chromosome
    let chromosome = Chromosome::new(1, "user_profile".to_string(), vec![1, 2, 3], 0.01);
    
    // Créer un genome
    let genome_id = db.create_genome(vec![chromosome], vec![gene1, gene2, gene3])?;
    println!("✅ Genome créé avec ID: {}", genome_id);
    
    // Lire le genome
    if let Some(genome) = db.get_genome(genome_id)? {
        println!("📖 Genome lu: génération {}, fitness: {:.2}", 
                 genome.generation, genome.fitness.value());
        
        if let Some(name_gene) = genome.get_gene_by_name("name") {
            println!("   Nom: {:?}", name_gene.value);
        }
    }
    
    // Appliquer une mutation
    let mutation = Mutation::SubstituteGene {
        genome_id,
        chromosome_id: 1,
        gene_id: 1,
        new_value: Value::String("Bob".to_string()),
    };
    
    let new_id = db.mutate(mutation)?;
    println!("🔬 Mutation appliquée, nouveau genome ID: {}", new_id);
    
    // Afficher le fitness
    db.update_fitness(new_id, Some(0.8))?;
    
    if let Some(genome) = db.get_genome(new_id)? {
        println!("📊 Nouveau fitness: {:.2}", genome.fitness.value());
        if let Some(name_gene) = genome.get_gene_by_name("name") {
            println!("   Nom après mutation: {:?}", name_gene.value);
        }
    }
    
    // Afficher les top genomes
    println!("\n🏆 Top genomes: {:?}", db.top_genomes(5));
    
    println!("\n✅ DNA DB opérationnelle !");
    
    Ok(())
}