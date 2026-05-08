use std::collections::HashMap;
use std::time::Duration;

/// Timing statistics for a single operation type.
#[derive(Debug, Clone)]
struct OpTiming {
    count: u64,
    failures: u64,
    total: Duration,
    min: Duration,
    max: Duration,
    /// Sum of squared durations (in microseconds) for variance calculation.
    sum_sq_us: f64,
}

impl OpTiming {
    fn new() -> Self {
        Self {
            count: 0,
            failures: 0,
            total: Duration::ZERO,
            min: Duration::MAX,
            max: Duration::ZERO,
            sum_sq_us: 0.0,
        }
    }

    fn record(&mut self, elapsed: Duration, success: bool) {
        self.count += 1;
        if !success {
            self.failures += 1;
        }
        self.total += elapsed;
        self.min = self.min.min(elapsed);
        self.max = self.max.max(elapsed);
        let us = elapsed.as_secs_f64() * 1_000_000.0;
        self.sum_sq_us += us * us;
    }

    fn avg(&self) -> Duration {
        if self.count == 0 {
            return Duration::ZERO;
        }
        self.total / u32::try_from(self.count).unwrap_or(u32::MAX)
    }

    fn stddev_us(&self) -> f64 {
        if self.count < 2 {
            return 0.0;
        }
        let n = self.count as f64;
        let mean_us = self.total.as_secs_f64() * 1_000_000.0 / n;
        let variance = (self.sum_sq_us / n) - (mean_us * mean_us);
        // Guard against floating-point rounding producing a tiny negative
        variance.max(0.0).sqrt()
    }
}

/// Counters and timing collected during a fuzz run.
///
/// Tracks per-operation-type wall-clock execution time as a proxy for
/// computational cost, since cw-multi-test does not meter gas.
#[derive(Debug, Clone)]
pub struct RunStats {
    pub op_counts: HashMap<&'static str, u64>,
    pub success_count: u64,
    pub error_count: u64,
    pub total_ops: u64,
    timings: HashMap<&'static str, OpTiming>,
}

impl RunStats {
    /// Create empty stats.
    pub fn new() -> Self {
        Self {
            op_counts: HashMap::new(),
            success_count: 0,
            error_count: 0,
            total_ops: 0,
            timings: HashMap::new(),
        }
    }

    /// Record one operation result with its wall-clock duration.
    pub fn record(&mut self, op_name: &'static str, success: bool, elapsed: Duration) {
        *self.op_counts.entry(op_name).or_default() += 1;
        self.total_ops += 1;
        if success {
            self.success_count += 1;
        } else {
            self.error_count += 1;
        }
        self.timings
            .entry(op_name)
            .or_insert_with(OpTiming::new)
            .record(elapsed, success);
    }

    /// Format op counts with avg timing and failure rate as a compact string.
    /// e.g., "swap:2596(1.2ms,0%err) add:1466(3.4ms,2%err) remove:1366(0.8ms,5%err)"
    pub fn op_breakdown(&self) -> String {
        let mut pairs: Vec<_> = self.timings.iter().collect();
        pairs.sort_by_key(|(name, _)| *name);
        pairs
            .iter()
            .map(|(name, timing)| {
                let err_pct = if timing.count > 0 {
                    timing.failures * 100 / timing.count
                } else {
                    0
                };
                format!(
                    "{}:{}({},{}%err)",
                    name,
                    timing.count,
                    format_duration(timing.avg()),
                    err_pct,
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Print a summary with op counts, timing stats, and seed for reproducibility.
    pub fn print_summary(&self, label: &str, seed: u64) {
        println!(
            "{}: {} ops ({} success, {} errors), seed={}",
            label, self.total_ops, self.success_count, self.error_count, seed
        );

        if self.timings.is_empty() {
            return;
        }

        // Sort by op name for stable output
        let mut ops: Vec<_> = self.timings.iter().collect();
        ops.sort_by_key(|(name, _)| *name);

        println!(
            "  {:<18} {:>6} {:>5} {:>10} {:>10} {:>10} {:>10}",
            "operation", "count", "err%", "avg", "min", "max", "stddev"
        );
        println!("  {}", "-".repeat(75));

        for (name, timing) in &ops {
            let err_pct = if timing.count > 0 {
                timing.failures * 100 / timing.count
            } else {
                0
            };
            println!(
                "  {:<18} {:>6} {:>4}% {:>10} {:>10} {:>10} {:>10}",
                name,
                timing.count,
                err_pct,
                format_duration(timing.avg()),
                format_duration(timing.min),
                format_duration(timing.max),
                format!("{:.0}us", timing.stddev_us()),
            );
        }

        // Total row
        let total_time: Duration = self.timings.values().map(|t| t.total).sum();
        let total_err_pct = if self.total_ops > 0 {
            self.error_count * 100 / self.total_ops
        } else {
            0
        };
        println!("  {}", "-".repeat(75));
        println!(
            "  {:<18} {:>6} {:>4}% {:>10}",
            "total",
            self.total_ops,
            total_err_pct,
            format_duration(total_time),
        );
    }

    /// Merge another `RunStats` into this one.
    pub fn merge(&mut self, other: &RunStats) {
        self.success_count += other.success_count;
        self.error_count += other.error_count;
        self.total_ops += other.total_ops;
        for (name, count) in &other.op_counts {
            *self.op_counts.entry(name).or_default() += count;
        }
        for (name, timing) in &other.timings {
            let entry = self.timings.entry(name).or_insert_with(OpTiming::new);
            entry.count += timing.count;
            entry.failures += timing.failures;
            entry.total += timing.total;
            entry.min = entry.min.min(timing.min);
            entry.max = entry.max.max(timing.max);
            entry.sum_sq_us += timing.sum_sq_us;
        }
    }
}

/// Format a Duration as a human-readable string (e.g., "1h 23m 45s").
pub(crate) fn humanize_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{}h {:02}m {:02}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {:02}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Format a Duration for the timing table (e.g., "12.3ms", "1.23s", "456us").
fn format_duration(d: Duration) -> String {
    let us = d.as_micros();
    if us >= 1_000_000 {
        format!("{:.2}s", d.as_secs_f64())
    } else if us >= 1_000 {
        format!("{:.1}ms", us as f64 / 1_000.0)
    } else {
        format!("{}us", us)
    }
}
