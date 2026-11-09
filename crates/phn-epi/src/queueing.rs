//! Multi-server queues (M/M/c, Erlang C) for dispensing-site and clinic
//! planning (EPR-02).
//!
//! Each station is modeled as an M/M/c queue: Poisson arrivals at rate λ,
//! exponential service at rate μ per server, c identical servers, first come
//! first served, unlimited waiting room, in steady state. With offered load
//! a = λ/μ and utilization ρ = a/c < 1:
//!
//! * Erlang C, the probability an arrival waits:
//!   C(c, a) = [aᶜ/c! · c/(c − a)] / [Σₖ₌₀^{c−1} aᵏ/k! + aᶜ/c! · c/(c − a)].
//!   It is computed through the Erlang B recursion B(0) = 1,
//!   B(k) = a·B(k−1) / (k + a·B(k−1)), and C = c·B / (c − a(1 − B)), which is
//!   numerically stable for hundreds of servers.
//! * Mean queue length Lq = C·ρ/(1 − ρ); mean wait Wq = C/(cμ − λ);
//!   time in system W = Wq + 1/μ; number in system L = λW (Little's law).
//! * P(wait > t) = C·exp(−(cμ − λ)t).
//!
//! Reference: D. Gross, J. F. Shortle, J. M. Thompson, C. M. Harris,
//! *Fundamentals of Queueing Theory*, 4th ed. (Wiley, 2008), section 2.3
//! (M/M/c). Golden tests: `tests/queueing_golden.rs`.

use serde::{Deserialize, Serialize};

/// Steady-state measures of an M/M/c queue. Times are in the units of
/// 1/λ and 1/μ (hours when the rates are per hour).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MmcMeasures {
    pub servers: u32,
    /// Arrival rate λ.
    pub arrival_rate: f64,
    /// Service rate μ per server.
    pub service_rate: f64,
    /// Offered load a = λ/μ (busy servers on average).
    pub offered_load: f64,
    /// Utilization ρ = λ/(cμ).
    pub utilization: f64,
    /// Erlang C: probability an arrival has to wait.
    pub prob_wait: f64,
    /// Mean number waiting in line, Lq.
    pub mean_queue: f64,
    /// Mean wait in line, Wq.
    pub mean_wait: f64,
    /// Mean time in system (wait + service), W.
    pub mean_time_in_system: f64,
    /// Mean number in system, L = λW.
    pub mean_in_system: f64,
}

/// Erlang B (blocking probability) for `c` servers and offered load `a`.
pub fn erlang_b(c: u32, a: f64) -> f64 {
    let mut b = 1.0;
    for k in 1..=c {
        b = a * b / (k as f64 + a * b);
    }
    b
}

/// Erlang C (probability of waiting). Returns 1 when the queue is unstable (a ≥ c).
pub fn erlang_c(c: u32, a: f64) -> f64 {
    if c == 0 || a >= c as f64 {
        return 1.0;
    }
    if a <= 0.0 {
        return 0.0;
    }
    let b = erlang_b(c, a);
    let c = c as f64;
    c * b / (c - a * (1.0 - b))
}

/// Steady-state M/M/c measures, or None when λ ≥ cμ (the line grows without limit)
/// or a rate is not positive.
pub fn mmc(servers: u32, arrival_rate: f64, service_rate: f64) -> Option<MmcMeasures> {
    if servers == 0
        || !(arrival_rate.is_finite() && service_rate.is_finite())
        || arrival_rate < 0.0
        || service_rate <= 0.0
    {
        return None;
    }
    let c = servers as f64;
    let a = arrival_rate / service_rate;
    if a >= c {
        return None;
    }
    let rho = a / c;
    let pw = erlang_c(servers, a);
    let (lq, wq) = if arrival_rate == 0.0 {
        (0.0, 0.0)
    } else {
        (
            pw * rho / (1.0 - rho),
            pw / (c * service_rate - arrival_rate),
        )
    };
    let w = wq + 1.0 / service_rate;
    Some(MmcMeasures {
        servers,
        arrival_rate,
        service_rate,
        offered_load: a,
        utilization: rho,
        prob_wait: pw,
        mean_queue: lq,
        mean_wait: wq,
        mean_time_in_system: w,
        mean_in_system: arrival_rate * w,
    })
}

/// Probability the wait in line exceeds `t` (same time unit as the rates).
pub fn prob_wait_exceeds(m: &MmcMeasures, t: f64) -> f64 {
    let gap = m.servers as f64 * m.service_rate - m.arrival_rate;
    (m.prob_wait * (-gap * t.max(0.0)).exp()).clamp(0.0, 1.0)
}

/// Fewest servers so that utilization ≤ `max_utilization` and the mean wait
/// ≤ `max_mean_wait` (when given). Returns None when more than `limit`
/// servers would be needed.
pub fn min_servers(
    arrival_rate: f64,
    service_rate: f64,
    max_utilization: f64,
    max_mean_wait: Option<f64>,
    limit: u32,
) -> Option<u32> {
    if arrival_rate <= 0.0 {
        return Some(1);
    }
    let start = ((arrival_rate / service_rate / max_utilization).ceil() as u32).max(1);
    (start..=limit).find(|&c| {
        mmc(c, arrival_rate, service_rate).is_some_and(|m| {
            m.utilization <= max_utilization + 1e-12
                && max_mean_wait.is_none_or(|w| m.mean_wait <= w + 1e-12)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_server_reduces_to_mm1() {
        // M/M/1: P(wait) = ρ, Lq = ρ²/(1 − ρ), Wq = ρ/(μ − λ).
        let m = mmc(1, 3.0, 4.0).unwrap();
        assert!((m.prob_wait - 0.75).abs() < 1e-12);
        assert!((m.mean_queue - 2.25).abs() < 1e-12);
        assert!((m.mean_wait - 0.75).abs() < 1e-12);
        assert!((m.mean_in_system - 3.0).abs() < 1e-12);
    }

    #[test]
    fn two_servers_match_the_closed_form() {
        // C(2, a) = a² / (2 + a).
        for a in [0.2, 0.5, 1.0, 1.5, 1.9] {
            assert!((erlang_c(2, a) - a * a / (2.0 + a)).abs() < 1e-12);
        }
    }

    #[test]
    fn unstable_and_sizing() {
        assert!(mmc(2, 10.0, 5.0).is_none());
        assert_eq!(erlang_c(2, 3.0), 1.0);
        // 60 arrivals/hour, 12 per server per hour: 5 busy servers on average;
        // at most 85% busy needs 6.
        assert_eq!(min_servers(60.0, 12.0, 0.85, None, 100), Some(6));
        let m = mmc(6, 60.0, 12.0).unwrap();
        assert!(prob_wait_exceeds(&m, 0.0) - m.prob_wait < 1e-12);
        assert!(prob_wait_exceeds(&m, 1.0) < m.prob_wait);
        assert_eq!(min_servers(1.0e6, 1.0, 0.9, None, 10), None);
    }
}
