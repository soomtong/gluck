//! 성능 측정 — warmup + iters 루프, percentile/평균 계산.

use std::time::Instant;

use crate::search::SearchEngine;
use crate::search::SearchResult;
use crate::search::report::fixtures::FixtureQuery;
use crate::search::report::ReportError;

#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub mean_ms: f64,
    pub qps: f64,
    pub n_samples: usize,
}

/// 정렬된 밀리초 샘플에서 percentile (0.0~1.0)을 nearest-rank로 산출.
fn percentile(sorted_ms: &[f64], p: f64) -> f64 {
    if sorted_ms.is_empty() {
        return 0.0;
    }
    let rank = (p * sorted_ms.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted_ms.len() - 1);
    sorted_ms[idx]
}

pub fn latency_stats(samples_ms: &[f64], total_elapsed_s: f64) -> LatencyStats {
    let mut sorted: Vec<f64> = samples_ms.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = if sorted.is_empty() {
        0.0
    } else {
        sorted.iter().sum::<f64>() / sorted.len() as f64
    };
    let qps = if total_elapsed_s > 0.0 {
        sorted.len() as f64 / total_elapsed_s
    } else {
        0.0
    };
    LatencyStats {
        p50_ms: percentile(&sorted, 0.50),
        p95_ms: percentile(&sorted, 0.95),
        p99_ms: percentile(&sorted, 0.99),
        mean_ms: mean,
        qps,
        n_samples: sorted.len(),
    }
}

/// warmup회 검색 후 iters회 반복하며 latency 수집. 마지막 iter의 결과(쿼리당 1개) 반환.
pub fn run_perf(
    engine: &SearchEngine,
    queries: &[FixtureQuery],
    warmup: usize,
    iters: usize,
    limit: usize,
) -> Result<(LatencyStats, Vec<Vec<SearchResult>>), ReportError> {
    for _ in 0..warmup {
        for q in queries {
            let _ = engine.search(&q.text, limit)?;
        }
    }

    let mut latencies_ms: Vec<f64> = Vec::with_capacity(iters * queries.len());
    let mut last_results: Vec<Vec<SearchResult>> = Vec::with_capacity(queries.len());

    let total_start = Instant::now();
    for iter_i in 0..iters {
        if iter_i == iters - 1 {
            last_results.clear();
        }
        for q in queries {
            let t0 = Instant::now();
            let r = engine.search(&q.text, limit)?;
            let dt_ms = t0.elapsed().as_secs_f64() * 1000.0;
            latencies_ms.push(dt_ms);
            if iter_i == iters - 1 {
                last_results.push(r);
            }
        }
    }
    let total_s = total_start.elapsed().as_secs_f64();

    Ok((latency_stats(&latencies_ms, total_s), last_results))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_basic() {
        let s = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&s, 0.5), 5.0);
        assert_eq!(percentile(&s, 0.95), 10.0);
        assert_eq!(percentile(&s, 0.99), 10.0);
    }

    #[test]
    fn percentile_empty() {
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn latency_stats_basic() {
        let samples = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let stats = latency_stats(&samples, 0.5);
        assert!((stats.mean_ms - 30.0).abs() < 1e-6);
        assert_eq!(stats.p50_ms, 30.0);
        assert_eq!(stats.p95_ms, 50.0);
        assert!((stats.qps - 10.0).abs() < 1e-6); // 5 samples / 0.5s
        assert_eq!(stats.n_samples, 5);
    }

    #[test]
    fn latency_stats_zero_elapsed_gives_zero_qps() {
        let stats = latency_stats(&[1.0, 2.0], 0.0);
        assert_eq!(stats.qps, 0.0);
    }
}
