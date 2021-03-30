[![Crates.io](https://img.shields.io/crates/v/opentelemetry-stdout-tree.svg)](https://crates.io/crates/opentelemetry-stdout-tree)
[![Documentation](https://docs.rs/opentelemetry-stdout-tree/badge.svg)](https://docs.rs/opentelemetry-stdout-tree)
[![Workflow Status](https://github.com/frigus02/opentelemetry-stdout-tree/workflows/CI/badge.svg)](https://github.com/frigus02/opentelemetry-stdout-tree/actions?query=workflow%3A%22CI%22)

# opentelemetry-stdout-tree

An stdout exporter implementation for [OpenTelemetry Rust], which prints traces in a tree-like
format.

[opentelemetry rust]: https://github.com/open-telemetry/opentelemetry-rust

```
SE  my-awesome-books.com  GET /authors/:authorId/boo  500  584ms  ==================
 IN  middleware - expressInit                           0      0  =
 IN  middleware - query                                 0      0  =
 IN  middleware - session                               0  523ms  ================
  CL  pg-pool.connect                                   0  303ms  =========
  CL  sessions  SELECT sess FROM "session" WHERE sid    0  219ms           =======
 IN  middleware - initialize                            0      0                  =
 IN  middleware - authenticate                          0      0                  =
  user authenticated                                                              ·
 IN  request handler - /authors/:authorId/books/:boo    0   59ms                  ==
  CL  book-service.book-service  POST /graphql        200   59ms                  ==
   SE  book-service.book.service  POST /graphql       200      0                   =
    IN  query                                           0      0                   =
     IN  field                                          2      0                   =
      unknown: something went wrong                                                ·
    IN  parse                                           0      0                   =
    IN  validation                                      0      0                   =
```

## Usage

Configure an OpenTelemetry pipeline and start creating spans:

```rust
use opentelemetry::trace::Tracer as _;

let tracer = opentelemetry_stdout_tree::new_pipeline().install_simple();
tracer.in_span("main", |_cx| {});
```

### Features

The function `install` automatically configures an asynchronous batch exporter if you enable
either the **async-std** or **tokio** feature for the `opentelemetry` crate. Otherwise spans
will be exported synchronously.

## Attribute mapping

The exporter makes use of [OpenTelemetry semantic conventions] to provide more useful output
for known types of spans. Currently supported are:

- HTTP: Shows method, host and path and uses status code to determine errors.
- DB: Shows database name and statement or operation.
- Exception events: shows exception type and message.

[opentelemetry semantic conventions]: https://github.com/open-telemetry/opentelemetry-specification/tree/master/specification/trace/semantic_conventions
