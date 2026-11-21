use crate::dataset::Dataset;
use crate::report::{Report, SCHEMA, SCORECARD_SCHEMA};
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
    Publish {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
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
    expect_sha256: Option<String>,
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
        #[arg(long)]
        expect_sha256: Option<String>,
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
                expect_sha256,
            } => verify(dataset, sample, all, k, expect_sha256.as_deref()),
        },
        Command::Run(args) => run_command(args),
        Command::Report { input, html } => report_command(input, html),
        Command::Publish { input, out } => publish_command(input, out),
    }
}

fn verify(
    dataset: PathBuf,
    sample: Option<usize>,
    all: bool,
    k: usize,
    expect_sha256: Option<&str>,
) -> ExitCode {
    let result = Dataset::load(&dataset, expect_sha256);
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
        expect_sha256,
    } = args;
    if repeat == 0 {
        return error_exit("--repeat must be positive");
    }
    let dataset = match Dataset::load(&root, expect_sha256.as_deref()) {
        Ok(dataset) => dataset,
        Err(error) => return error_exit(error),
    };
    let faiss_python = faiss_python.unwrap_or_else(|| {
        std::env::var("KNN_AUDIT_FAISS_PYTHON").unwrap_or_else(|_| "python3".to_string())
    });
    let spec = match engine_name.as_str() {
        "reference" => EngineSpec::Reference,
        "qdrant" => EngineSpec::Qdrant {
            url: engine_url,
            collection,
        },
        "pgvector" => EngineSpec::PgVector {
            url: database_url,
            collection,
        },
        "faiss" => EngineSpec::Faiss {
            python: faiss_python,
        },
        other => {
            return error_exit(format!(
                "engine {other:?} is not available; available engines: reference, qdrant, pgvector, faiss"
            ));
        }
    };
    let engine = match spec.build() {
        Ok(engine) => engine,
        Err(error) => return error_exit(error),
    };
    let result = match run(RunOptions {
        dataset,
        engine,
        k,
        limit,
        repeat,
        jobs,
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
    if fail_on_mismatch && summary.failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

enum EngineSpec {
    Reference,
    Qdrant { url: String, collection: String },
    PgVector { url: String, collection: String },
    Faiss { python: String },
}

impl EngineSpec {
    fn build(&self) -> Result<Box<dyn Engine + Send + Sync>, String> {
        match self {
            Self::Reference => Ok(Box::new(ReferenceEngine::new())),
            Self::Qdrant { url, collection } => QdrantEngine::new(url, collection)
                .map(|engine| Box::new(engine) as Box<dyn Engine + Send + Sync>)
                .map_err(|error| format!("qdrant engine failed: {error}")),
            Self::PgVector { url, collection } => PgVectorEngine::new(url, collection)
                .map(|engine| Box::new(engine) as Box<dyn Engine + Send + Sync>)
                .map_err(|error| format!("pgvector engine failed: {error}")),
            Self::Faiss { python } => FaissEngine::new(python)
                .map(|engine| Box::new(engine) as Box<dyn Engine + Send + Sync>)
                .map_err(|error| format!("faiss engine failed: {error}")),
        }
    }
}

fn report_command(input: PathBuf, html: Option<PathBuf>) -> ExitCode {
    let report = match read_report(&input) {
        Ok(report) => report,
        Err(code) => return code,
    };
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

fn publish_command(input: PathBuf, out: PathBuf) -> ExitCode {
    let report = match read_report(&input) {
        Ok(report) => report,
        Err(code) => return code,
    };
    let json = match report.publish() {
        Ok(json) => json,
        Err(error) => return error_exit(error),
    };
    if let Err(error) = write_file(&out, &json) {
        return error_exit(error);
    }
    println!("scorecard {}: {}", SCORECARD_SCHEMA, out.display());
    ExitCode::SUCCESS
}

fn read_report(input: &Path) -> Result<Report, ExitCode> {
    let bytes = fs::read(input)
        .map_err(|error| error_exit(format!("cannot read {}: {error}", input.display())))?;
    let report: Report = serde_json::from_slice(&bytes)
        .map_err(|error| error_exit(format!("invalid report {}: {error}", input.display())))?;
    if report.schema != SCHEMA {
        return Err(error_exit(format!(
            "unsupported report schema {}",
            report.schema
        )));
    }
    Ok(report)
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
