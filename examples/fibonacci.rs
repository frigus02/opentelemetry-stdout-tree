// Adapted from
// https://github.com/tokio-rs/tracing/blob/df1d626a2719ad6aa38d47b2984d0569af0b7525/examples/examples/attrs-args.rs

use opentelemetry::{
    global,
    trace::{get_active_span, TraceContextExt as _, Tracer},
    Context, Key,
};

const TRACER_NAME: &str = "fibonacci";

fn debug(msg: impl Into<String>) {
    if std::env::var_os("DEBUG").is_some() {
        info(msg);
    }
}

fn info(msg: impl Into<String>) {
    get_active_span(|span| {
        span.add_event(msg.into(), Default::default());
    });
}

fn function_span<T, F>(name: &'static str, arg1: u64, f: F) -> T
where
    F: FnOnce() -> T,
{
    let tracer = global::tracer(TRACER_NAME);
    let span = tracer
        .span_builder(name)
        .with_attributes(vec![Key::from("arg1").string(arg1.to_string())])
        .start(&tracer);
    let _guard = Context::current_with_span(span).attach();
    f()
}

fn nth_fibonacci(n: u64) -> u64 {
    function_span("nth_fibonacci", n, || {
        if n == 0 || n == 1 {
            debug("Base case");
            1
        } else {
            debug("Recursing");
            nth_fibonacci(n - 1) + nth_fibonacci(n - 2)
        }
    })
}

fn fibonacci_seq(to: u64) -> Vec<u64> {
    function_span("fibonacci_seq", to, || {
        let mut sequence = vec![];

        for n in 0..=to {
            debug(format!("Pushing {n} fibonacci", n = n));
            sequence.push(nth_fibonacci(n));
        }

        sequence
    })
}

fn main() {
    let _ = opentelemetry_stdout_tree::new_pipeline()
        .with_timing_column_width(0.5)
        .install_simple();

    global::tracer(TRACER_NAME).in_span("root", |_| {
        let mut args = std::env::args();
        let _process_name = args.next().expect("0th argument should exist");
        let n = args
            .next()
            .map(|n| n.parse::<u64>().expect(""))
            .unwrap_or(5);
        let sequence = fibonacci_seq(n);
        info(format!(
            "The first {} fibonacci numbers are {:?}",
            n, sequence
        ));
    });

    global::shutdown_tracer_provider();
}
