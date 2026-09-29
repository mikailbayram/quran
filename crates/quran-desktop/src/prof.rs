//! Tiny opt-in profiler (`QURAN_PROFILE=1`): accumulates timings per label.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Instant;

thread_local! {
    static STATS: RefCell<BTreeMap<&'static str, (u32, u128, u128)>> = RefCell::default();
    static ON: bool = std::env::var_os("QURAN_PROFILE").is_some();
}

pub struct Span(&'static str, Instant);

pub fn span(name: &'static str) -> Option<Span> {
    ON.with(|on| *on).then(|| Span(name, Instant::now()))
}

impl Drop for Span {
    fn drop(&mut self) {
        let us = self.1.elapsed().as_micros();
        STATS.with(|s| {
            let mut s = s.borrow_mut();
            let e = s.entry(self.0).or_default();
            e.0 += 1;
            e.1 += us;
            e.2 = e.2.max(us);
        });
    }
}

pub fn report() -> String {
    STATS.with(|s| {
        let s = s.borrow();
        let mut out = String::new();
        for (k, (n, total, max)) in s.iter() {
            out.push_str(&format!(
                "{k:24} n={n:5} total={:8.1}ms avg={:7.1}us max={:7.1}ms\n",
                *total as f64 / 1000.0,
                *total as f64 / (*n).max(1) as f64,
                *max as f64 / 1000.0
            ));
        }
        out
    })
}

pub fn reset() {
    STATS.with(|s| s.borrow_mut().clear());
}
