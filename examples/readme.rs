use opentelemetry::trace::{SpanKind, StatusCode, TraceContextExt as _, Tracer};
use opentelemetry_semantic_conventions as semcov;
use std::{thread, time::Duration};

fn main() {
    let tracer = opentelemetry_stdout_tree::new_pipeline().install_simple();

    let span = tracer
        .span_builder("request")
        .with_kind(SpanKind::Server)
        .with_attributes(vec![
            semcov::trace::HTTP_METHOD.string("GET"),
            semcov::trace::HTTP_FLAVOR.string("1.1"),
            semcov::trace::HTTP_TARGET.string("/authors/6d50807b-80e6-4802-b01e-3e78137a0fc9/books/d13d226c-c600-42c9-bb9d-96395c5e9351"),
            semcov::trace::HTTP_HOST.string("my-awesome-books.com:443"),
            semcov::trace::HTTP_SERVER_NAME.string("my-awesome-books.com"),
            semcov::trace::NET_HOST_PORT.i64(443),
            semcov::trace::HTTP_SCHEME.string("https"),
            semcov::trace::HTTP_ROUTE.string("/authors/:authorId/books/:bookId"),
            semcov::trace::HTTP_STATUS_CODE.i64(500),
            semcov::trace::HTTP_CLIENT_IP.string("192.0.2.4"),
            semcov::trace::NET_PEER_IP.string("192.0.2.5"),
            semcov::trace::HTTP_USER_AGENT.string("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:72.0) Gecko/20100101 Firefox/72.0"),
        ])
        .start(&tracer);
    tracer.with_span(span, |_cx| {
        let span = tracer
            .span_builder("middleware - expressInit")
            .with_kind(SpanKind::Internal)
            .start(&tracer);
        tracer.with_span(span, |_cx| {});

        let span = tracer
            .span_builder("middleware - query")
            .with_kind(SpanKind::Internal)
            .start(&tracer);
        tracer.with_span(span, |_cx| {});

        let span = tracer
            .span_builder("middleware - session")
            .with_kind(SpanKind::Internal)
            .start(&tracer);
        tracer.with_span(span, |_cx| {
            let span = tracer
                .span_builder("pg-pool.connect")
                .with_kind(SpanKind::Client)
                .start(&tracer);
            tracer.with_span(span, |_cx| {
                thread::sleep(Duration::from_millis(300));
            });

            let span = tracer
                .span_builder("get session")
                .with_kind(SpanKind::Client)
                .with_attributes(vec![
                    semcov::trace::DB_SYSTEM.string("postgresql"),
                    semcov::trace::DB_CONNECTION_STRING
                        .string("postgresql://user@localhost/sessions"),
                    semcov::trace::DB_USER.string("user"),
                    semcov::trace::NET_PEER_NAME.string("localhost"),
                    semcov::trace::NET_PEER_IP.string("127.0.0.1"),
                    semcov::trace::NET_PEER_PORT.i64(5432),
                    semcov::trace::NET_TRANSPORT.string("IP.TCP"),
                    semcov::trace::DB_NAME.string("sessions"),
                    semcov::trace::DB_STATEMENT.string(
                        "SELECT sess FROM \"session\" WHERE sid = $1 AND expire >= to_timestamp($2)",
                    ),
                ])
                .start(&tracer);
            tracer.with_span(span, |_cx| {
                thread::sleep(Duration::from_millis(200));
            });
        });

        let span = tracer
            .span_builder("middleware - initialize")
            .with_kind(SpanKind::Internal)
            .start(&tracer);
        tracer.with_span(span, |_cx| {});

        let span = tracer
            .span_builder("middleware - authenticate")
            .with_kind(SpanKind::Internal)
            .start(&tracer);
        tracer.with_span(span, |cx| {
            cx.span().add_event("user authenticated", vec![
                semcov::trace::ENDUSER_ID.string("42")
            ]);
        });

        let span = tracer
            .span_builder("request handler - /authors/:authorId/books/:bookId")
            .with_kind(SpanKind::Internal)
            .start(&tracer);
        tracer.with_span(span, |_cx| {
            let span = tracer
                .span_builder("get book")
                .with_kind(SpanKind::Client)
                .with_attributes(vec![
                    semcov::trace::HTTP_METHOD.string("POST"),
                    semcov::trace::HTTP_FLAVOR.string("1.1"),
                    semcov::trace::HTTP_URL
                        .string("http://book-service.book-service/graphql"),
                    semcov::trace::NET_PEER_IP.string("192.0.2.5"),
                    semcov::trace::HTTP_STATUS_CODE.i64(200),
                ])
                .start(&tracer);
            tracer.with_span(span, |_cx| {
                thread::sleep(Duration::from_millis(5));

                let span = tracer
                    .span_builder("request")
                    .with_kind(SpanKind::Server)
                    .with_attributes(vec![
                        semcov::trace::HTTP_METHOD.string("POST"),
                        semcov::trace::HTTP_FLAVOR.string("1.1"),
                        semcov::trace::HTTP_TARGET.string("/graphql"),
                        semcov::trace::HTTP_HOST.string("book-service.book-service:443"),
                        semcov::trace::HTTP_SERVER_NAME.string("book-service.book.service"),
                        semcov::trace::NET_HOST_PORT.i64(80),
                        semcov::trace::HTTP_SCHEME.string("http"),
                        semcov::trace::HTTP_ROUTE.string("/graphql"),
                        semcov::trace::HTTP_STATUS_CODE.i64(200),
                        semcov::trace::HTTP_CLIENT_IP.string("192.0.2.4"),
                        semcov::trace::NET_PEER_IP.string("192.0.2.5"),
                    ])
                    .start(&tracer);
                tracer.with_span(span, |_cx| {
                    let span = tracer
                        .span_builder("query")
                        .with_kind(SpanKind::Internal)
                        .start(&tracer);
                    tracer.with_span(span, |_cx| {
                        let span = tracer
                            .span_builder("field")
                            .with_kind(SpanKind::Internal)
                            .with_status_code(StatusCode::Error)
                            .start(&tracer);
                        tracer.with_span(span, |cx| {
                            let err: Box<dyn std::error::Error> = "something went wrong".into();
                            cx.span().record_exception(err.as_ref());
                        });
                    });

                    let span = tracer
                        .span_builder("parse")
                        .with_kind(SpanKind::Internal)
                        .start(&tracer);
                    tracer.with_span(span, |_cx| {});

                    let span = tracer
                        .span_builder("validation")
                        .with_kind(SpanKind::Internal)
                        .start(&tracer);
                    tracer.with_span(span, |_cx| {});
                });

                thread::sleep(Duration::from_millis(21));
            });
        });
    });

    opentelemetry::global::shutdown_tracer_provider();
}
