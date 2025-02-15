//! Internal Dipstick runtime metrics.
//! Because the possibly high volume of data, this is pre-set to use aggregation.
//! This is also kept in a separate module because it is not to be exposed outside of the crate.

use crate::core::attributes::Prefixed;
use crate::core::input::{Counter, InputScope, Marker};
use crate::core::proxy::Proxy;

metrics! {
    /// Dipstick's own internal metrics.
    pub DIPSTICK_METRICS = "dipstick" => {

        "queue" => {
            pub SEND_FAILED: Marker = "send_failed";
        }

        "prometheus" => {
            pub PROMETHEUS_SEND_ERR: Marker = "send_failed";
            pub PROMETHEUS_OVERFLOW: Marker = "buf_overflow";
            pub PROMETHEUS_SENT_BYTES: Counter = "sent_bytes";
        }

        "graphite" => {
            pub GRAPHITE_SEND_ERR: Marker = "send_failed";
            pub GRAPHITE_OVERFLOW: Marker = "buf_overflow";
            pub GRAPHITE_SENT_BYTES: Counter = "sent_bytes";
        }

        "statsd" => {
            pub STATSD_SEND_ERR: Marker ="send_failed";
            pub STATSD_SENT_BYTES: Counter = "sent_bytes";
        }
    }
}
