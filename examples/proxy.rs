//! Use the proxy to dynamically switch the metrics input & names.

extern crate dipstick;

use std::thread::sleep;
use std::time::Duration;
use dipstick::{Proxy, Stream, InputScope, Input, Naming};


fn main() {
    let root_proxy = Proxy::default();
    let sub = root_proxy.add_naming("sub");

    let count1 = root_proxy.counter("counter_a");

    let count2 = sub.counter("counter_b");

    loop {
        let stdout = Stream::stdout().input();
        root_proxy.set_target(stdout.clone());
        count1.count(1);
        count2.count(2);

        // route every metric from the root to stdout with prefix "root"
        root_proxy.set_target(stdout.add_naming("root"));
        count1.count(3);
        count2.count(4);

        // route metrics from "sub" to stdout with prefix "mutant"
        sub.set_target(stdout.add_naming("mutant"));
        count1.count(5);
        count2.count(6);

        // clear root metrics route, "sub" still appears
        root_proxy.unset_target();
        count1.count(7);
        count2.count(8);

        // now no metrics appear
        sub.unset_target();
        count1.count(9);
        count2.count(10);

        // go back to initial single un-prefixed route
        root_proxy.set_target(stdout.clone());
        count1.count(11);
        count2.count(12);

        sleep(Duration::from_secs(1));

        println!()
    }

}