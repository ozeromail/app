//! Gmail batch requests: many GETs in one multipart/mixed round trip.

use ozero_http::Response;

const BOUNDARY: &str = "ozero-batch";

/// The media type of a batch request body.
pub const CONTENT_TYPE: &str = "multipart/mixed; boundary=ozero-batch";

/// A batch body asking for each path in turn; part `i` is named by its index.
pub fn request(paths: &[String]) -> String {
    let mut body = String::new();
    for (i, path) in paths.iter().enumerate() {
        body.push_str(&format!(
            "--{BOUNDARY}\r\nContent-Type: application/http\r\nContent-ID: <{i}>\r\n\r\nGET {path}\r\n\r\n"
        ));
    }
    body.push_str(&format!("--{BOUNDARY}--\r\n"));
    body
}

/// The answers to a batch of `count` requests, in request order. None if the body is not a
/// batch response that answers each of them once.
pub fn response(content_type: &str, body: &[u8], count: usize) -> Option<Vec<Response>> {
    let boundary = content_type
        .split(';')
        .filter_map(|p| p.split_once('='))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("boundary"))?
        .1
        .trim()
        .trim_matches('"');
    // A delimiter starts a line; the first may start the body.
    let delimiter = format!("\r\n--{boundary}");
    let delimiter = delimiter.as_bytes();
    let body = [b"\r\n", body].concat();
    let mut answers: Vec<Option<Response>> = (0..count).map(|_| None).collect();
    let mut rest = &body[find(&body, delimiter)? + delimiter.len()..];
    while !rest.starts_with(b"--") {
        let end = find(rest, delimiter)?;
        let (index, response) = part(&rest[..end])?;
        let slot = answers.get_mut(index)?;
        if slot.replace(response).is_some() {
            return None;
        }
        rest = &rest[end + delimiter.len()..];
    }
    answers.into_iter().collect()
}

/// `Content-ID: <response-N>`, an empty line, then an HTTP response.
fn part(part: &[u8]) -> Option<(usize, Response)> {
    let part = part.trim_ascii();
    let end = find(part, b"\r\n\r\n")?;
    let head = std::str::from_utf8(&part[..end]).ok()?;
    let index = head
        .lines()
        .filter_map(|l| l.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("Content-ID"))?
        .1
        .trim()
        .strip_prefix("<response-")?
        .strip_suffix('>')?
        .parse()
        .ok()?;
    let mut response = ozero_http::parse(&part[end + 4..])?;
    response.body = response.body.trim_ascii().to_vec();
    Some((index, response))
}

fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPE: &str = "multipart/mixed; Boundary=\"batch_X\"";

    #[test]
    fn asks_for_each_path() {
        let body = request(&["/a?x=1".into(), "/b".into()]);
        assert_eq!(
            body,
            "--ozero-batch\r\nContent-Type: application/http\r\nContent-ID: <0>\r\n\r\nGET /a?x=1\r\n\r\n\
             --ozero-batch\r\nContent-Type: application/http\r\nContent-ID: <1>\r\n\r\nGET /b\r\n\r\n\
             --ozero-batch--\r\n"
        );
    }

    #[test]
    fn puts_the_answers_in_request_order() {
        let body =
            b"\r\n--batch_X\r\nContent-Type: application/http\r\nContent-ID: <response-1>\r\n\r\n\
            HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\n\r\n{\"error\":{}}\r\n\
            --batch_X\r\nContent-Type: application/http\r\nContent-ID: <response-0>\r\n\r\n\
            HTTP/1.1 200 OK\r\n\r\n{\"id\":\"a--batch_X\"}\r\n--batch_X--\r\n";
        let answers = response(TYPE, body, 2).unwrap();
        let got: Vec<_> = answers
            .iter()
            .map(|r| (r.status, r.body.as_slice()))
            .collect();
        assert_eq!(
            got,
            [
                (200, &b"{\"id\":\"a--batch_X\"}"[..]),
                (404, &b"{\"error\":{}}"[..])
            ]
        );
    }

    #[test]
    fn refuses_what_is_not_a_whole_batch() {
        let one = b"--batch_X\r\nContent-ID: <response-0>\r\n\r\nHTTP/1.1 200 OK\r\n\r\n{}\r\n--batch_X--";
        assert!(response(TYPE, one, 1).is_some());
        assert_eq!(response(TYPE, one, 2), None);
        assert_eq!(response("application/json", one, 1), None);
        assert_eq!(response(TYPE, b"{\"id\":\"a\"}", 1), None);
    }
}
