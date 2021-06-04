use opentelemetry::{
    trace::{FutureExt as _, Span as _, SpanKind, StatusCode, TraceContextExt as _, Tracer as _},
    Context, Key,
};
use opentelemetry_semantic_conventions as semcov;
use std::time::Duration;
use tide::{Middleware, Next, Request};

/// Copied from https://github.com/asaaki/opentelemetry-tide to test different attributes
pub struct OtelMiddleware;

#[tide::utils::async_trait]
impl<State: Clone + Send + Sync + 'static> Middleware<State> for OtelMiddleware {
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> tide::Result {
        let method = req.method();
        let url = req.url().clone();

        let tracer = opentelemetry::global::tracer("http");
        let mut span = tracer
            .span_builder(format!("{} {}", method, url))
            .with_kind(SpanKind::Server)
            .with_attributes(vec![
                semcov::trace::HTTP_METHOD.string(method.to_string()),
                semcov::trace::HTTP_URL.string(url.to_string()),
            ])
            .start(&tracer);

        span.add_event("request.started".into(), Vec::new());

        let cx = Context::current_with_span(span);
        let res = next.run(req).with_context(cx.clone()).await;
        let span = cx.span();

        span.add_event("request.completed".into(), Vec::new());

        span.set_attribute(semcov::trace::HTTP_STATUS_CODE.i64(u16::from(res.status()).into()));
        if let Some(err) = res.error() {
            span.set_status(StatusCode::Error, err.to_string());
        }

        Ok(res)
    }
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    let _ = opentelemetry_stdout_tree::new_pipeline().install_simple();

    let mut app = tide::new();
    app.with(OtelMiddleware);
    app.at("/hello/:name").get(say_hello);
    println!("Visit http://localhost:8080/hello/your_name to see traces...");
    app.listen("127.0.0.1:8080").await?;

    opentelemetry::global::shutdown_tracer_provider();

    Ok(())
}

async fn say_hello(req: Request<()>) -> tide::Result {
    let name = req.param("name")?;

    let tracer = opentelemetry::global::tracer("http");
    let span = tracer
        .span_builder("thinking")
        .with_attributes(vec![Key::new("name").string(name.to_owned())])
        .start(&tracer);
    async_std::task::sleep(Duration::from_secs(1))
        .with_context(Context::current_with_span(span))
        .await;

    Ok(format!("Hello, {}!", name).into())
}
