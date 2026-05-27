use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use dongler_core::{load_path, ExtractionStatus, InputFormat, Result};

#[derive(Debug, Parser)]
#[command(
    name = "dongler",
    version,
    about = "Dongler extracts structure from messy documents."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Inspect {
        path: PathBuf,
    },
    Extract {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
        format: OutputFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Markdown,
    Json,
    Latex,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Inspect { path } => inspect(&path),
        Commands::Extract { path, format } => extract(&path, format),
    }
}

fn inspect(path: &Path) -> Result<()> {
    let format = InputFormat::detect_path(path)?;
    println!("path: {}", path.display());
    println!("format: {format}");
    println!("extraction_status: {}", extraction_status_label(format));

    if let Ok(metadata) = std::fs::metadata(path) {
        println!("size_bytes: {}", metadata.len());
        println!("is_file: {}", metadata.is_file());
    }

    Ok(())
}

fn extract(path: &Path, output_format: OutputFormat) -> Result<()> {
    let document = load_path(path)?;
    let output = match output_format {
        OutputFormat::Markdown => document.to_markdown()?,
        OutputFormat::Json => document.to_json()?,
        OutputFormat::Latex => document.to_latex()?,
    };

    println!("{output}");
    Ok(())
}

fn extraction_status_label(format: InputFormat) -> &'static str {
    match format.extraction_status() {
        ExtractionStatus::Supported => "supported",
        ExtractionStatus::Planned => "planned",
    }
}
