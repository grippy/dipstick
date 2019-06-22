//! Send metrics to a graphite server.

use crate::cache::cache_out;
use crate::core::attributes::{Attributes, Buffered, OnFlush, Prefixed, WithAttributes};
use crate::core::error;
use crate::core::input::InputKind;
use crate::core::label::Labels;
use crate::core::metrics;
use crate::core::name::MetricName;
use crate::core::output::{Output, OutputMetric, OutputScope};
use crate::core::{Flush, MetricValue};
use crate::output::socket::RetrySocket;
use crate::queue::queue_out;

use std::net::ToSocketAddrs;

use std::fmt::Debug;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use std::sync::Arc;

#[cfg(not(feature = "parking_lot"))]
use std::sync::RwLock;

#[cfg(feature = "parking_lot")]
use parking_lot::RwLock;

/// Graphite output holds a socket to a graphite server.
/// The socket is shared between scopes opened from the output.
#[derive(Clone, Debug)]
pub struct Graphite {
    attributes: Attributes,
    socket: Arc<RwLock<RetrySocket>>,
}

impl Output for Graphite {
    type SCOPE = GraphiteScope;

    fn new_scope(&self) -> Self::SCOPE {
        GraphiteScope {
            attributes: self.attributes.clone(),
            buffer: Rc::new(RefCell::new(String::new())),
            socket: self.socket.clone(),
        }
    }
}

impl Graphite {
    /// Send metrics to a graphite server at the address and port provided.
    pub fn send_to<A: ToSocketAddrs + Debug + Clone>(address: A) -> error::Result<Graphite> {
        debug!("Connecting to graphite {:?}", address);
        let socket = Arc::new(RwLock::new(RetrySocket::new(address.clone())?));

        Ok(Graphite {
            attributes: Attributes::default(),
            socket,
        })
    }
}

impl WithAttributes for Graphite {
    fn get_attributes(&self) -> &Attributes {
        &self.attributes
    }
    fn mut_attributes(&mut self) -> &mut Attributes {
        &mut self.attributes
    }
}

impl Buffered for Graphite {}

/// Graphite Input
#[derive(Debug, Clone)]
pub struct GraphiteScope {
    attributes: Attributes,
    buffer: Rc<RefCell<String>>,
    socket: Arc<RwLock<RetrySocket>>,
}

impl OutputScope for GraphiteScope {
    /// Define a metric of the specified type.
    fn new_metric(&self, name: MetricName, kind: InputKind) -> OutputMetric {
        let prefix = self.prefix_prepend(name).join(".");
        let scale = match kind {
            // timers are in µs, but we give graphite milliseconds
            InputKind::Timer => 1000,
            _ => 1,
        };

        let cloned = self.clone();
        let metric = GraphiteMetric { prefix, scale };

        OutputMetric::new(move |value, labels| {
            cloned.print(&metric, value, labels);
        })
    }
}

impl Flush for GraphiteScope {
    fn flush(&self) -> error::Result<()> {
        self.notify_flush_listeners();
        let buf = self.buffer.borrow_mut();
        self.flush_inner(buf)
    }
}

impl GraphiteScope {
    fn print(&self, metric: &GraphiteMetric, value: MetricValue, labels: Labels) {
        let scaled_value = value / metric.scale;
        let value_str = scaled_value.to_string();

        let start = SystemTime::now();

        let mut buffer = self.buffer.borrow_mut();
        match start.duration_since(UNIX_EPOCH) {
            Ok(timestamp) => {
                // add prefix
                buffer.push_str(&metric.prefix);
                // add labels?
                let labels_map = labels.into_map();
                if !labels_map.is_empty() {
                    buffer.push_str(";");
                    let mut i = labels_map.into_iter();
                    let mut next = i.next();
                    while let Some((k, v)) = next {
                        buffer.push_str(&k);
                        buffer.push_str("=");
                        buffer.push_str(&v);
                        next = i.next();
                        if next.is_some() {
                            buffer.push_str(";");
                        }
                    }
                }
                buffer.push(' ');
                // add value + timestamp
                buffer.push_str(&value_str);
                buffer.push(' ');
                buffer.push_str(&timestamp.as_secs().to_string());
                buffer.push('\n');

                if buffer.len() > BUFFER_FLUSH_THRESHOLD {
                    metrics::GRAPHITE_OVERFLOW.mark();
                    warn!("Graphite Buffer Size Exceeded: {}", BUFFER_FLUSH_THRESHOLD);
                    let _ = self.flush_inner(buffer);
                    buffer = self.buffer.borrow_mut();
                }
            }
            Err(e) => {
                warn!("Could not compute epoch timestamp. {}", e);
            }
        };

        if self.is_buffered() {
            if let Err(e) = self.flush_inner(buffer) {
                debug!("Could not send to graphite {}", e)
            }
        }
    }

    fn flush_inner(&self, mut buf: RefMut<String>) -> error::Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        trace!("Sending to graphite: {:?}", &buf.as_str());
        let mut sock = write_lock!(self.socket);
        match sock.write_all(buf.as_bytes()) {
            Ok(()) => {
                metrics::GRAPHITE_SENT_BYTES.count(buf.len());
                trace!("Sent {} bytes to graphite", buf.len());
                buf.clear();
                Ok(())
            }
            Err(e) => {
                metrics::GRAPHITE_SEND_ERR.mark();
                debug!("Failed to send buffer to graphite: {}", e);
                Err(e.into())
            }
        }
    }
}

impl WithAttributes for GraphiteScope {
    fn get_attributes(&self) -> &Attributes {
        &self.attributes
    }
    fn mut_attributes(&mut self) -> &mut Attributes {
        &mut self.attributes
    }
}

impl Buffered for GraphiteScope {}

impl queue_out::QueuedOutput for Graphite {}
impl cache_out::CachedOutput for Graphite {}

/// Its hard to see how a single scope could get more metrics than this.
// TODO make configurable?
const BUFFER_FLUSH_THRESHOLD: usize = 65_536;

/// Key of a graphite metric.
#[derive(Debug, Clone)]
pub struct GraphiteMetric {
    prefix: String,
    scale: isize,
}

/// Any remaining buffered data is flushed on Drop.
impl Drop for GraphiteScope {
    fn drop(&mut self) {
        if let Err(err) = self.flush() {
            warn!("Could not flush graphite metrics upon Drop: {}", err)
        }
    }
}

#[cfg(feature = "bench")]
mod bench {

    use super::*;
    use crate::core::attributes::*;
    use crate::core::input::*;

    #[bench]
    pub fn immediate_graphite(b: &mut test::Bencher) {
        let sd = Graphite::send_to("localhost:2003").unwrap().metrics();
        let timer = sd.new_metric("timer".into(), InputKind::Timer);

        b.iter(|| test::black_box(timer.write(2000, labels![])));
    }

    #[bench]
    pub fn buffering_graphite(b: &mut test::Bencher) {
        let sd = Graphite::send_to("localhost:2003")
            .unwrap()
            .buffered(Buffering::BufferSize(65465))
            .metrics();
        let timer = sd.new_metric("timer".into(), InputKind::Timer);

        b.iter(|| test::black_box(timer.write(2000, labels![])));
    }

}
