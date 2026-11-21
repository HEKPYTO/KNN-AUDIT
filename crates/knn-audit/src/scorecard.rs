use crate::compare::RankSwap;
use crate::report::{CategorySummary, Report, RunSummary, Summary};
use serde::Serialize;

pub const SCORECARD_SCHEMA: &str = "knn-audit-scorecard/v1";

#[derive(Serialize)]
pub struct Scorecard {
    schema: &'static str,
    created_at: String,
    dataset: ScorecardDataset,
    engine: ScorecardEngine,
    k: usize,
    repeat: usize,
    runs: Vec<RunSummary>,
    summary: Summary,
    categories: Vec<CategorySummary>,
    failures: Vec<ScorecardFailure>,
}

#[derive(Serialize)]
struct ScorecardDataset {
    base_sha256: String,
    base_rows: usize,
    cases: usize,
    dim: usize,
}

#[derive(Serialize)]
struct ScorecardEngine {
    name: String,
    version: String,
    exact_mode: bool,
    metric: String,
}

#[derive(Serialize)]
struct ScorecardFailure {
    case_id: String,
    category: String,
    expected_ids: Vec<u64>,
    actual_ids: Vec<u64>,
    missing_ids: Vec<u64>,
    extra_ids: Vec<u64>,
    rank_swaps: Vec<RankSwap>,
    distance_gap: Option<f64>,
    error: Option<String>,
    failed_repetitions: Vec<usize>,
    unstable_actual_ids: Vec<Vec<u64>>,
}

impl From<Report> for Scorecard {
    fn from(report: Report) -> Self {
        let Report {
            created_at,
            dataset,
            engine,
            k,
            repeat,
            runs,
            summary,
            mut categories,
            mut failures,
            ..
        } = report;
        categories.sort_by(|a, b| a.category.cmp(&b.category));
        failures.sort_by(|a, b| {
            a.case_id
                .cmp(&b.case_id)
                .then_with(|| a.category.cmp(&b.category))
        });
        Self {
            schema: SCORECARD_SCHEMA,
            created_at,
            dataset: ScorecardDataset {
                base_sha256: dataset.base_sha256,
                base_rows: dataset.base_rows,
                cases: dataset.cases,
                dim: dataset.dim,
            },
            engine: ScorecardEngine {
                name: engine.name,
                version: engine.version,
                exact_mode: engine.exact_mode,
                metric: engine.metric,
            },
            k,
            repeat,
            runs,
            summary,
            categories,
            failures: failures
                .into_iter()
                .map(|failure| ScorecardFailure {
                    case_id: failure.case_id,
                    category: failure.category,
                    expected_ids: failure.expected_ids,
                    actual_ids: failure.actual_ids,
                    missing_ids: failure.missing_ids,
                    extra_ids: failure.extra_ids,
                    rank_swaps: failure.rank_swaps,
                    distance_gap: failure.distance_gap,
                    error: failure.error,
                    failed_repetitions: failure.failed_repetitions,
                    unstable_actual_ids: failure.unstable_actual_ids,
                })
                .collect(),
        }
    }
}

impl Scorecard {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }
}
