//! EWS SOAP envelopes and what their answers mean.

use ozero_domain::ProviderError;
use ozero_http::Response;

use crate::xml::{self, Element};

pub const CONTENT_TYPE: &str = "text/xml; charset=utf-8";

/// Exchange 2013 SP1 and later: what `SuppressReadReceipts` and AQS search need.
const VERSION: &str = "Exchange2013_SP1";

/// A request body: `operation` inside an envelope.
pub fn envelope(operation: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<soap:Envelope xmlns:soap=\"http://schemas.xmlsoap.org/soap/envelope/\" \
xmlns:t=\"http://schemas.microsoft.com/exchange/services/2006/types\" \
xmlns:m=\"http://schemas.microsoft.com/exchange/services/2006/messages\">\
<soap:Header><t:RequestServerVersion Version=\"{VERSION}\"/></soap:Header>\
<soap:Body>{operation}</soap:Body></soap:Envelope>"
    )
}

/// The operation's answer inside the envelope, or the error the response means.
pub fn answer(response: &Response) -> Result<Element, ProviderError> {
    let root = xml::parse(&response.body);
    let fault = root.as_ref().and_then(|r| r.at(&["Body", "Fault"]));
    if response.status == 401 {
        return Err(ProviderError::Auth);
    }
    if let Some(f) = fault {
        return Err(error(
            f.text_at(&["detail", "ResponseCode"]),
            f.text_at(&["faultstring"]),
        ));
    }
    match response.status {
        200 => {}
        status @ (429 | 500..) => return Err(ProviderError::Network(status.to_string())),
        status => return Err(ProviderError::Rejected(status.to_string())),
    }
    // Taken, not copied: a message body in it may be large.
    root.and_then(|mut r| {
        let at = r.children.iter().position(|c| c.name == "Body")?;
        let mut body = r.children.swap_remove(at);
        (!body.children.is_empty()).then(|| body.children.swap_remove(0))
    })
    .ok_or_else(|| ProviderError::Protocol("bad SOAP answer".into()))
}

/// The one response message of a request for one thing, if it went through.
pub fn first<'a>(answer: &'a Element, operation: &str) -> Result<&'a Element, ProviderError> {
    let message = messages(answer)
        .next()
        .ok_or_else(|| ProviderError::Protocol(format!("no answer to {operation}")))?;
    check(message)?;
    Ok(message)
}

/// The response message for each item or folder of a request, in request order.
pub fn messages(answer: &Element) -> impl Iterator<Item = &Element> {
    answer
        .child("ResponseMessages")
        .into_iter()
        .flat_map(|m| m.children.iter())
}

/// Ok for a message that went through, or the error it reports.
pub fn check(message: &Element) -> Result<(), ProviderError> {
    match message.attr("ResponseClass") {
        Some("Error") => Err(error(
            message.text_at(&["ResponseCode"]),
            message.text_at(&["MessageText"]),
        )),
        _ => Ok(()),
    }
}

/// The error an EWS response code means.
pub fn error(code: &str, text: &str) -> ProviderError {
    match code {
        "ErrorItemNotFound"
        | "ErrorFolderNotFound"
        | "ErrorInvalidIdMalformed"
        | "ErrorInvalidIdNotAnItemAttachmentId"
        | "ErrorMoveCopyFailed" => ProviderError::Gone,
        "ErrorServerBusy"
        | "ErrorTimeoutExpired"
        | "ErrorInternalServerTransientError"
        | "ErrorMailboxMoveInProgress"
        | "ErrorConnectionFailed"
        | "ErrorMailboxStoreUnavailable" => ProviderError::Network(code.to_owned()),
        code => ProviderError::Rejected(format!("{code} {text}").trim().to_owned()),
    }
}
