use opentelemetry::exporter::trace::{ExportResult, SpanData};
use opentelemetry::{
    trace::{SpanId, SpanKind, StatusCode},
    Value,
};
use opentelemetry_semantic_conventions as semcov;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;
use std::time::Duration;
use termcolor::{Buffer, BufferWriter, Color, ColorChoice, ColorSpec, WriteColor};
use terminal_size::terminal_size;
use url::Url;

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs > 7200 {
        format!("{}h", secs / 3600)
    } else if secs > 120 {
        format!("{}m", secs / 60)
    } else if secs > 0 {
        format!("{}s", d.as_secs())
    } else if d.as_millis() > 0 {
        format!("{}ms", d.as_millis())
    } else {
        "0".into()
    }
}

fn print_spans(
    trace: &mut HashMap<SpanId, Vec<SpanData>>,
    span_id: SpanId,
    indent: usize,
    buffer: &mut Buffer,
    terminal_width: u16,
) -> ExportResult {
    let mut spans = trace.remove(&span_id).unwrap_or_default();
    spans.sort_by_key(|span_data| span_data.start_time);
    for span_data in spans {
        let kind = match span_data.span_kind {
            SpanKind::Client => "CL",
            SpanKind::Server => "SE",
            SpanKind::Producer => "PR",
            SpanKind::Consumer => "CO",
            SpanKind::Internal => "IN",
        };

        let (name, details, is_err, status_code): (Cow<str>, Cow<str>, bool, i64) =
            if let Some(method) = span_data.attributes.get(&semcov::trace::HTTP_METHOD) {
                let name = if let Some(url) = span_data.attributes.get(&semcov::trace::HTTP_URL) {
                    Url::parse(&url.as_str())?
                        .host_str()
                        .unwrap_or("")
                        .to_owned()
                        .into()
                } else if let Some(server_name) =
                    span_data.attributes.get(&semcov::trace::HTTP_SERVER_NAME)
                {
                    server_name.as_str()
                } else if let Some(host) = span_data.attributes.get(&semcov::trace::HTTP_HOST) {
                    host.as_str()
                } else {
                    span_data.name.into()
                };
                let method = method.as_str();
                let path = if let Some(url) = span_data.attributes.get(&semcov::trace::HTTP_URL) {
                    Url::parse(&url.as_str())?.path().to_owned().into()
                } else if let Some(route) = span_data.attributes.get(&semcov::trace::HTTP_ROUTE) {
                    route.as_str()
                } else if let Some(target) = span_data.attributes.get(&semcov::trace::HTTP_TARGET) {
                    target.as_str()
                } else {
                    "".into()
                };
                let status_code = span_data
                    .attributes
                    .get(&semcov::trace::HTTP_STATUS_CODE)
                    .and_then(|v| match v {
                        Value::I64(v) => Some(*v),
                        Value::F64(v) => Some(*v as i64),
                        Value::String(v) => i64::from_str_radix(v, 10).ok(),
                        _ => None,
                    });
                let is_err = status_code
                    .map(|status_code| status_code >= 400)
                    .unwrap_or(span_data.status_code == StatusCode::Error);
                (
                    name,
                    format!("{} {}", method, path).into(),
                    is_err,
                    status_code.unwrap_or(0),
                )
            } else {
                (
                    span_data.name.into(),
                    "".into(),
                    span_data.status_code == StatusCode::Error,
                    span_data.status_code as i64,
                )
            };

        let duration = span_data
            .end_time
            .duration_since(span_data.start_time)
            .unwrap_or_default();

        buffer.set_color(ColorSpec::new().set_fg(if is_err { Some(Color::Red) } else { None }))?;
        let mut start = format!(
            "{indent}{kind} {name} {details}",
            indent = " ".repeat(indent),
            kind = kind,
            name = name,
            details = details
        );
        let start_width = (terminal_width - 2 - 3 - 2 - 5) as usize;
        start.truncate(start_width);
        writeln!(
            buffer,
            "{start:start_width$}  {status_code:3}  {duration:5}",
            start = start,
            start_width = start_width,
            status_code = status_code,
            duration = format_duration(duration)
        )?;

        print_spans(
            trace,
            span_data.span_context.span_id(),
            indent + 1,
            buffer,
            terminal_width,
        )?;
    }

    Ok(())
}

pub(crate) fn print_trace(mut trace: HashMap<SpanId, Vec<SpanData>>) -> ExportResult {
    let bufwtr = BufferWriter::stdout(ColorChoice::Auto);
    let mut buffer = bufwtr.buffer();

    let terminal_width = if let Some((terminal_size::Width(w), _)) = terminal_size() {
        w
    } else {
        120
    };

    // SE beta.future.nhs.uk  GET /authors/:authorId/books/:bookId           500  624ms ===========
    //   IN middleware - expressInit                                         0    0     =
    //   IN middleware - query                                               0    0     =
    //   IN middleware - session                                             0    515ms ========
    //     CL pg-pool.connect                                                0    399ms ======
    //     CL session  SQL: SELECT sess FROM "session" WHERE sid = $1 AND e  0    116ms       ==
    //   IN middleware - initialize                                          0    0             =
    //   IN middleware - authenticate                                        0    0             =
    //   IN request handler - /authors/:authorId/books/:bookId               0    0             =
    //     CL book-service.book-service POST /graphql                        200  26ms          ===
    //       SE book-service.book-service POST /graphql                      200  0.0ms          =
    //         IN query                                                      0    0              =
    //           IN field                                                    2    0              =
    //         IN parse                                                      0    0              =
    //         IN validation                                                 0    0              =

    print_spans(
        &mut trace,
        SpanId::invalid(),
        0,
        &mut buffer,
        terminal_width,
    )?;

    bufwtr.print(&buffer)?;
    Ok(())
}
