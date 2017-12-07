//! A sample application sending ad-hoc counter values both to statsd _and_ to stdout.

extern crate dipstick;

use dipstick::*;
use std::time::Duration;

fn main() {
    let metrics = metrics(
        // Metric caching allows re-use of the counter, skipping cost of redefining it on each use.
        cache(12, (
            sample(0.01, to_statsd("localhost:8125", "myapp.").expect("Could not connect to statsd")),
            to_stdout(),
        )),
    );

    loop {
        metrics.counter("counter_a").count(123);
        metrics.timer("timer_a").interval_us(2000000);
        std::thread::sleep(Duration::from_millis(40));
    }
}
