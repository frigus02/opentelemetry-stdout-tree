use opentelemetry::{sdk::export::trace::SpanData, trace::StatusCode, Value};
use opentelemetry_semantic_conventions as semcov;
use std::borrow::Cow;
use url::Url;

pub(crate) struct SemanticInfo<'a> {
    pub(crate) name: Cow<'a, str>,
    pub(crate) details: Cow<'a, str>,
    pub(crate) is_err: bool,
    pub(crate) status: i64,
}

impl<'a> From<&'a SpanData> for SemanticInfo<'a> {
    fn from(span_data: &'a SpanData) -> Self {
        get_http_span_semantic_info(span_data)
            .or_else(|| get_db_span_semantic_info(span_data))
            .unwrap_or_else(|| get_default_span_semantic_info(span_data))
    }
}

fn get_http_span_semantic_info(span_data: &SpanData) -> Option<SemanticInfo> {
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

    Some(SemanticInfo {
        name,
        details: format!("{} {}", method, path).into(),
        is_err,
        status: status_code.unwrap_or(0),
    })
}

fn get_db_span_semantic_info(span_data: &SpanData) -> Option<SemanticInfo> {
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

    Some(SemanticInfo {
        name,
        details,
        is_err: span_data.status_code == StatusCode::Error,
        status: span_data.status_code as i64,
    })
}

fn get_default_span_semantic_info(span_data: &SpanData) -> SemanticInfo {
    let details = span_data
        .attributes
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(" ");

    SemanticInfo {
        name: span_data.name.as_str().into(),
        details: details.into(),
        is_err: span_data.status_code == StatusCode::Error,
        status: span_data.status_code as i64,
    }
}
