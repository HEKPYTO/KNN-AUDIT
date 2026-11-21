use crate::{
    AdapterError, Caps, CollectionView, Engine, Hit, Metric, id_string, valid_name,
    validate_collection, vector_text, verify_count,
};
use bytes::BytesMut;
use postgres::types::{Format, IsNull, ToSql, Type};
use postgres::{Client, Config, NoTls};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct VectorLiteral(String);

impl ToSql for VectorLiteral {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        if ty.name() != "vector" {
            return Err(format!("expected vector parameter, got {}", ty.name()).into());
        }
        out.extend_from_slice(self.0.as_bytes());
        Ok(IsNull::No)
    }

    fn accepts(ty: &Type) -> bool {
        ty.name() == "vector"
    }

    fn to_sql_checked(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        self.to_sql(ty, out)
    }

    fn encode_format(&self, _ty: &Type) -> Format {
        Format::Text
    }
}

pub struct PgVectorEngine {
    clients: Vec<Mutex<Client>>,
    next: AtomicUsize,
    table: String,
    version: Option<String>,
    dim: Option<usize>,
}

impl PgVectorEngine {
    pub fn new(url: impl AsRef<str>, table: impl Into<String>) -> Result<Self, AdapterError> {
        let table = table.into();
        if !valid_name(&table) {
            return Err(AdapterError::new("invalid pgvector table name"));
        }
        let config: Config = url
            .as_ref()
            .parse()
            .map_err(|error| AdapterError::new(format!("invalid database URL: {error}")))?;
        let size = std::thread::available_parallelism()
            .map_or(1, |count| count.get())
            .min(8);
        let mut clients = Vec::with_capacity(size);
        for _ in 0..size {
            let mut client = config.connect(NoTls).map_err(|error| {
                AdapterError::new(format!("postgres connection failed: {error}"))
            })?;
            client
                .batch_execute("SET enable_indexscan=off; SET enable_bitmapscan=off;")
                .map_err(|error| AdapterError::new(format!("postgres setup failed: {error}")))?;
            clients.push(Mutex::new(client));
        }
        Ok(Self {
            clients,
            next: AtomicUsize::new(0),
            table,
            version: None,
            dim: None,
        })
    }

    fn connection(&self) -> Result<std::sync::MutexGuard<'_, Client>, AdapterError> {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.clients.len();
        self.clients[index]
            .lock()
            .map_err(|_| AdapterError::new("postgres client lock is poisoned"))
    }

    fn load(
        &self,
        client: &mut Client,
        collection: &CollectionView<'_>,
    ) -> Result<(), AdapterError> {
        if let Some(&id) = collection.ids.iter().find(|&&id| id > i64::MAX as u64) {
            return Err(AdapterError::new(format!(
                "id {} exceeds i64::MAX and cannot be stored in pgvector",
                id_string(id)
            )));
        }
        let mut rows = String::new();
        for (index, id) in collection.ids.iter().enumerate() {
            writeln!(
                rows,
                "{}\t{}",
                id_string(*id),
                vector_text(collection.vector(index))
            )
            .expect("write to string");
        }
        let mut copy = client
            .copy_in(&format!("COPY {} (id, embedding) FROM STDIN", self.table))
            .map_err(|error| AdapterError::new(format!("postgres COPY failed: {error}")))?;
        copy.write_all(rows.as_bytes())
            .map_err(|error| AdapterError::new(format!("postgres COPY write failed: {error}")))?;
        copy.finish()
            .map_err(|error| AdapterError::new(format!("postgres COPY finish failed: {error}")))?;
        Ok(())
    }

    pub fn cleanup(&self) -> Result<(), AdapterError> {
        let mut client = self.connection()?;
        client
            .execute(&format!("DROP TABLE IF EXISTS {}", self.table), &[])
            .map_err(|error| AdapterError::new(format!("DROP TABLE failed: {error}")))?;
        Ok(())
    }
}

fn plan_is_sequential(plan: &[String]) -> bool {
    plan.iter().any(|line| line.contains("Seq Scan"))
        && !plan
            .iter()
            .any(|line| line.contains("Index Scan") || line.contains("Bitmap"))
}

impl Engine for PgVectorEngine {
    fn name(&self) -> &'static str {
        "pgvector"
    }

    fn version(&self) -> String {
        self.version.clone().unwrap_or_default()
    }

    fn config(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("plan".to_string(), "seq_scan".to_string()),
            ("enable_indexscan".to_string(), "off".to_string()),
            ("enable_bitmapscan".to_string(), "off".to_string()),
            ("order".to_string(), "distance,id".to_string()),
        ])
    }

    fn prepare(&mut self, collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        validate_collection(collection)?;
        let mut client = self.connection()?;
        client
            .batch_execute("CREATE EXTENSION IF NOT EXISTS vector")
            .map_err(|error| AdapterError::new(format!("CREATE EXTENSION failed: {error}")))?;
        let version: String = client
            .query_one(
                "SELECT extversion FROM pg_extension WHERE extname = 'vector'",
                &[],
            )
            .map_err(|error| AdapterError::new(format!("pgvector version failed: {error}")))?
            .get(0);
        client
            .execute(&format!("DROP TABLE IF EXISTS {}", self.table), &[])
            .map_err(|error| AdapterError::new(format!("DROP TABLE failed: {error}")))?;
        client
            .execute(
                &format!(
                    "CREATE TABLE {} (id bigint PRIMARY KEY, embedding vector({}))",
                    self.table, collection.dim
                ),
                &[],
            )
            .map_err(|error| AdapterError::new(format!("CREATE TABLE failed: {error}")))?;
        self.load(&mut client, collection)?;
        client
            .execute(&format!("ANALYZE {}", self.table), &[])
            .map_err(|error| AdapterError::new(format!("ANALYZE failed: {error}")))?;
        let count: i64 = client
            .query_one(&format!("SELECT count(*) FROM {}", self.table), &[])
            .map_err(|error| AdapterError::new(format!("count query failed: {error}")))?
            .get(0);
        verify_count(count as u64, collection.ids.len())?;
        let probe = VectorLiteral(vector_text(&vec![0.0; collection.dim]));
        let plan: Vec<String> = client
            .query(
                &format!(
                    "EXPLAIN SELECT id, embedding <-> $1::vector AS d FROM {} ORDER BY d, id LIMIT $2",
                    self.table
                ),
                &[&probe, &2_i64],
            )
            .map_err(|error| AdapterError::new(format!("EXPLAIN failed: {error}")))?
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect();
        if !plan_is_sequential(&plan) {
            return Err(AdapterError::new(format!(
                "pgvector plan is not a sequential scan: {}",
                plan.join("; ")
            )));
        }
        drop(client);
        self.version = Some(version);
        self.dim = Some(collection.dim);
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> Result<Vec<Hit>, AdapterError> {
        let dim = self
            .dim
            .ok_or_else(|| AdapterError::new("pgvector is not prepared"))?;
        if query.len() != dim {
            return Err(AdapterError::new(format!(
                "query dimension {} does not match collection dimension {dim}",
                query.len()
            )));
        }
        if k == 0 {
            return Ok(Vec::new());
        }
        let literal = VectorLiteral(vector_text(query));
        let mut client = self.connection()?;
        let rows = client
            .query(
                &format!(
                    "SELECT id, embedding <-> $1::vector AS d FROM {} ORDER BY d, id LIMIT $2",
                    self.table
                ),
                &[&literal, &(k as i64)],
            )
            .map_err(|error| AdapterError::new(format!("pgvector query failed: {error}")))?;
        rows.into_iter()
            .map(|row| {
                Ok(Hit {
                    id: row.get::<_, i64>(0).to_string(),
                    distance: Some(row.get::<_, f64>(1)),
                })
            })
            .collect()
    }

    fn capabilities(&self) -> Caps {
        Caps {
            exact_mode: true,
            distance_metric: Metric::L2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::plan_is_sequential;

    #[test]
    fn plan_is_sequential_rejects_index_scan() {
        let plan = vec![
            "Limit".to_string(),
            "Index Scan using audit_pkey on audit".to_string(),
        ];
        assert!(!plan_is_sequential(&plan));
        assert!(plan_is_sequential(&["Seq Scan on audit".to_string()]));
    }
}
