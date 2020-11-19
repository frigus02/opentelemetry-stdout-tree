use opentelemetry::exporter::trace::{ExportResult, SpanData};
use opentelemetry::trace::SpanId;
use opentelemetry_semantic_conventions as semcov;
use std::collections::HashMap;
use std::io::Write;
use termcolor::{Buffer, BufferWriter, Color, ColorChoice, ColorSpec, WriteColor};
use terminal_size::terminal_size;

fn print_spans(
    trace: &mut HashMap<SpanId, Vec<SpanData>>,
    span_id: SpanId,
    indent: usize,
    buffer: &mut Buffer,
) -> ExportResult {
    let mut spans = trace.remove(&span_id).unwrap_or_default();
    spans.sort_by_key(|span_data| span_data.start_time);
    for span_data in spans {
        write!(buffer, "{}", " ".repeat(indent * 2))?;
        if let Some(method) = span_data.attributes.get(&semcov::trace::HTTP_METHOD) {
            let method = method.as_str();
            let url = span_data
                .attributes
                .get(&semcov::trace::HTTP_URL)
                .map(|v| v.as_str())
                .unwrap_or_else(|| "".into());
            let status_code = span_data
                .attributes
                .get(&semcov::trace::HTTP_STATUS_CODE)
                .map(|v| v.as_str())
                .unwrap_or_else(|| "".into());
            buffer.set_color(ColorSpec::new().set_fg(Some(Color::Blue)))?;
            write!(buffer, "{} {}", method, url)?;
            buffer.set_color(ColorSpec::new().set_fg(None))?;
            write!(buffer, " --> ")?;
            buffer.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
            writeln!(buffer, "{}", status_code)?;
        } else {
            buffer.set_color(ColorSpec::new().set_fg(None))?;
            writeln!(buffer, "{}", span_data.name)?;
        }

        print_spans(trace, span_data.span_context.span_id(), indent + 1, buffer)?;
    }

    Ok(())
}

pub(crate) fn print_trace(mut trace: HashMap<SpanId, Vec<SpanData>>) -> ExportResult {
    let bufwtr = BufferWriter::stdout(ColorChoice::Auto);
    let mut buffer = bufwtr.buffer();

    let size = terminal_size();
    if let Some((terminal_size::Width(w), _)) = size {
        writeln!(&mut buffer, "TERMINAL: {}", w)?;
    } else {
        writeln!(&mut buffer, "TERMINAL: unable to get size")?;
    }

    // server = 🖥
    // client = 📱
    // internal = ⚙
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

    print_spans(&mut trace, SpanId::invalid(), 0, &mut buffer)?;

    bufwtr.print(&buffer)?;
    Ok(())
}
