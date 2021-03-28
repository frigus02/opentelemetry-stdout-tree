//! An stdout exporter implementation for [OpenTelemetry Rust], which prints traces in a tree-like
//! format.
//!
//! [opentelemetry rust]: https://github.com/open-telemetry/opentelemetry-rust
//!
//! ```text
//! SE  my-awesome-books.com  GET /authors/:authorId/boo  500  584ms  ==================
//!  IN  middleware - expressInit                           0      0  =
//!  IN  middleware - query                                 0      0  =
//!  IN  middleware - session                               0  523ms  ================
//!   CL  pg-pool.connect                                   0  303ms  =========
//!   CL  sessions  SELECT sess FROM "session" WHERE sid    0  219ms           =======
//!  IN  middleware - initialize                            0      0                  =
//!  IN  middleware - authenticate                          0      0                  =
//!   user authenticated
//!  IN  request handler - /authors/:authorId/books/:boo    0   59ms                  ==
//!   CL  book-service.book-service  POST /graphql        200   59ms                  ==
//!    SE  book-service.book.service  POST /graphql       200      0                   =
//!     IN  query                                           0      0                   =
//!      IN  field                                          2      0                   =
//!       unknown: something went wrong
//!     IN  parse                                           0      0                   =
//!     IN  validation                                      0      0                   =
//! ```
//!
//! # Usage
//!
//! Configure an OpenTelemetry pipeline and start creating spans:
//!
//! ```
//! use opentelemetry::trace::Tracer as _;
//!
//! let tracer = opentelemetry_stdout_tree::new_pipeline().install_simple();
//! tracer.in_span("main", |_cx| {});
//! ```
//!
//! ## Features
//!
//! The function `install` automatically configures an asynchronous batch exporter if you enable
//! either the **async-std** or **tokio** feature for the `opentelemetry` crate. Otherwise spans
//! will be exported synchronously.
//!
//! # Attribute mapping
//!
//! The exporter makes use of [OpenTelemetry semantic conventions] to provide more useful output
//! for known types of spans. Currently supported are:
//!
//! - HTTP: Shows method, host and path and uses status code to determine errors.
//! - DB: Shows database name and statement or operation.
//! - Exception events: shows exception type and message.
//!
//! [opentelemetry semantic conventions]: https://github.com/open-telemetry/opentelemetry-specification/tree/master/specification/trace/semantic_conventions
#![doc(html_root_url = "https://docs.rs/opentelemetry-stdout-tree/0.1.0")]
#![deny(missing_docs, unreachable_pub, missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]

use async_trait::async_trait;
use opentelemetry::{
    global,
    sdk::{
        self,
        export::{
            trace::{ExportResult, SpanData, SpanExporter},
            ExportError,
        },
    },
    trace::{SpanContext, SpanId, SpanKind, StatusCode, TraceId, TracerProvider},
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::SystemTime,
};

mod print;

/// Create a new stdout tree exporter pipeline builder
pub fn new_pipeline() -> StdoutTreePipelineBuilder {
    StdoutTreePipelineBuilder::default()
}

/// Pipeline builder for stdout tree exporter
#[derive(Debug)]
pub struct StdoutTreePipelineBuilder {
    trace_config: Option<sdk::trace::Config>,
}

impl Default for StdoutTreePipelineBuilder {
    fn default() -> Self {
        Self { trace_config: None }
    }
}

impl StdoutTreePipelineBuilder {
    /// Install an OpenTelemetry pipeline with the stdout tree span exporter
    pub fn install_simple(mut self) -> sdk::trace::Tracer {
        let exporter = StdoutTreeExporter::new();
        let mut provider_builder =
            sdk::trace::TracerProvider::builder().with_simple_exporter(exporter);
        if let Some(config) = self.trace_config.take() {
            provider_builder = provider_builder.with_config(config);
        }
        let provider = provider_builder.build();
        let tracer =
            provider.get_tracer("opentelemetry-stdout-tree", Some(env!("CARGO_PKG_VERSION")));
        let _ = global::set_tracer_provider(provider);
        tracer
    }

    /// Assign the SDK trace configuration
    pub fn with_trace_config(mut self, config: sdk::trace::Config) -> Self {
        self.trace_config = Some(config);
        self
    }
}

/// Stdout tree span exporter
#[derive(Debug)]
pub struct StdoutTreeExporter {
    buffer: HashMap<TraceId, HashMap<SpanId, Vec<SpanData>>>,
}

impl StdoutTreeExporter {
    fn new() -> Self {
        Self {
            buffer: HashMap::new(),
        }
    }
}

#[async_trait]
impl SpanExporter for StdoutTreeExporter {
    async fn export(&mut self, batch: Vec<SpanData>) -> ExportResult {
        for span_data in batch {
            if span_data.parent_span_id.to_u64() == 0 || span_data.span_context.is_remote() {
                // TODO: This assumes that a trace only has 1 root span, which can be identified by
                // a zero-ed parent span id or by having a remote parent. Is this true?
                let mut trace = self
                    .buffer
                    .remove(&span_data.span_context.trace_id())
                    .unwrap_or_else(HashMap::new);
                trace.insert(SpanId::invalid(), vec![span_data]);
                print::print_trace(trace).map_err(Error::IOError)?;
            } else {
                self.buffer
                    .entry(span_data.span_context.trace_id())
                    .or_default()
                    .entry(span_data.parent_span_id)
                    .or_default()
                    .push(span_data);
            }
        }

        Ok(())
    }

    fn shutdown(&mut self) {
        let trace_ids: Vec<_> = self.buffer.keys().cloned().collect();
        for trace_id in trace_ids {
            let mut trace = self.buffer.remove(&trace_id).expect("key must exist");
            let span_ids: HashSet<_> = trace
                .values()
                .flatten()
                .map(|span_data| span_data.span_context.span_id())
                .collect();
            let parent_span_ids: Vec<_> = trace
                .keys()
                .cloned()
                .filter(|x| !span_ids.contains(x))
                .collect();
            trace.insert(
                SpanId::invalid(),
                parent_span_ids
                    .into_iter()
                    .map(|parent_span_id| SpanData {
                        span_context: SpanContext::new(
                            trace_id,
                            parent_span_id,
                            0,
                            false,
                            Default::default(),
                        ),
                        parent_span_id: SpanId::invalid(),
                        span_kind: SpanKind::Internal,
                        name: "ORPHANED".into(),
                        start_time: SystemTime::now(),
                        end_time: SystemTime::now(),
                        attributes: sdk::trace::EvictedHashMap::new(0, 0),
                        message_events: sdk::trace::EvictedQueue::new(0),
                        links: sdk::trace::EvictedQueue::new(0),
                        status_code: StatusCode::Unset,
                        status_message: String::new(),
                        resource: Arc::new(sdk::Resource::default()),
                        instrumentation_lib: sdk::InstrumentationLibrary::new(
                            "opentelemetry-stdout-tree",
                            None,
                        ),
                    })
                    .collect(),
            );

            // We're in shutdown. So we're doing a best effort attempt to print traces and silently
            // ignore any errors.
            let _ = print::print_trace(trace);
        }
    }
}

/// Errors that occurred during span export.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Printing to stdout failed.
    #[error("write to stdout failed with {0}")]
    IOError(std::io::Error),
}

impl ExportError for Error {
    fn exporter_name(&self) -> &'static str {
        "stdout-tree"
    }
}
