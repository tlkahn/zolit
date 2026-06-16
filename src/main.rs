use clap::Parser;
use zolit::cli::{Cli, Command};
use zolit::{companion, error, manifest, pipeline, zotero};
use tracing_subscriber::EnvFilter;

fn main() {
    let cli = Cli::parse();

    let filter = match cli.verbose {
        0 => "zolit=warn",
        1 => "zolit=info",
        2 => "zolit=debug",
        _ => "zolit=trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .init();

    let result = match cli.command {
        Command::Sync => cmd_sync(&cli),
        Command::List => cmd_list(&cli),
        Command::Status => cmd_status(&cli),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn resolve_db_path(cli: &Cli) -> error::Result<String> {
    if let Some(ref db) = cli.db {
        return Ok(db.to_string_lossy().to_string());
    }
    let home = std::env::var("HOME")
        .map_err(|_| error::ZolitError::Other("HOME not set".into()))?;
    Ok(format!("{}/Zotero/zotero.sqlite", home))
}

fn resolve_md_dir(cli: &Cli) -> error::Result<std::path::PathBuf> {
    cli.md_dir
        .clone()
        .ok_or_else(|| error::ZolitError::Other("--md-dir is required".into()))
}

fn cmd_sync(cli: &Cli) -> error::Result<()> {
    let db_path = resolve_db_path(cli)?;
    let md_dir = resolve_md_dir(cli)?;

    let result = pipeline::sync_all(&db_path, &md_dir, cli.threshold, cli.dry_run)?;

    if cli.dry_run {
        println!("Dry run: {} entries would be processed", result.entries_processed);
    } else {
        println!(
            "Synced: {} entries processed, {} annotations inserted, {} unmatched, {} skipped",
            result.entries_processed,
            result.total_inserted,
            result.total_unmatched,
            result.total_skipped,
        );
    }

    if !result.errors.is_empty() {
        eprintln!("\nErrors:");
        for (stem, err) in &result.errors {
            eprintln!("  {}: {}", stem, err);
        }
    }

    Ok(())
}

fn cmd_list(cli: &Cli) -> error::Result<()> {
    let db_path = resolve_db_path(cli)?;

    let pdfs = zotero::db::query_all_annotated_pdfs(&db_path)?;

    if pdfs.is_empty() {
        println!("No annotated PDFs found in Zotero database.");
        return Ok(());
    }

    println!("{:<50} {:>6} {:>6}", "PDF", "AnnID", "ParID");
    println!("{}", "-".repeat(66));
    for (stem, att_id, parent_id) in &pdfs {
        println!("{:<50} {:>6} {:>6}", stem, att_id, parent_id);
    }
    println!("\n{} annotated PDFs found.", pdfs.len());

    Ok(())
}

fn cmd_status(cli: &Cli) -> error::Result<()> {
    let db_path = resolve_db_path(cli)?;
    let md_dir = resolve_md_dir(cli)?;

    let pdfs = zotero::db::query_all_annotated_pdfs(&db_path)?;
    let md_index = companion::scan_md_dir(&md_dir);
    let manifest = manifest::read_manifest(&md_dir);

    let mut matched = 0usize;
    let mut unmatched = 0usize;
    let mut pending_annotations = 0usize;

    for (stem, att_id, _parent_id) in &pdfs {
        if md_index.contains_key(&stem.to_lowercase()) {
            matched += 1;
            let anns = zotero::db::query_zotero_for_pdf(&db_path, *att_id)?;
            let manifest_ids: std::collections::HashSet<String> = manifest
                .entries
                .get(stem)
                .map(|e| e.imported_ids.iter().cloned().collect())
                .unwrap_or_default();
            let pending = anns
                .iter()
                .filter(|a| !manifest_ids.contains(&format!("zot-{}", a.item_id)))
                .count();
            pending_annotations += pending;
        } else {
            unmatched += 1;
        }
    }

    println!("Zotero: {} annotated PDFs", pdfs.len());
    println!("Matched to markdown: {}", matched);
    println!("No companion found: {}", unmatched);
    println!("Annotations pending: {}", pending_annotations);

    if manifest.last_import.is_some() {
        println!(
            "Last sync: {}",
            manifest.last_import.as_deref().unwrap_or("never")
        );
    }

    Ok(())
}
