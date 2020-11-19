use opentelemetry::exporter::trace::{ExportResult, SpanData};
use opentelemetry::{
    trace::{SpanId, SpanKind, StatusCode},
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

struct PrintableTrace {
    trace: HashMap<SpanId, Vec<SpanData>>,
    trace_time: Option<(SystemTime, Duration)>,
    buffer: Buffer,
    start_width: usize,
    status_width: usize,
    duration_width: usize,
    trace_time_width: usize,
}

impl PrintableTrace {
    fn print(
        trace: HashMap<SpanId, Vec<SpanData>>,
        buffer: Buffer,
        terminal_width: u16,
    ) -> Result<Buffer, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let trace_time = trace
            .get(&SpanId::invalid())
            .and_then(|spans| spans.first())
            .map(|span_data| {
                (
                    span_data.start_time,
                    span_data
                        .end_time
                        .duration_since(span_data.start_time)
                        .unwrap_or_default(),
                )
            });
        let trace_time_width = if trace_time.is_some() {
            (terminal_width / 5) as usize
        } else {
            0
        };
        let status_width = 5;
        let duration_width = 7;
        let mut trace = PrintableTrace {
            trace,
            trace_time,
            buffer,
            start_width: terminal_width as usize - status_width - duration_width - trace_time_width,
            status_width,
            duration_width,
            trace_time_width,
        };
        trace.print_spans(SpanId::invalid(), 0)?;
        Ok(trace.buffer)
    }

    fn print_spans(&mut self, span_id: SpanId, indent: usize) -> ExportResult {
        let mut spans = self.trace.remove(&span_id).unwrap_or_default();
        spans.sort_by_key(|span_data| span_data.start_time);
        for span_data in spans {
            let kind = match span_data.span_kind {
                SpanKind::Client => "CL",
                SpanKind::Server => "SE",
                SpanKind::Producer => "PR",
                SpanKind::Consumer => "CO",
                SpanKind::Internal => "IN",
            };

            let (name, details, is_err, status): (Cow<str>, Cow<str>, bool, i64) =
                if let Some(method) = span_data.attributes.get(&semcov::trace::HTTP_METHOD) {
                    let name = if let Some(url) = span_data.attributes.get(&semcov::trace::HTTP_URL)
                    {
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
                    let path = if let Some(url) = span_data.attributes.get(&semcov::trace::HTTP_URL)
                    {
                        Url::parse(&url.as_str())?.path().to_owned().into()
                    } else if let Some(route) = span_data.attributes.get(&semcov::trace::HTTP_ROUTE)
                    {
                        route.as_str()
                    } else if let Some(target) =
                        span_data.attributes.get(&semcov::trace::HTTP_TARGET)
                    {
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

            let mut start = format!(
                "{indent}{kind}  {name}  {details}",
                indent = " ".repeat(indent),
                kind = kind,
                name = name,
                details = details
            );
            start.truncate(self.start_width);

            let timing: Cow<str> = if let Some((trace_start_time, trace_duration)) = self.trace_time
            {
                let available_width = self.trace_time_width - 2;
                let scale = available_width as f64 / trace_duration.as_nanos() as f64;
                let start_len = (span_data
                    .start_time
                    .duration_since(trace_start_time)
                    .unwrap_or_default()
                    .as_nanos() as f64
                    * scale)
                    .round() as usize;
                let fill_len = ((duration.as_nanos() as f64 * scale).round() as usize).max(1);

                format!(
                    "  {start}{fill}{end}",
                    start = " ".repeat(start_len),
                    fill = "=".repeat(fill_len),
                    end = " ".repeat(available_width - start_len - fill_len)
                )
                .into()
            } else {
                "".into()
            };

            self.buffer.set_color(ColorSpec::new().set_fg(if is_err {
                Some(Color::Red)
            } else {
                None
            }))?;
            writeln!(
                self.buffer,
                "{start:start_width$}{status:>status_width$}{duration:>duration_width$}{timing}",
                start = start,
                start_width = self.start_width,
                status = status,
                status_width = self.status_width,
                duration = format_duration(duration),
                duration_width = self.duration_width,
                timing = timing
            )?;

            self.print_spans(span_data.span_context.span_id(), indent + 1)?;
        }

        Ok(())
    }
}

pub(crate) fn print_trace(trace: HashMap<SpanId, Vec<SpanData>>) -> ExportResult {
    let bufwtr = BufferWriter::stdout(ColorChoice::Auto);
    let buffer = bufwtr.buffer();

    let terminal_width = if let Some((terminal_size::Width(w), _)) = terminal_size() {
        w
    } else {
        120
    };

    let buffer = PrintableTrace::print(trace, buffer, terminal_width)?;
    bufwtr.print(&buffer)?;
    Ok(())
}
