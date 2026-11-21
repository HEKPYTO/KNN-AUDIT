use crate::{AdapterError, CollectionView};
use num_bigint::BigInt;
use num_traits::Zero;
use rayon::prelude::*;
use std::cmp::Ordering;

const SCREENING_FACTOR: f64 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactDistance {
    pub value: BigInt,
    pub exponent: i32,
}

impl ExactDistance {
    pub fn zero() -> Self {
        Self {
            value: BigInt::zero(),
            exponent: 0,
        }
    }

    pub fn cmp_f64(&self, other: f64) -> Ordering {
        assert!(!other.is_nan(), "cannot compare exact distance with NaN");
        if other < 0.0 {
            return Ordering::Greater;
        }
        if other == 0.0 {
            return if self.value.is_zero() {
                Ordering::Equal
            } else {
                Ordering::Greater
            };
        }
        if other.is_infinite() {
            return Ordering::Less;
        }
        let bits = other.to_bits();
        let biased = ((bits >> 52) & 0x7ff) as i32;
        let fraction = bits & ((1_u64 << 52) - 1);
        let (mantissa, exponent) = if biased == 0 {
            (fraction, -1074)
        } else {
            (fraction | (1_u64 << 52), biased - 1075)
        };
        let base = (2 * self.exponent).min(exponent);
        let left = &self.value << ((2 * self.exponent - base) as u32);
        let right = BigInt::from(mantissa) << ((exponent - base) as u32);
        left.cmp(&right)
    }
}

impl Ord for ExactDistance {
    fn cmp(&self, other: &Self) -> Ordering {
        let base = self.exponent.min(other.exponent);
        let left = &self.value << ((2 * (self.exponent - base)) as u32);
        let right = &other.value << ((2 * (other.exponent - base)) as u32);
        left.cmp(&right)
    }
}

impl PartialOrd for ExactDistance {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn decode(value: f32) -> (i64, i32) {
    let bits = value.to_bits();
    let sign = if bits >> 31 == 0 { 1 } else { -1 };
    let biased = ((bits >> 23) & 0xff) as i32;
    let fraction = (bits & 0x7f_ffff) as i64;
    if biased == 0 {
        (sign * fraction, -149)
    } else {
        (sign * (fraction | (1 << 23)), biased - 150)
    }
}

fn gamma(operations: usize) -> f64 {
    let bound = (operations as f64) * (f64::EPSILON / 2.0);
    bound / (1.0 - bound)
}

fn screening_relative(dim: usize) -> f64 {
    SCREENING_FACTOR * gamma(4 * dim)
}

pub fn squared_l2_f64(query: &[f32], vector: &[f32]) -> f64 {
    assert_eq!(query.len(), vector.len(), "dimension mismatch");
    let mut sums = [0.0_f64; 4];
    let mut index = 0;
    while index + 4 <= query.len() {
        for lane in 0..4 {
            let delta = f64::from(query[index + lane]) - f64::from(vector[index + lane]);
            sums[lane] += delta * delta;
        }
        index += 4;
    }
    let mut total = (sums[0] + sums[1]) + (sums[2] + sums[3]);
    while index < query.len() {
        let delta = f64::from(query[index]) - f64::from(vector[index]);
        total += delta * delta;
        index += 1;
    }
    total
}

pub fn squared_l2_exact(query: &[f32], vector: &[f32]) -> ExactDistance {
    assert_eq!(query.len(), vector.len(), "dimension mismatch");
    let mut base = i32::MAX;
    for index in 0..query.len() {
        let (_, query_exp) = decode(query[index]);
        let (_, vector_exp) = decode(vector[index]);
        base = base.min(query_exp.min(vector_exp));
    }
    let mut sum = BigInt::zero();
    for index in 0..query.len() {
        let (query_mantissa, query_exp) = decode(query[index]);
        let (vector_mantissa, vector_exp) = decode(vector[index]);
        let left = BigInt::from(query_mantissa) << ((query_exp - base) as u32);
        let right = BigInt::from(vector_mantissa) << ((vector_exp - base) as u32);
        let delta = left - right;
        sum += &delta * &delta;
    }
    ExactDistance {
        value: sum,
        exponent: base,
    }
}

pub fn top_k_exact(
    query: &[f32],
    collection: &CollectionView<'_>,
    k: usize,
) -> Result<Vec<u64>, AdapterError> {
    if collection.dim == 0 {
        return Ok(Vec::new());
    }
    if query.len() != collection.dim {
        return Err(AdapterError::new(format!(
            "query dimension {} does not match collection dimension {}",
            query.len(),
            collection.dim
        )));
    }
    if collection.vectors.len() != collection.ids.len() * collection.dim {
        return Err(AdapterError::new(format!(
            "collection size mismatch: {} values for {} vectors of dimension {}",
            collection.vectors.len(),
            collection.ids.len(),
            collection.dim
        )));
    }
    let count = collection.ids.len();
    if count == 0 || k == 0 {
        return Ok(Vec::new());
    }
    let distances: Vec<f64> = collection
        .vectors
        .par_chunks_exact(collection.dim)
        .map(|vector| squared_l2_f64(query, vector))
        .collect();
    let mut ordered = distances.clone();
    let rank = k.min(count) - 1;
    let threshold = {
        let (_, kth, _) = ordered.select_nth_unstable_by(rank, |left, right| left.total_cmp(right));
        *kth
    };
    let limit = if threshold == 0.0 {
        0.0
    } else {
        threshold * (1.0 + screening_relative(collection.dim))
            / (1.0 - screening_relative(collection.dim))
    };
    let mut candidates: Vec<(u64, ExactDistance)> = Vec::new();
    for index in 0..count {
        let distance = distances[index];
        if distance > limit {
            continue;
        }
        let exact = if distance == 0.0 {
            ExactDistance::zero()
        } else {
            squared_l2_exact(query, collection.vector(index))
        };
        candidates.push((collection.ids[index], exact));
    }
    candidates.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
    candidates.truncate(k.min(count));
    Ok(candidates.into_iter().map(|(id, _)| id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f64_interval_contains_exact_two_sided() {
        let mut state = 0x1234_5678_9abc_def0_u64;
        for case in 0..500 {
            let mut query = [0.0_f32; 8];
            let mut vector = [0.0_f32; 8];
            for index in 0..8 {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let raw = ((state >> 33) as i64) as f32 / i32::MAX as f32;
                let scale = if case % 3 == 0 { 1.0e10_f32 } else { 1.0_f32 };
                query[index] = raw * scale;
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let raw_other = ((state >> 33) as i64) as f32 / i32::MAX as f32;
                vector[index] = raw_other * scale;
            }
            let exact = squared_l2_exact(&query, &vector);
            let computed = squared_l2_f64(&query, &vector);
            let relative = screening_relative(query.len());
            let (lower, upper) = (computed * (1.0 - relative), computed * (1.0 + relative));
            assert!(
                exact.cmp_f64(lower) != Ordering::Less && exact.cmp_f64(upper) != Ordering::Greater,
                "case {case}: f64 interval [{lower}, {upper}] does not contain the exact value"
            );
        }
    }
}
