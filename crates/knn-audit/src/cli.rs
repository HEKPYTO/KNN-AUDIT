use crate::dataset::{Dataset, EXPECTED_BASE_SHA256};
use crate::report::{Report, SCHEMA};
use crate::run::{RunOptions, run};
use clap::{Args, Parser, Subcommand};
use knn_audit_adapters::{Engine, FaissEngine, PgVectorEngine, QdrantEngine, ReferenceEngine};
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "knn-audit", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Dataset {
        #[command(subcommand)]
        command: DatasetCommand,
    },
    Run(RunArgs),
    Report {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        html: Option<PathBuf>,
    },
}

#[derive(Args)]
struct RunArgs {
    #[arg(long)]
    dataset: PathBuf,
    #[arg(long)]
    engine: String,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    html: Option<PathBuf>,
    #[arg(long)]
    fail_on_mismatch: bool,
    #[arg(long)]
    jobs: Option<usize>,
    #[arg(long, default_value_t = 10)]
    k: usize,
    #[arg(long, default_value = "http://127.0.0.1:6333")]
    engine_url: String,
    #[arg(
        long,
        default_value = "postgres://postgres:postgres@127.0.0.1:5432/postgres"
    )]
    database_url: String,
    #[arg(long, default_value = "errpedia_audit")]
    collection: String,
    #[arg(long, default_value_t = 1)]
    repeat: usize,
    #[arg(long)]
    faiss_python: Option<String>,
}

#[derive(Subcommand)]
enum DatasetCommand {
    Verify {
        #[arg(long)]
        dataset: PathBuf,
        #[arg(long, conflicts_with = "all")]
        sample: Option<usize>,
        #[arg(long)]
        all: bool,
        #[arg(long, default_value_t = 10)]
        k: usize,
    },
}

pub fn main_entry() -> ExitCode {
    let args = Cli::parse();
    match args.command {
        Command::Dataset { command } => match command {
            DatasetCommand::Verify {
                dataset,
                sample,
                all,
                k,
            } => verify(dataset, sample, all, k),
        },
        Command::Run(args) => run_command(args),
        Command::Report { input, html } => report_command(input, html),
    }
}

fn verify(dataset: PathBuf, sample: Option<usize>, all: bool, k: usize) -> ExitCode {
    let result = load(&dataset);
    let dataset = match result {
        Ok(dataset) => dataset,
        Err(error) => return error_exit(error),
    };
    let total = dataset.cases.len();
    let indices: Vec<usize> = if all {
        (0..total).collect()
    } else {
        let count = sample.unwrap_or(100).min(total);
        if count == 0 {
            return error_exit("sample size must be positive");
        }
        (0..count).map(|index| index * total / count).collect()
    };
    let summary = match dataset.verify(&indices, k) {
        Ok(summary) => summary,
        Err(error) => return error_exit(error),
    };
    println!(
        "dataset verify: {}/{} cases mismatch (sha256 {})",
        summary.mismatches.len(),
        summary.cases,
        dataset.base_sha256
    );
    for case_id in &summary.mismatches {
        println!("  mismatch: {case_id}");
    }
    if summary.mismatches.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_command(args: RunArgs) -> ExitCode {
    let RunArgs {
        dataset: root,
        engine: engine_name,
        limit,
        out,
        html,
        fail_on_mismatch,
        jobs,
        k,
        engine_url,
        database_url,
        collection,
        faiss_python,
        repeat,
    } = args;
    if repeat == 0 {
        return error_exit("--repeat must be positive");
    }
    if let Some(count) = jobs {
        if count == 0 {
            return error_exit("--jobs must be positive");
        }
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(count)
            .build_global();
    }
    let dataset = match load(&root) {
        Ok(dataset) => dataset,
        Err(error) => return error_exit(error),
    };
    let faiss_python = faiss_python.unwrap_or_else(|| {
        std::env::var("KNN_AUDIT_FAISS_PYTHON").unwrap_or_else(|_| "python3".to_string())
    });
    let engine = match build_engine(
        &engine_name,
        &engine_url,
        &database_url,
        &collection,
        &faiss_python,
    ) {
        Ok(engine) => engine,
        Err(error) => return error_exit(error),
    };
    let result = match run(RunOptions {
        dataset,
        engine,
        k,
        limit,
        repeat,
    }) {
        Ok(result) => result,
        Err(error) => return error_exit(error),
    };
    println!("category cases failed");
    for category in &result.report.categories {
        println!(
            "{} {} {}",
            category.category, category.cases, category.failed
        );
    }
    let out = out.unwrap_or_else(|| PathBuf::from("results").join(format!("{engine_name}.json")));
    if let Err(error) = write_file(
        &out,
        &result.report.to_json().expect("report serialization"),
    ) {
        return error_exit(error);
    }
    println!("report: {}", out.display());
    if let Some(html) = html {
        if let Err(error) = write_file(&html, &result.report.to_html()) {
            return error_exit(error);
        }
        println!("html: {}", html.display());
    }
    println!("{}", result.summary_line);
    let summary = &result.report.summary;
    if fail_on_mismatch && (summary.missing > 0 || summary.extra > 0 || summary.errors > 0) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn build_engine(
    name: &str,
    engine_url: &str,
    database_url: &str,
    collection: &str,
    faiss_python: &str,
) -> Result<Box<dyn Engine + Send + Sync>, String> {
    match name {
        "reference" => Ok(Box::new(ReferenceEngine::new())),
        "qdrant" => QdrantEngine::new(engine_url, collection)
            .map(|engine| Box::new(engine) as Box<dyn Engine + Send + Sync>)
            .map_err(|error| format!("qdrant prepare failed: {error}")),
        "pgvector" => PgVectorEngine::new(database_url, collection)
            .map(|engine| Box::new(engine) as Box<dyn Engine + Send + Sync>)
            .map_err(|error| format!("pgvector connection failed: {error}")),
        "faiss" => FaissEngine::new(faiss_python)
            .map(|engine| Box::new(engine) as Box<dyn Engine + Send + Sync>)
            .map_err(|error| format!("faiss prepare failed: {error}")),
        other => Err(format!(
            "engine {other:?} is not available; available engines: reference, qdrant, pgvector, faiss"
        )),
    }
}

fn report_command(input: PathBuf, html: Option<PathBuf>) -> ExitCode {
    let bytes = match fs::read(&input) {
        Ok(bytes) => bytes,
        Err(error) => return error_exit(format!("cannot read {}: {error}", input.display())),
    };
    let report: Report = match serde_json::from_slice(&bytes) {
        Ok(report) => report,
        Err(error) => return error_exit(format!("invalid report {}: {error}", input.display())),
    };
    if report.schema != SCHEMA {
        return error_exit(format!("unsupported report schema {}", report.schema));
    }
    if let Some(html) = html {
        if let Err(error) = write_file(&html, &report.to_html()) {
            return error_exit(error);
        }
        println!("html: {}", html.display());
    }
    println!(
        "report {}: {}/{} failures",
        report.schema, report.summary.failed, report.summary.cases
    );
    ExitCode::SUCCESS
}

fn load(root: &Path) -> Result<Dataset, crate::dataset::DatasetError> {
    let dataset = Dataset::load(root)?;
    if dataset.is_shipped() {
        dataset.check_checksum(EXPECTED_BASE_SHA256)?;
    }
    Ok(dataset)
}

fn write_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

fn error_exit(message: impl Display) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(2)
}
