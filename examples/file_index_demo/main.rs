use std::error::Error;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tuckdb::IncrementalEngine;
use tuckdb::embedding::{EmbeddingProvider, MockEmbeddingProvider};

fn main() -> Result<(), Box<dyn Error>> {
    let (dir, startup_files, top) = parse_args();
    let db_path = dir.join("vector_db.tkdb");

    println!("=== TuckDB-AI: Persistent vector DB from real files (incremental) ===");
    println!();

    let mut engine = if db_path.exists() {
        let e = IncrementalEngine::load_from(&db_path)?;
        println!(
            "[load] restored {} docs / {} vectors from {} — nothing re-processed",
            e.lifecycle().iter().count(),
            e.store().len(),
            db_path.display()
        );
        e
    } else {
        std::fs::create_dir_all(&dir)?;
        IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()))
    };
    let mut manifest = load_manifest(&dir);

    for path in &startup_files {
        add_file(&mut engine, &db_path, &dir, &mut manifest, path)?;
    }

    println!();
    println!("Commands:");
    println!("  add <path>    upload a text file and index it (only the new file is processed)");
    println!("  del <doc_id>  remove a file's vectors (only that file's data is cleaned)");
    println!("  q <text>      semantic search");
    println!("  list          show indexed files");
    println!("  exit          quit");
    println!();

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        print!("> ");
        io::stdout().flush()?;
        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (cmd, rest) = match line.split_once(char::is_whitespace) {
            Some((c, r)) => (c.to_string(), r.trim().to_string()),
            None => (line.to_string(), String::new()),
        };
        match cmd.as_str() {
            "exit" | "quit" => break,
            "add" if !rest.is_empty() => add_file(
                &mut engine,
                &db_path,
                &dir,
                &mut manifest,
                &PathBuf::from(&rest),
            )?,
            "del" if !rest.is_empty() => {
                del_doc(&mut engine, &db_path, &dir, &mut manifest, &rest)?
            }
            "q" if !rest.is_empty() => search(&engine, &manifest, &rest, top)?,
            "list" => list_files(&engine, &manifest),
            other => println!(
                "unknown command: {other} (add <path> | del <id> | q <text> | list | exit)"
            ),
        }
    }
    println!("Bye.");
    Ok(())
}

fn add_file(
    engine: &mut IncrementalEngine,
    db_path: &Path,
    dir: &Path,
    manifest: &mut Vec<(i64, PathBuf)>,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let doc_id = engine
        .lifecycle()
        .iter()
        .map(|(id, _)| id)
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    let before = engine.store().len();
    let report = engine.ingest(doc_id, &text);
    println!(
        "[add] doc {doc_id} <- {}: {} new chunks embedded ({} -> {} vectors; existing untouched)",
        path.display(),
        report.chunks,
        before,
        engine.store().len()
    );
    manifest.push((doc_id, path.to_path_buf()));
    engine.save_to(db_path)?;
    save_manifest(dir, manifest);
    println!("      vector DB saved to {}", db_path.display());
    Ok(())
}

fn del_doc(
    engine: &mut IncrementalEngine,
    db_path: &Path,
    dir: &Path,
    manifest: &mut Vec<(i64, PathBuf)>,
    id: &str,
) -> Result<(), Box<dyn Error>> {
    let doc_id: i64 = id.trim().parse().map_err(|_| "doc id must be a number")?;
    let before = engine.store().len();
    let report = engine.delete(doc_id)?;
    println!(
        "[del] doc {doc_id}: removed {} vectors directly ({} -> {}); unrelated untouched",
        report.affected_vectors, before, report.vectors_remaining
    );
    manifest.retain(|(d, _)| *d != doc_id);
    engine.save_to(db_path)?;
    save_manifest(dir, manifest);
    Ok(())
}

fn load_manifest(dir: &Path) -> Vec<(i64, PathBuf)> {
    let mut manifest = Vec::new();
    if let Ok(text) = std::fs::read_to_string(dir.join("manifest.txt")) {
        for line in text.lines() {
            if let Some((id, p)) = line.split_once('\t')
                && let Ok(id) = id.parse()
            {
                manifest.push((id, PathBuf::from(p)));
            }
        }
    }
    manifest
}

fn save_manifest(dir: &Path, manifest: &[(i64, PathBuf)]) {
    let text = manifest
        .iter()
        .map(|(id, p)| format!("{id}\t{}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.join("manifest.txt"), text).ok();
}

fn search(
    engine: &IncrementalEngine,
    manifest: &[(i64, PathBuf)],
    query: &str,
    top: usize,
) -> Result<(), Box<dyn Error>> {
    println!("Query: \"{query}\"");
    let qv = MockEmbeddingProvider::default().embed(query);
    let results = engine.store().search(&qv, top);
    for (i, m) in results.iter().enumerate() {
        let chunks = engine.chunks().get(m.document_id).unwrap();
        let snippet: String = chunks[m.chunk_id as usize]
            .content
            .chars()
            .take(120)
            .collect();
        let src = manifest
            .iter()
            .find(|(d, _)| *d == m.document_id)
            .map(|(_, p)| p.display().to_string())
            .unwrap_or_else(|| format!("doc {}", m.document_id));
        println!(
            "  #{} score={:.3} [{} chunk {}]",
            i + 1,
            m.score,
            src,
            m.chunk_id
        );
        println!("     \"{}\"", snippet.replace('\n', " | "));
    }
    Ok(())
}

fn list_files(engine: &IncrementalEngine, manifest: &[(i64, PathBuf)]) {
    println!("indexed files ({}):", manifest.len());
    for (id, path) in manifest {
        let n = engine.chunks().get(*id).map(|c| c.len()).unwrap_or(0);
        println!("  doc {id}: {} ({n} chunks)", path.display());
    }
    println!("  total vectors in vector DB: {}", engine.store().len());
}

fn parse_args() -> (PathBuf, Vec<PathBuf>, usize) {
    let args: Vec<String> = std::env::args().collect();
    let mut dir = PathBuf::from("/tmp/tuckdb_files");
    let mut files: Vec<PathBuf> = Vec::new();
    let mut top = 5usize;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" => {
                i += 1;
                dir = PathBuf::from(&args[i]);
            }
            "--file" => {
                i += 1;
                files.push(PathBuf::from(&args[i]));
            }
            "--top" => {
                i += 1;
                top = args[i].parse().expect("--top expects a number");
            }
            other => {
                eprintln!("unknown argument: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }
    (dir, files, top)
}
