use crate::{
    format::{format_duration, format_timing},
    semantics::SemanticInfo,
};
use opentelemetry::{
    sdk::export::trace::SpanData,
    trace::{Event, SpanId, SpanKind},
};
use opentelemetry_semantic_conventions as semcov;
use std::collections::HashMap;
use std::io::Write;
use std::time::{Duration, SystemTime};
use termcolor::{Buffer, BufferWriter, Color, ColorChoice, ColorSpec, WriteColor};
use terminal_size::terminal_size;

/// Number of whitespace characters between columns (e.g. between status and duration).
const COLUMN_GAP: usize = 2;

/// Minimum width of the start column. Should have enough space to display "kind" (2 characters),
/// gap (see above) and some part of the span name.
///
/// 10 was chosen arbitrarily.
const MIN_START_WIDTH: usize = 10;

/// Width of the status column. The longest expected content is an HTTP status code, i.e. 3 digits.
const STATUS_WIDTH: usize = 3;

/// Width of the duration column. The longest expected content  is 3 digits plus a 1-2 character
/// long unit, e.g. 999ms.
const DURATION_WIDTH: usize = 5;

#[derive(Clone, Copy)]
struct Columns {
    start_width: usize,
    status_width: usize,
    duration_width: usize,
    timing_width: usize,
}

impl Columns {
    fn new(terminal_width: usize, timing_column_width: f64) -> Self {
        let status_width = STATUS_WIDTH + COLUMN_GAP;
        let duration_width = DURATION_WIDTH + COLUMN_GAP;
        let timing_width = ((terminal_width as f64 * timing_column_width).round() as usize).clamp(
            0,
            terminal_width - MIN_START_WIDTH - status_width - duration_width,
        );
        Self {
            start_width: terminal_width - status_width - duration_width - timing_width,
            status_width,
            duration_width,
            timing_width,
        }
    }
}

struct TimingParent {
    start: SystemTime,
    duration: Duration,
}

impl TimingParent {
    fn new(start: SystemTime, end: SystemTime) -> Self {
        let duration = end.duration_since(start).unwrap_or_default();
        Self { start, duration }
    }
}

fn get_color(is_err: bool) -> ColorSpec {
    let mut color = ColorSpec::new();
    color.set_fg(if is_err { Some(Color::Red) } else { None });
    color
}

struct PrintContext<'a> {
    buffer: &'a mut Buffer,
    columns: Columns,
    timing_parent: TimingParent,
}

impl<'a> PrintContext<'a> {
    fn print_event(&mut self, event: Event, indent: usize) -> std::io::Result<()> {
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
        start.truncate(
            self.columns.start_width + self.columns.status_width + self.columns.duration_width,
        );

        let timing = if self.columns.timing_width > COLUMN_GAP {
            format_timing(
                self.columns.timing_width - COLUMN_GAP,
                self.timing_parent.start,
                self.timing_parent.duration,
                event.timestamp,
                Duration::from_nanos(0),
                '·',
            )
        } else {
            "".into()
        };

        self.buffer.set_color(&get_color(is_exception))?;
        writeln!(
            self.buffer,
            "{start:start_width$}{timing:>timing_width$}",
            start = start,
            start_width =
                self.columns.start_width + self.columns.status_width + self.columns.duration_width,
            timing = timing,
            timing_width = self.columns.timing_width
        )
    }

    fn print_span(&mut self, span_data: &SpanData, indent: usize) -> std::io::Result<()> {
        let kind = match span_data.span_kind {
            SpanKind::Client => "CL",
            SpanKind::Server => "SE",
            SpanKind::Producer => "PR",
            SpanKind::Consumer => "CO",
            SpanKind::Internal => "IN",
        };

        let SemanticInfo {
            name,
            details,
            is_err,
            status,
        } = SemanticInfo::from(span_data);

        let mut start = format!(
            "{indent}{kind}  {name}  {details}",
            indent = " ".repeat(indent),
            kind = kind,
            name = name,
            details = details
        );
        start.truncate(self.columns.start_width);

        let duration = span_data
            .end_time
            .duration_since(span_data.start_time)
            .unwrap_or_default();

        let timing = if self.columns.timing_width > COLUMN_GAP {
            format_timing(
                self.columns.timing_width - COLUMN_GAP,
                self.timing_parent.start,
                self.timing_parent.duration,
                span_data.start_time,
                duration,
                '=',
            )
        } else {
            "".into()
        };

        self.buffer.set_color(&get_color(is_err))?;
        writeln!(
            self.buffer,
            "{start:start_width$}{status:>status_width$}{duration:>duration_width$}{timing:>timing_width$}",
            start = start,
            start_width = self.columns.start_width,
            status = status,
            status_width = self.columns.status_width,
            duration = format_duration(duration),
            duration_width = self.columns.duration_width,
            timing = timing,
            timing_width = self.columns.timing_width
        )
    }
}

enum Printable {
    Event(Box<Event>),
    Span(Box<SpanData>),
}

impl Printable {
    fn merge_lists(
        spans: impl IntoIterator<Item = SpanData>,
        events: impl IntoIterator<Item = Event>,
    ) -> Vec<Printable> {
        let mut merged: Vec<Printable> = spans
            .into_iter()
            .map(|span| Printable::Span(Box::new(span)))
            .chain(
                events
                    .into_iter()
                    .map(|event| Printable::Event(Box::new(event))),
            )
            .collect();
        merged.sort_by_key(|x| match x {
            Printable::Span(span) => span.start_time,
            Printable::Event(event) => event.timestamp,
        });
        merged
    }
}

struct PrintableTrace(HashMap<SpanId, Vec<SpanData>>);

impl PrintableTrace {
    fn new(trace: HashMap<SpanId, Vec<SpanData>>) -> Self {
        Self(trace)
    }

    fn print(
        mut self,
        buffer: &mut Buffer,
        terminal_width: usize,
        timing_column_width: f64,
    ) -> std::io::Result<()> {
        let columns = Columns::new(terminal_width, timing_column_width);

        let parent_span_id = SpanId::invalid();
        let spans = self.consume_child_spans(parent_span_id);
        for span in spans {
            let timing_parent = TimingParent::new(span.start_time, span.end_time);
            let mut context = PrintContext {
                buffer,
                columns,
                timing_parent,
            };
            self.print_span_tree(&mut context, span, 0)?;
        }

        Ok(())
    }

    fn consume_child_spans(&mut self, parent_span_id: SpanId) -> Vec<SpanData> {
        self.0.remove(&parent_span_id).unwrap_or_default()
    }

    fn print_span_tree(
        &mut self,
        context: &mut PrintContext,
        span_data: SpanData,
        indent: usize,
    ) -> std::io::Result<()> {
        context.print_span(&span_data, indent)?;

        let child_spans = self.consume_child_spans(span_data.span_context.span_id());
        let child_events = span_data.events;
        let children = Printable::merge_lists(child_spans, child_events);

        for child in children {
            match child {
                Printable::Span(span) => self.print_span_tree(context, *span, indent + 1)?,
                Printable::Event(event) => context.print_event(*event, indent + 1)?,
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

pub(crate) fn print_trace(
    trace: HashMap<SpanId, Vec<SpanData>>,
    timing_column_width: f64,
) -> std::io::Result<()> {
    let bufwtr = BufferWriter::stdout(ColorChoice::Auto);
    let mut buffer = bufwtr.buffer();

    let terminal_width = get_terminal_width();

    PrintableTrace::new(trace).print(&mut buffer, terminal_width, timing_column_width)?;
    bufwtr.print(&buffer)?;
    Ok(())
}
