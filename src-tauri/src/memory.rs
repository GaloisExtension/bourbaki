//! Memory Agent: FTS5 + 埋め込みの RRF 統合と時間減衰。

use std::collections::HashMap;

/// 正規化済みベクトル同士のドット積（コサイン類似度）
pub fn cosine_dot_norm_q_d(query: &[f32], doc: &[f32]) -> f32 {
    if query.len() != doc.len() || query.is_empty() {
        return 0.0;
    }
    let mut s = 0.0f32;
    for i in 0..query.len() {
        s += query[i] * doc[i];
    }
    s
}

pub fn f32_blob_to_vec(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.len() < 4 || blob.len() % 4 != 0 {
        return None;
    }
    let n = blob.len() / 4;
    let mut v = Vec::with_capacity(n);
    for chunk in blob.chunks_exact(4) {
        v.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Some(v)
}

/// 類似度降順で先頭が最良 → rank 1, 2, 3…
pub fn ranks_from_vector_scores(id_scores: &[(i64, f32)]) -> HashMap<i64, usize> {
    let mut sorted: Vec<(i64, f32)> = id_scores.to_vec();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut map = HashMap::new();
    for (i, (id, _)) in sorted.iter().enumerate() {
        map.entry(*id).or_insert(i + 1);
    }
    map
}

/// FTS は bm25 昇順で渡す（先頭が最良）
pub fn ranks_from_ordered_ids(ids: &[i64]) -> HashMap<i64, usize> {
    let mut map = HashMap::new();
    for (i, id) in ids.iter().enumerate() {
        map.entry(*id).or_insert(i + 1);
    }
    map
}

/// RRF: score(d) = Σ 1/(k + rank_i)
pub fn reciprocal_rank_fusion(
    lists: &[HashMap<i64, usize>],
    k: f64,
) -> HashMap<i64, f64> {
    let mut acc: HashMap<i64, f64> = HashMap::new();
    for m in lists {
        for (id, rank) in m {
            *acc.entry(*id).or_insert(0.0) += 1.0 / (k + *rank as f64);
        }
    }
    acc
}

/// 経過日数に対する指数減衰（λ は 1 日あたり）
pub fn time_decay_factor(created_at_unix: i64, now_unix: i64, lambda_per_day: f64) -> f64 {
    if lambda_per_day <= 0.0 {
        return 1.0;
    }
    let secs = (now_unix - created_at_unix).max(0) as f64;
    let days = secs / 86400.0;
    (-lambda_per_day * days).exp()
}

pub fn apply_decay_to_scores(
    scores: HashMap<i64, f64>,
    created_at: &HashMap<i64, i64>,
    decay_weight: &HashMap<i64, f64>,
    now_unix: i64,
    lambda_per_day: f64,
) -> Vec<(i64, f64)> {
    let mut out: Vec<(i64, f64)> = scores
        .into_iter()
        .map(|(id, s)| {
            let age = created_at.get(&id).copied().unwrap_or(now_unix);
            let w = decay_weight.get(&id).copied().unwrap_or(1.0);
            let d = time_decay_factor(age, now_unix, lambda_per_day);
            (id, s * d * w)
        })
        .collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_prefers_top_of_both_lists() {
        let mut a = HashMap::new();
        a.insert(1i64, 2);
        a.insert(2, 1);
        let mut b = HashMap::new();
        b.insert(1, 2);
        b.insert(2, 1);
        let fused = reciprocal_rank_fusion(&[a, b], 60.0);
        assert!(fused[&2] > fused[&1]);
    }

    #[test]
    fn decay_old_is_lower() {
        let now = 1_000_000i64;
        let recent = now - 3600;
        let old = now - 10 * 86400;
        assert!(time_decay_factor(recent, now, 0.05) > time_decay_factor(old, now, 0.05));
    }
}
