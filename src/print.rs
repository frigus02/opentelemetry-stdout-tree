use opentelemetry::{
    sdk::export::trace::SpanData,
    trace::{Event, SpanId, SpanKind, StatusCode},
    Value,
};
use opentelemetry_semantic_conventions as semcov;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;
use std::time::{Duration, SystemTime};
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

fn format_timing(
    available_width: usize,
    timing_parent: &TimingParent,
    span_data: &SpanData,
) -> String {
    let duration = span_data
        .end_time
        .duration_since(span_data.start_time)
        .unwrap_or_default();
    let scale = available_width as f64 / timing_parent.duration.as_nanos() as f64;
    let start_len = (span_data
        .start_time
        .duration_since(timing_parent.start)
        .unwrap_or_default()
        .as_nanos() as f64
        * scale)
        .round() as usize;
    let fill_len = ((duration.as_nanos() as f64 * scale).round() as usize).max(1);

    format!(
        "{start}{fill}{end}",
        start = " ".repeat(start_len),
        fill = "=".repeat(fill_len),
        end = " ".repeat(available_width - start_len - fill_len)
    )
}

struct SpanStartInfo<'a> {
    name: Cow<'a, str>,
    details: Cow<'a, str>,
    is_err: bool,
    status: i64,
}

fn get_http_span_start_info(span_data: &SpanData) -> Option<SpanStartInfo> {
    let method = span_data
        .attributes
        .get(&semcov::trace::HTTP_METHOD)?
        .as_str();

    let name = if let Some(url) = span_data.attributes.get(&semcov::trace::HTTP_URL) {
        Url::parse(&url.as_str())
            .ok()?
            .host_str()
            .unwrap_or("")
            .to_owned()
            .into()
    } else if let Some(server_name) = span_data.attributes.get(&semcov::trace::HTTP_SERVER_NAME) {
        server_name.as_str()
    } else if let Some(host) = span_data.attributes.get(&semcov::trace::HTTP_HOST) {
        host.as_str()
    } else {
        span_data.name.as_str().into()
    };

    let path = if let Some(url) = span_data.attributes.get(&semcov::trace::HTTP_URL) {
        Url::parse(&url.as_str()).ok()?.path().to_owned().into()
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

    Some(SpanStartInfo {
        name,
        details: format!("{} {}", method, path).into(),
        is_err,
        status: status_code.unwrap_or(0),
    })
}

fn get_db_span_start_info(span_data: &SpanData) -> Option<SpanStartInfo> {
    span_data.attributes.get(&semcov::trace::DB_SYSTEM)?;

    let name = if let Some(name) = span_data.attributes.get(&semcov::trace::DB_NAME) {
        name.as_str()
    } else {
        span_data.name.as_str().into()
    };

    let details = if let Some(statement) = span_data.attributes.get(&semcov::trace::DB_STATEMENT) {
        statement.as_str()
    } else if let Some(operation) = span_data.attributes.get(&semcov::trace::DB_OPERATION) {
        operation.as_str()
    } else {
        "".into()
    };

    Some(SpanStartInfo {
        name,
        details,
        is_err: span_data.status_code == StatusCode::Error,
        status: span_data.status_code as i64,
    })
}

fn get_default_span_start_info(span_data: &SpanData) -> SpanStartInfo {
    SpanStartInfo {
        name: span_data.name.as_str().into(),
        details: "".into(),
        is_err: span_data.status_code == StatusCode::Error,
        status: span_data.status_code as i64,
    }
}

fn print_event(event: Event, buffer: &mut Buffer, indent: usize) -> std::io::Result<()> {
    let is_exception = event.name == "exception";
    let message = if is_exception {
        let exc_type = event
            .attributes
            .iter()
            .find(|kv| kv.key == semcov::trace::EXCEPTION_TYPE)
            .map_or_else(|| "unknown".into(), |kv| kv.value.as_str());
        let exc_message = event
            .attributes
            .iter()
            .find(|kv| kv.key == semcov::trace::EXCEPTION_MESSAGE)
            .map_or_else(|| "".into(), |kv| kv.value.as_str());
        format!("{}: {}", exc_type, exc_message)
    } else {
        event.name.into_owned()
    };
    buffer.set_color(ColorSpec::new().set_fg(if is_exception {
        Some(Color::Red)
    } else {
        None
    }))?;
    writeln!(
        buffer,
        "{indent}{message}",
        indent = " ".repeat(indent),
        message = message
    )
}

fn print_span(
    span_data: &SpanData,
    buffer: &mut Buffer,
    indent: usize,
    columns: &SpanColumns,
    timing_parent: &TimingParent,
) -> std::io::Result<()> {
    let kind = match span_data.span_kind {
        SpanKind::Client => "CL",
        SpanKind::Server => "SE",
        SpanKind::Producer => "PR",
        SpanKind::Consumer => "CO",
        SpanKind::Internal => "IN",
    };

    let SpanStartInfo {
        name,
        details,
        is_err,
        status,
    } = get_http_span_start_info(&span_data)
        .or_else(|| get_db_span_start_info(&span_data))
        .unwrap_or_else(|| get_default_span_start_info(&span_data));

    let mut start = format!(
        "{indent}{kind}  {name}  {details}",
        indent = " ".repeat(indent),
        kind = kind,
        name = name,
        details = details
    );
    start.truncate(columns.start_width);

    let duration = span_data
        .end_time
        .duration_since(span_data.start_time)
        .unwrap_or_default();

    let timing = format_timing(columns.trace_time_width - 2, timing_parent, span_data);

    buffer.set_color(ColorSpec::new().set_fg(if is_err { Some(Color::Red) } else { None }))?;
    writeln!(
        buffer,
        "{start:start_width$}{status:>status_width$}{duration:>duration_width$}{timing:>timing_width$}",
        start = start,
        start_width = columns.start_width,
        status = status,
        status_width = columns.status_width,
        duration = format_duration(duration),
        duration_width = columns.duration_width,
        timing = timing,
        timing_width = columns.trace_time_width
    )
}

struct SpanColumns {
    start_width: usize,
    status_width: usize,
    duration_width: usize,
    trace_time_width: usize,
}

struct TimingParent {
    start: SystemTime,
    duration: Duration,
}

struct PrintableTrace {
    trace: HashMap<SpanId, Vec<SpanData>>,
    buffer: Buffer,
    columns: SpanColumns,
    timing_parent: TimingParent,
}

impl PrintableTrace {
    fn print(
        trace: HashMap<SpanId, Vec<SpanData>>,
        buffer: Buffer,
        terminal_width: u16,
    ) -> std::io::Result<Buffer> {
        let parent_span_id = SpanId::invalid();
        let first_span = trace
            .get(&parent_span_id)
            .and_then(|span| span.first())
            .ok_or(std::io::ErrorKind::NotFound)?;
        let trace_start = first_span.start_time;
        let trace_duration = first_span
            .end_time
            .duration_since(first_span.start_time)
            .unwrap_or_default();
        let timing_parent = TimingParent {
            start: trace_start,
            duration: trace_duration,
        };

        let status_width = 5;
        let duration_width = 7;
        let trace_time_width = (terminal_width / 5) as usize;
        let columns = SpanColumns {
            start_width: terminal_width as usize - status_width - duration_width - trace_time_width,
            status_width,
            duration_width,
            trace_time_width,
        };
        let mut trace = PrintableTrace {
            trace,
            buffer,
            columns,
            timing_parent,
        };
        trace.print_spans(parent_span_id, 0)?;
        Ok(trace.buffer)
    }

    fn print_spans(&mut self, parent_span_id: SpanId, indent: usize) -> std::io::Result<()> {
        let mut spans = self.trace.remove(&parent_span_id).unwrap_or_default();
        spans.sort_by_key(|span_data| span_data.start_time);
        for span_data in spans {
            print_span(
                &span_data,
                &mut self.buffer,
                indent,
                &self.columns,
                &self.timing_parent,
            )?;

            for event in span_data.message_events {
                print_event(event, &mut self.buffer, indent + 1)?;
            }

            self.print_spans(span_data.span_context.span_id(), indent + 1)?;
        }

        Ok(())
    }
}

pub(crate) fn print_trace(trace: HashMap<SpanId, Vec<SpanData>>) -> std::io::Result<()> {
    let bufwtr = BufferWriter::stdout(ColorChoice::Auto);
    let buffer = bufwtr.buffer();

    let terminal_width = if let Some((terminal_size::Width(w), _)) = terminal_size() {
        w
    } else {
        100
    };

    let buffer = PrintableTrace::print(trace, buffer, terminal_width)?;
    bufwtr.print(&buffer)?;
    Ok(())
}
