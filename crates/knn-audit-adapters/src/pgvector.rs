use crate::{AdapterError, Caps, CollectionView, Engine, Hit, Metric};
use bytes::BytesMut;
use postgres::types::{Format, IsNull, ToSql, Type};
use postgres::{Client, Config, NoTls};
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
        let mut rows = String::new();
        for (index, id) in collection.ids.iter().enumerate() {
            write!(rows, "{id}\t[").expect("write to string");
            for (position, value) in collection.vector(index).iter().enumerate() {
                if position > 0 {
                    rows.push(',');
                }
                write!(rows, "{value}").expect("write to string");
            }
            rows.push_str("]\n");
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
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 63
}

fn vector_literal(values: &[f32]) -> VectorLiteral {
    let mut literal = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            literal.push(',');
        }
        write!(literal, "{value}").expect("write to string");
    }
    literal.push(']');
    VectorLiteral(literal)
}

impl Engine for PgVectorEngine {
    fn name(&self) -> &'static str {
        "pgvector"
    }

    fn version(&self) -> String {
        self.version.clone().unwrap_or_default()
    }

    fn prepare(&mut self, collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        if collection.dim == 0 || collection.vectors.len() != collection.ids.len() * collection.dim
        {
            return Err(AdapterError::new(format!(
                "count mismatch: collection has {} values, expected {} for {} vectors of dimension {}",
                collection.vectors.len(),
                collection.ids.len() * collection.dim,
                collection.ids.len(),
                collection.dim
            )));
        }
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
        if count as usize != collection.ids.len() {
            return Err(AdapterError::new(format!(
                "count mismatch: engine reports {count}, collection has {}",
                collection.ids.len()
            )));
        }
        let probe = vector_literal(&vec![0.0; collection.dim]);
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
        if !plan.iter().any(|line| line.contains("Seq Scan"))
            || plan
                .iter()
                .any(|line| line.contains("Index Scan") || line.contains("Bitmap"))
        {
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
        let literal = vector_literal(query);
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
            supports_ids: true,
        }
    }

    fn reset(&mut self) -> Result<(), AdapterError> {
        let mut client = self.connection()?;
        client
            .execute(&format!("DROP TABLE IF EXISTS {}", self.table), &[])
            .map_err(|error| AdapterError::new(format!("DROP TABLE failed: {error}")))?;
        drop(client);
        self.dim = None;
        Ok(())
    }
}
