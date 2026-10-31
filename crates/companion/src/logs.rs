//! Logging setup, plus the last log lines kept in memory for the diagnostics report. Only info
//! and above are kept, at most 200 lines and 64 KB, and each line is redacted first.

use std::collections::VecDeque;
use std::fmt::Write as _;

use parking_lot::Mutex;
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const MAX_LINES: usize = 200;
const MAX_BYTES: usize = 64 * 1024;
const MAX_LINE: usize = 1000;

struct Ring {
    lines: VecDeque<String>,
    bytes: usize,
}

static RING: Mutex<Ring> = parking_lot::const_mutex(Ring { lines: VecDeque::new(), bytes: 0 });

/// Logs to the terminal (level from KEYSIDE_LOG, info by default) and keeps recent lines in memory.
pub fn init(with_target: bool) {
    let filter = EnvFilter::try_from_env("KEYSIDE_LOG").unwrap_or_else(|_| "info".into());
    let _ =
        tracing_subscriber::registry().with(tracing_subscriber::fmt::layer().with_target(with_target).with_filter(filter)).with(Recent.with_filter(LevelFilter::INFO)).try_init();
}

/// Adds a line, dropping the oldest ones past the limits.
pub fn push(line: &str) {
    let mut line = crate::diagnostics::redact(line);
    if line.len() > MAX_LINE {
        let mut cut = MAX_LINE;
        while !line.is_char_boundary(cut) {
            cut -= 1;
        }
        line.truncate(cut);
        line.push('…');
    }
    let mut r = RING.lock();
    r.bytes += line.len();
    r.lines.push_back(line);
    while r.lines.len() > MAX_LINES || r.bytes > MAX_BYTES {
        match r.lines.pop_front() {
            Some(old) => r.bytes -= old.len(),
            None => break,
        }
    }
}

/// The kept lines, oldest first, at most `n`.
pub fn recent(n: usize) -> Vec<String> {
    let r = RING.lock();
    r.lines.iter().skip(r.lines.len().saturating_sub(n)).cloned().collect()
}

struct Recent;

#[derive(Default)]
struct Line {
    message: String,
    fields: String,
}

impl Visit for Line {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            let _ = write!(self.fields, " {}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.fields, " {}={value:?}", field.name());
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for Recent {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut l = Line::default();
        event.record(&mut l);
        let t = time::OffsetDateTime::now_utc();
        let line = format!("{:02}-{:02} {:02}:{:02}:{:02} {:<5} {}{}", t.month() as u8, t.day(), t.hour(), t.minute(), t.second(), event.metadata().level(), l.message, l.fields);
        push(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_within_the_limits() {
        for i in 0..(MAX_LINES + 50) {
            push(&format!("ring test line {i}"));
        }
        let r = RING.lock();
        assert!(r.lines.len() <= MAX_LINES && r.bytes <= MAX_BYTES);
        assert_eq!(r.bytes, r.lines.iter().map(String::len).sum::<usize>());
        drop(r);
        push(&"x".repeat(5000));
        assert!(recent(MAX_LINES).iter().all(|l| l.len() <= MAX_LINE + 3));
    }
}
