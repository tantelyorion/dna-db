//! Interface en ligne de commande de DNA DB.
//!
//! Exemples :
//!   dna new name=Alice age=30 active=true
//!   dna set 1 name Bob
//!   dna history 2
//!   dna --dir ./ma_base list

use dna_db::*;
use std::process::ExitCode;

const HELP: &str = "DNA DB - ligne de commande

USAGE:
    dna [--dir <dossier>] <commande> [arguments]

    Le dossier par défaut est $DNA_DIR, sinon ./dna_data.

COMMANDES:
    new <nom=valeur>...          Crée un genome (un chromosome « default »)
    list                         Liste les genomes
    show <id>                    Affiche un genome
    set <id> <gène> <valeur>     Mute un gène : crée un genome enfant
    history <id>                 Lignée du genome (du plus récent au plus ancien)
    find <gène> <valeur>         Genomes dont ce gène a cette valeur
    similar <id> [seuil]         Genomes proches (seuil entre 0 et 1, défaut 0.8)
    evolve <id> [n]              Applique n mutations aléatoires (défaut 1)
    top [n]                      Genomes au meilleur fitness (défaut 5)
    fitness <id>                 Recalcule et sauvegarde le fitness
    gc                           Supprime les fossiles sans enfant
    help                         Affiche cette aide

Valeurs : entier, décimal, true/false, null, sinon texte.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Erreur : {}", e);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut dir = std::env::var("DNA_DIR").unwrap_or_else(|_| "./dna_data".to_string());
    if args.len() >= 2 && args[0] == "--dir" {
        dir = args[1].clone();
        args.drain(0..2);
    }
    if args.is_empty() || args[0] == "help" || args[0] == "--help" || args[0] == "-h" {
        println!("{}", HELP);
        return Ok(());
    }

    let db = DNADatabase::builder().data_dir(&dir).build()?;
    let rest = &args[1..];

    match args[0].as_str() {
        "new" => {
            let mut genes = Vec::new();
            let mut ids = Vec::new();
            for (i, item) in rest.iter().enumerate() {
                let (name, raw) = item.split_once('=').ok_or("format attendu : nom=valeur")?;
                let id = i as u64 + 1;
                genes.push(Gene::new(id, name.to_string(), parse_value(raw), 0.5));
                ids.push(id);
            }
            if genes.is_empty() {
                return Err("donnez au moins un gène : nom=valeur".into());
            }
            let chromosome = Chromosome::new(1, "default".to_string(), ids, 0.01);
            let id = db.create_genome(vec![chromosome], genes)?;
            println!("{}", id);
        }
        "list" => {
            for id in db.list_ids() {
                if let Some(g) = db.peek_genome(id)? {
                    println!(
                        "{:>6}  gen {:<4} fitness {:.2}  {:?}  {} gène(s)",
                        g.id,
                        g.generation,
                        g.fitness.value(),
                        g.memory_status,
                        g.genes.len()
                    );
                }
            }
        }
        "show" => {
            let id = parse_id(arg(rest, 0, "show <id>")?)?;
            let genome = db.peek_genome(id)?.ok_or("genome introuvable")?;
            println!("genome {}", genome.id);
            match genome.parent_id {
                Some(p) => println!("parent      : {}", p),
                None => println!("parent      : -"),
            }
            println!("génération  : {}", genome.generation);
            println!("fitness     : {:.3} ({:?})", genome.fitness.value(), genome.memory_status);
            println!("accès       : {}", genome.access_count);
            for chromosome in &genome.chromosomes {
                println!("chromosome {} « {} »", chromosome.id, chromosome.name);
                for gene_id in &chromosome.genes {
                    if let Some(g) = genome.get_gene(*gene_id) {
                        println!("  #{} {} = {}", g.id, g.name, show_value(&g.value));
                    }
                }
            }
        }
        "set" => {
            let id = parse_id(arg(rest, 0, "set <id> <gène> <valeur>")?)?;
            let name = arg(rest, 1, "set <id> <gène> <valeur>")?;
            let raw = arg(rest, 2, "set <id> <gène> <valeur>")?;
            let genome = db.peek_genome(id)?.ok_or("genome introuvable")?;
            let (chromosome_id, gene_id) = locate_gene(&genome, name).ok_or("gène introuvable dans un chromosome")?;
            let child = db.mutate(Mutation::SubstituteGene {
                genome_id: id,
                chromosome_id,
                gene_id,
                new_value: parse_value(raw),
            })?;
            println!("{}", child);
        }
        "history" => {
            let id = parse_id(arg(rest, 0, "history <id>")?)?;
            let chain: Vec<String> = db.history(id)?.iter().map(|i| i.to_string()).collect();
            if chain.is_empty() {
                return Err("genome introuvable".into());
            }
            println!("{}", chain.join(" <- "));
        }
        "find" => {
            let name = arg(rest, 0, "find <gène> <valeur>")?;
            let raw = arg(rest, 1, "find <gène> <valeur>")?;
            for id in db.find_by_gene(name, &parse_value(raw))? {
                println!("{}", id);
            }
        }
        "similar" => {
            let id = parse_id(arg(rest, 0, "similar <id> [seuil]")?)?;
            let threshold: f32 = match rest.get(1) {
                Some(s) => s.parse().map_err(|_| "seuil invalide")?,
                None => 0.8,
            };
            for found in db.search_similar(id, threshold)? {
                println!("{}", found);
            }
        }
        "evolve" => {
            let mut id = parse_id(arg(rest, 0, "evolve <id> [n]")?)?;
            let count: usize = match rest.get(1) {
                Some(s) => s.parse().map_err(|_| "nombre invalide")?,
                None => 1,
            };
            for _ in 0..count {
                id = db.evolve_random(id)?;
                println!("{}", id);
            }
        }
        "top" => {
            let n: usize = match rest.first() {
                Some(s) => s.parse().map_err(|_| "nombre invalide")?,
                None => 5,
            };
            for id in db.top_genomes(n) {
                println!("{}", id);
            }
        }
        "fitness" => {
            let id = parse_id(arg(rest, 0, "fitness <id>")?)?;
            db.update_fitness(id, None)?;
            let genome = db.peek_genome(id)?.ok_or("genome introuvable")?;
            println!("{:.3} ({:?})", genome.fitness.value(), genome.memory_status);
        }
        "gc" => {
            println!("{} genome(s) supprimé(s)", db.gc()?);
        }
        other => {
            return Err(format!("commande inconnue : {} (essayez `dna help`)", other).into());
        }
    }
    Ok(())
}

/// Argument positionnel obligatoire.
fn arg<'a>(rest: &'a [String], index: usize, usage: &str) -> Result<&'a str, String> {
    rest.get(index)
        .map(|s| s.as_str())
        .ok_or_else(|| format!("usage : dna {}", usage))
}

fn parse_id(s: &str) -> Result<GenomeId, String> {
    s.parse::<GenomeId>().map_err(|_| format!("identifiant invalide : {}", s))
}

/// Interprète une valeur de la ligne de commande.
fn parse_value(raw: &str) -> Value {
    match raw {
        "null" => Value::Null,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            if let Ok(i) = raw.parse::<i64>() {
                Value::Int(i)
            } else if let Ok(f) = raw.parse::<f64>() {
                Value::Float(f)
            } else {
                Value::String(raw.to_string())
            }
        }
    }
}

fn show_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("{:?}", s),
        Value::Bytes(b) => format!("<{} octets>", b.len()),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
    }
}

/// Retrouve (chromosome, gène) d'un gène par son nom, dans l'ordre des chromosomes.
fn locate_gene(genome: &Genome, name: &str) -> Option<(ChromosomeId, GeneId)> {
    for chromosome in &genome.chromosomes {
        for gene_id in &chromosome.genes {
            if let Some(gene) = genome.get_gene(*gene_id) {
                if gene.name == name {
                    return Some((chromosome.id, gene.id));
                }
            }
        }
    }
    None
}
