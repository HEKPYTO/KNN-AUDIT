use crate::compare::RankSwap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;

pub const SCHEMA: &str = "knn-audit/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema: String,
    pub created_at: String,
    pub dataset: DatasetInfo,
    pub engine: EngineInfo,
    pub k: usize,
    pub summary: Summary,
    pub categories: Vec<CategorySummary>,
    pub failures: Vec<Failure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetInfo {
    pub root: String,
    pub base_sha256: String,
    pub base_rows: usize,
    pub cases: usize,
    pub dim: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInfo {
    pub name: String,
    pub version: String,
    pub exact_mode: bool,
    pub metric: String,
    pub config_redacted: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub cases: usize,
    pub passed: usize,
    pub failed: usize,
    pub errors: usize,
    pub missing: usize,
    pub extra: usize,
    pub rank_swaps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySummary {
    pub category: String,
    pub cases: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure {
    pub case_id: String,
    pub category: String,
    pub query: Vec<f32>,
    pub expected_ids: Vec<u64>,
    pub actual_ids: Vec<u64>,
    pub missing_ids: Vec<u64>,
    pub extra_ids: Vec<u64>,
    pub rank_swaps: Vec<RankSwap>,
    pub distance_gap: Option<f64>,
    pub error: Option<String>,
}

impl Report {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }

    pub fn to_html(&self) -> String {
        let mut html = String::new();
        html.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">");
        html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
        html.push_str("<title>knn-audit scorecard</title><style>");
        html.push_str("body{font:15px system-ui,sans-serif;margin:2rem;color:#18212f}");
        html.push_str("table{border-collapse:collapse;margin:.5rem 0 1.5rem}");
        html.push_str("td,th{border:1px solid #ccd3dd;padding:.35rem .6rem;text-align:left}");
        html.push_str("code{word-break:break-all}li{margin:.4rem 0}</style></head><body>");
        let _ = write!(
            html,
            "<h1>knn-audit scorecard</h1><p>{} {} &middot; {} &middot; k={}</p>",
            escape(&self.engine.name),
            escape(&self.engine.version),
            escape(&self.dataset.root),
            self.k
        );
        let _ = write!(
            html,
            "<p>base sha256: <code>{}</code><br>created: {}</p>",
            escape(&self.dataset.base_sha256),
            escape(&self.created_at)
        );
        let _ = write!(
            html,
            "<table><tr><th>cases</th><th>passed</th><th>failed</th><th>errors</th><th>missing</th><th>extra</th><th>rank swaps</th></tr>\
             <tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr></table>",
            self.summary.cases,
            self.summary.passed,
            self.summary.failed,
            self.summary.errors,
            self.summary.missing,
            self.summary.extra,
            self.summary.rank_swaps
        );
        html.push_str(
            "<h2>Categories</h2><table><tr><th>category</th><th>cases</th><th>failed</th></tr>",
        );
        for category in &self.categories {
            let _ = write!(
                html,
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape(&category.category),
                category.cases,
                category.failed
            );
        }
        html.push_str("</table><h2>Failures</h2>");
        if self.failures.is_empty() {
            html.push_str("<p>None.</p>");
        } else {
            html.push_str("<ol>");
            for failure in &self.failures {
                let _ = write!(
                    html,
                    "<li><strong>{}</strong> ({})",
                    escape(&failure.case_id),
                    escape(&failure.category)
                );
                if let Some(error) = &failure.error {
                    let _ = write!(html, "<br>error: {}", escape(error));
                }
                let _ = write!(
                    html,
                    "<br>query: <code>{}</code><br>expected: <code>{}</code><br>actual: <code>{}</code>",
                    escape(&format!("{:?}", failure.query)),
                    escape(&format!("{:?}", failure.expected_ids)),
                    escape(&format!("{:?}", failure.actual_ids))
                );
                if !failure.missing_ids.is_empty() {
                    let _ = write!(
                        html,
                        "<br>missing: <code>{}</code>",
                        escape(&format!("{:?}", failure.missing_ids))
                    );
                }
                if !failure.extra_ids.is_empty() {
                    let _ = write!(
                        html,
                        "<br>extra: <code>{}</code>",
                        escape(&format!("{:?}", failure.extra_ids))
                    );
                }
                if !failure.rank_swaps.is_empty() {
                    let _ = write!(
                        html,
                        "<br>rank swaps: <code>{}</code>",
                        escape(&format!("{:?}", failure.rank_swaps))
                    );
                }
                if let Some(gap) = failure.distance_gap {
                    let _ = write!(html, "<br>distance gap: {gap}");
                }
                html.push_str("</li>");
            }
            html.push_str("</ol>");
        }
        html.push_str("</body></html>\n");
        html
    }
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}
