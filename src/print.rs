use crate::format::{format_duration, format_timing};
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

/// Number of whitespace characters between columns (e.g. between status and duration).
const COLUMN_GAP: usize = 2;

/// Width of the status column. The longest expected content is an HTTP status code, i.e. 3 digits.
const STATUS_WIDTH: usize = 3;

/// Width of the duration column. The longest expected content  is 3 digits plus a 1-2 character
/// long unit, e.g. 999ms.
const DURATION_WIDTH: usize = 5;

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
    let details = span_data
        .attributes
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(" ");

    SpanStartInfo {
        name: span_data.name.as_str().into(),
        details: details.into(),
        is_err: span_data.status_code == StatusCode::Error,
        status: span_data.status_code as i64,
    }
}

fn print_event(
    event: Event,
    buffer: &mut Buffer,
    indent: usize,
    columns: &PrintColumns,
    timing_parent: &TimingParent,
) -> std::io::Result<()> {
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

    let mut start = format!(
        "{indent}{message}",
        indent = " ".repeat(indent),
        message = message
    );
    start.truncate(columns.start_width + columns.status_width + columns.duration_width);

    let timing = format_timing(
        columns.trace_time_width - COLUMN_GAP,
        timing_parent.start,
        timing_parent.duration,
        event.timestamp,
        event.timestamp,
        '·',
    );

    buffer.set_color(ColorSpec::new().set_fg(if is_exception {
        Some(Color::Red)
    } else {
        None
    }))?;
    writeln!(
        buffer,
        "{start:start_width$}{timing:>timing_width$}",
        start = start,
        start_width = columns.start_width + columns.status_width + columns.duration_width,
        timing = timing,
        timing_width = columns.trace_time_width
    )
}

fn print_span(
    span_data: &SpanData,
    buffer: &mut Buffer,
    indent: usize,
    columns: &PrintColumns,
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

    let timing = format_timing(
        columns.trace_time_width - COLUMN_GAP,
        timing_parent.start,
        timing_parent.duration,
        span_data.start_time,
        span_data.end_time,
        '=',
    );

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

#[derive(Clone, Copy)]
struct PrintColumns {
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
    columns: PrintColumns,
    timing_parent: TimingParent,
}

enum Printable {
    Event(Box<Event>),
    Span(Box<SpanData>),
}

impl PrintableTrace {
    fn print(
        mut trace: HashMap<SpanId, Vec<SpanData>>,
        mut buffer: Buffer,
        terminal_width: usize,
    ) -> std::io::Result<Buffer> {
        let status_width = STATUS_WIDTH + COLUMN_GAP;
        let duration_width = DURATION_WIDTH + COLUMN_GAP;
        let trace_time_width = terminal_width / 5;
        let columns = PrintColumns {
            start_width: terminal_width - status_width - duration_width - trace_time_width,
            status_width,
            duration_width,
            trace_time_width,
        };

        let parent_span_id = SpanId::invalid();
        let spans = trace
            .remove(&parent_span_id)
            .ok_or(std::io::ErrorKind::NotFound)?;
        for span in spans {
            let trace_start = span.start_time;
            let trace_duration = span
                .end_time
                .duration_since(span.start_time)
                .unwrap_or_default();
            let timing_parent = TimingParent {
                start: trace_start,
                duration: trace_duration,
            };

            let mut printable_trace = PrintableTrace {
                trace,
                buffer,
                columns,
                timing_parent,
            };
            printable_trace.print_span_tree(span, 0)?;
            trace = printable_trace.trace;
            buffer = printable_trace.buffer;
        }

        Ok(buffer)
    }

    fn get_child_spans(&mut self, parent_span_id: SpanId) -> Vec<SpanData> {
        self.trace.remove(&parent_span_id).unwrap_or_default()
    }

    fn print_span_tree(&mut self, span_data: SpanData, indent: usize) -> std::io::Result<()> {
        print_span(
            &span_data,
            &mut self.buffer,
            indent,
            &self.columns,
            &self.timing_parent,
        )?;

        let mut children: Vec<Printable> = self
            .get_child_spans(span_data.span_context.span_id())
            .into_iter()
            .map(|span| Printable::Span(Box::new(span)))
            .chain(
                span_data
                    .message_events
                    .into_iter()
                    .map(|event| Printable::Event(Box::new(event))),
            )
            .collect();

        children.sort_by_key(|x| match x {
            Printable::Span(span) => span.start_time,
            Printable::Event(event) => event.timestamp,
        });

        for child in children {
            match child {
                Printable::Span(span) => self.print_span_tree(*span, indent + 1)?,
                Printable::Event(event) => print_event(
                    *event,
                    &mut self.buffer,
                    indent + 1,
                    &self.columns,
                    &self.timing_parent,
                )?,
            };
        }

        Ok(())
    }
}

fn get_terminal_width() -> usize {
    if let Some((terminal_size::Width(w), _)) = terminal_size() {
        w as usize
    } else {
        80
    }
}

pub(crate) fn print_trace(trace: HashMap<SpanId, Vec<SpanData>>) -> std::io::Result<()> {
    let bufwtr = BufferWriter::stdout(ColorChoice::Auto);
    let buffer = bufwtr.buffer();

    let terminal_width = get_terminal_width();

    let buffer = PrintableTrace::print(trace, buffer, terminal_width)?;
    bufwtr.print(&buffer)?;
    Ok(())
}
