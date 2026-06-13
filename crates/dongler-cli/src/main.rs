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
        /// Restrict output to a subset of 1-based pages, e.g. "45", "43-45", "1,5,9".
        #[arg(long)]
        pages: Option<String>,
    },
    /// Run the hybrid pipeline (triage + reading order + IR v2). The
    /// deterministic, model-free path; ML stages are a future opt-in.
    Convert {
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
        Commands::Extract {
            path,
            format,
            pages,
        } => extract(&path, format, pages.as_deref()),
        Commands::Convert { path, format } => convert(&path, format),
    }
}

fn convert(path: &Path, output_format: OutputFormat) -> Result<()> {
    let bytes = std::fs::read(path)
        .map_err(|e| dongler_core::DonglerError::invalid_input(format!("cannot read file: {e}")))?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("input");
    let pipeline = dongler_pipeline::Pipeline::new();
    let output = match output_format {
        OutputFormat::Markdown => pipeline.convert_to_markdown(&bytes, filename)?,
        OutputFormat::Json => pipeline.convert_to_json(&bytes, filename)?,
        OutputFormat::Latex => {
            // LaTeX still routes through the core renderer over the IR v2 doc.
            let document = pipeline.convert_bytes(&bytes, filename)?;
            document.to_latex()?
        }
    };
    println!("{output}");
    Ok(())
}

/// Parse a 1-based page spec like "45", "43-45", "1,5,9" into a membership test.
fn parse_page_spec(spec: &str) -> Result<Vec<(usize, usize)>> {
    let mut ranges = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|part| !part.is_empty()) {
        let bounds = match part.split_once('-') {
            Some((start, end)) => {
                let start = start.trim().parse::<usize>();
                let end = end.trim().parse::<usize>();
                match (start, end) {
                    (Ok(start), Ok(end)) if start >= 1 && end >= start => (start, end),
                    _ => return Err(dongler_core::DonglerError::invalid_input(format!(
                        "invalid page range: {part}"
                    ))),
                }
            }
            None => match part.parse::<usize>() {
                Ok(page) if page >= 1 => (page, page),
                _ => return Err(dongler_core::DonglerError::invalid_input(format!(
                    "invalid page number: {part}"
                ))),
            },
        };
        ranges.push(bounds);
    }
    Ok(ranges)
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

fn extract(path: &Path, output_format: OutputFormat, pages: Option<&str>) -> Result<()> {
    let mut document = load_path(path)?;
    if let Some(spec) = pages {
        let ranges = parse_page_spec(spec)?;
        document
            .pages
            .retain(|page| ranges.iter().any(|(start, end)| page.number >= *start && page.number <= *end));
    }
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
