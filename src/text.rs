//! Standard stateless metric outputs.

// TODO parameterize templates
use core::{Name, AddPrefix, Value, Kind, OutputScope, WithAttributes, Attributes,
           Buffered, OutputMetric, Output, Flush};
use error;
use std::sync::{RwLock, Arc};
use std::io::{Write, self};
use std::rc::Rc;
use std::cell::RefCell;

pub fn format_name(name: &Name, _kind: Kind) -> Vec<String> {
    let mut z = name.join(".");
    z.push_str(" ");
    vec![z]
}

pub fn print_name_value_line(output: &mut impl Write, template: &[String], value: Value) -> error::Result<()> {
    write!(output, "{}", template[0])?;
    write!(output, "{}", value)?;
    writeln!(output)?;
    Ok(())
}

/// Buffered metrics text output.
pub struct Text<W: Write + Send + Sync + 'static> {
    attributes: Attributes,
    inner: Arc<RwLock<W>>,
    format_fn: Arc<Fn(&Name, Kind) -> Vec<String> + Send + Sync>,
    print_fn: Arc<Fn(&mut Vec<u8>, &[String], Value) -> error::Result<()> + Send + Sync>,
}

use queue_out;
use cache_out;

impl<W: Write + Send + Sync + 'static> queue_out::QueuedOutput for Text<W> {}
impl<W: Write + Send + Sync + 'static> cache_out::CachedOutput for Text<W> {}

impl<W: Write + Send + Sync + 'static>  Text<W> {
    /// Write metric values to provided Write target.
    pub fn write_to(write: W) -> Text<W> {
        Text {
            attributes: Attributes::default(),
            inner: Arc::new(RwLock::new(write)),
            format_fn: Arc::new(format_name),
            print_fn: Arc::new(print_name_value_line),
        }
    }

    /// Write metric values to stdout.
    pub fn stdout() -> Text<io::Stdout> {
        Text::write_to(io::stdout())
    }

    /// Write metric values to stdout.
    pub fn stderr() -> Text<io::Stderr> {
        Text::write_to(io::stderr())
    }

}

// FIXME manual Clone impl required because auto-derive is borked (https://github.com/rust-lang/rust/issues/26925)
impl<W: Write + Send + Sync + 'static> Clone for Text<W> {
    fn clone(&self) -> Self {
        Text {
            attributes: self.attributes.clone(),
            inner: self.inner.clone(),
            format_fn: self.format_fn.clone(),
            print_fn: self.print_fn.clone(),
        }
    }
}

impl<W: Write + Send + Sync + 'static> WithAttributes for Text<W> {
    fn get_attributes(&self) -> &Attributes { &self.attributes }
    fn mut_attributes(&mut self) -> &mut Attributes { &mut self.attributes }
}

impl<W: Write + Send + Sync + 'static> Buffered for Text<W> {}

impl<W: Write + Send + Sync + 'static> Output for Text<W> {
    type SCOPE = TextScope<W>;

    fn output(&self) -> Self::SCOPE {
        TextScope {
            attributes: self.attributes.clone(),
            entries: Rc::new(RefCell::new(Vec::new())),
            output: self.clone(),
        }
    }
}

/// A scope for text metrics.
pub struct TextScope<W: Write + Send + Sync + 'static> {
    attributes: Attributes,
    entries: Rc<RefCell<Vec<Vec<u8>>>>,
    output: Text<W>,
}


impl<W: Write + Send + Sync + 'static> Clone for TextScope<W> {
    fn clone(&self) -> Self {
        TextScope {
            attributes: self.attributes.clone(),
            entries: self.entries.clone(),
            output: self.output.clone(),
        }
    }
}

impl<W: Write + Send + Sync + 'static> WithAttributes for TextScope<W> {
    fn get_attributes(&self) -> &Attributes { &self.attributes }
    fn mut_attributes(&mut self) -> &mut Attributes { &mut self.attributes }
}

impl<W: Write + Send + Sync + 'static> Buffered for TextScope<W> {}

impl<W: Write + Send + Sync + 'static> OutputScope for TextScope<W> {
    fn new_metric(&self, name: Name, kind: Kind) -> OutputMetric {
        let name = self.qualified_name(name);
        let template = (self.output.format_fn)(&name, kind);

        let print_fn = self.output.print_fn.clone();
        let entries = self.entries.clone();

        if self.is_buffered() {
            OutputMetric::new(move |value| {
                let mut buffer = Vec::with_capacity(32);
                match (print_fn)(&mut buffer, &template, value) {
                    Ok(()) => {
                        let mut entries = entries.borrow_mut();
                        entries.push(buffer.into())
                    },
                    Err(err) => debug!("{}", err),
                }
            })
        } else {
            let output = self.output.clone();
            OutputMetric::new(move |value| {
                let mut buffer = Vec::with_capacity(32);
                match (print_fn)(&mut buffer, &template, value) {
                    Ok(()) => {
                        let mut output = output.inner.write().expect("TextOutput");
                        if let Err(e) = output.write_all(&buffer).and_then(|_| output.flush()) {
                            debug!("Could not write text metrics: {}", e)
                        }
                    },
                    Err(err) => debug!("{}", err),
                }
            })
        }
    }
}

impl<W: Write + Send + Sync + 'static> Flush for TextScope<W> {

    fn flush(&self) -> error::Result<()> {
        let mut entries = self.entries.borrow_mut();
        if !entries.is_empty() {
            let mut output = self.output.inner.write().expect("TextOutput");
            for entry in entries.drain(..) {
                output.write_all(&entry)?
            }
            output.flush()?;
        }
        Ok(())
    }
}

impl<W: Write + Send + Sync + 'static> Drop for TextScope<W> {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            warn!("Could not flush text metrics on Drop. {}", e)
        }
    }
}

#[cfg(test)]
mod test {
    use core::*;
    use std::io;

    #[test]
    fn sink_print() {
        let c = super::Text::write_to(io::stdout()).output();
        let m = c.new_metric("test".into(), Kind::Marker);
        m.write(33);
    }

}
