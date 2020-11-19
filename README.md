# opentelemetry-stdout-tree

**NOTE:** This is not released, yet.

An stdout exporter implementation for [OpenTelemetry Rust], which prints traces
in a tree-like format.

[opentelemetry rust]: https://github.com/open-telemetry/opentelemetry-rust

```
SE  my-awesome-books.com  GET /authors/:authorId/books/:bookId         500  586ms  ==================
 IN  middleware - expressInit                                            1      0  =
 IN  middleware - query                                                  1      0  =
 IN  middleware - session                                                1  539ms  =================
  CL  pg-pool.connect                                                    1  407ms  =============
  CL  sessions  SELECT sess FROM "session" WHERE sid = $1 AND expire     1  131ms               ====
 IN  middleware - initialize                                             1      0                   =
 IN  middleware - authenticate                                           1      0                   =
  user authenticated
 IN  request handler - /authors/:authorId/books/:bookId                  1   46ms                   =
  CL  book-service.book-service  POST /graphql                         200   46ms                   =
   SE  book-service.book.service  POST /graphql                        200      0                   =
    IN  query                                                            1      0                   =
     IN  field                                                           2      0                   =
      unknown: something went wrong
    IN  parse                                                            1      0                   =
    IN  validation                                                       1      0                   =
```

## Usage

Configure an OpenTelemetry pipeline and start creating spans:

```rust
use opentelemetry::{trace::Tracer as _, sdk::trace::Tracer};

fn main() {
    let (tracer, _uninstall) = opentelemetry_stdout_tree::new_pipeline().install();
    tracer.in_span("main", |_cx| {});
}
```

### Features

The function `install` automatically configures an asynchronous batch exporter
if you enable either the **async-std** or **tokio** feature for the
`opentelemetry` crate. Otherwise spans will be exported synchronously.

## Attribute mapping

The exporter makes use of [OpenTelemetry semantic conventions] to provide more
useful output for known types of spans. Currently supported are:

- HTTP: Shows method, host and path and uses status code to determine errors.
- DB: Shows database name and statement or operation.
- Exception events: shows exception type and message.

[opentelemetry semantic conventions]: https://github.com/open-telemetry/opentelemetry-specification/tree/master/specification/trace/semantic_conventions
