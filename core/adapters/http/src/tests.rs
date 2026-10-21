use tokio::io::{DuplexStream, duplex};

use super::*;

/// A connection whose server side has already written `reply` and then closed.
async fn served(reply: &[u8]) -> Conn<DuplexStream> {
    let (client, mut server) = duplex(1 << 20);
    server.write_all(reply).await.unwrap();
    drop(server);
    Conn::new(client, MAX_BODY)
}

async fn read(reply: &[u8]) -> Result<(Response, bool), ProviderError> {
    served(reply)
        .await
        .response()
        .await
        .map_err(ProviderError::from)
}

#[tokio::test]
async fn reads_a_fixed_length_body() {
    let (response, keep) = read(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; Charset=A\r\nContent-Length: 5\r\n\r\nhello",
    )
    .await
    .unwrap();
    assert_eq!(
        response,
        Response {
            status: 200,
            content_type: "text/plain; Charset=A".into(),
            location: String::new(),
            body: b"hello".to_vec()
        }
    );
    assert!(keep);
}

#[tokio::test]
async fn reads_chunks_and_trailers() {
    let reply = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
        5;ext=1\r\nhello\r\n6\r\n world\r\n0\r\nX-Trailer: 1\r\n\r\n";
    let (response, keep) = read(reply).await.unwrap();
    assert_eq!(response.body, b"hello world");
    assert!(keep);
}

#[tokio::test]
async fn reads_to_the_end_without_a_length() {
    let (response, keep) = read(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nall of it")
        .await
        .unwrap();
    assert_eq!(response.body, b"all of it");
    assert!(!keep);
}

#[tokio::test]
async fn has_no_body_for_204_and_skips_100() {
    let (response, keep) = read(b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 204 No Content\r\n\r\n")
        .await
        .unwrap();
    assert_eq!(
        response,
        Response {
            status: 204,
            content_type: String::new(),
            location: String::new(),
            body: Vec::new()
        }
    );
    assert!(keep);
}

#[tokio::test]
async fn closes_when_the_server_says_so() {
    let (_, keep) = read(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    assert!(!keep);
}

#[tokio::test]
async fn refuses_broken_responses() {
    for reply in [
        &b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nshort"[..],
        b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nab",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 99999999999\r\n\r\n",
        b"nonsense\r\n\r\n",
    ] {
        assert!(
            read(reply).await.is_err(),
            "{}",
            String::from_utf8_lossy(reply)
        );
    }
}

#[tokio::test]
async fn tells_a_closed_connection_from_a_broken_response() {
    let mut conn = served(b"").await;
    assert!(matches!(conn.response().await, Err(Failure::Closed)));
    let mut conn = served(b"HTTP/1.1 200").await;
    assert!(matches!(conn.response().await, Err(Failure::Error(_))));
}

#[test]
fn writes_the_request() {
    let body = Body {
        content_type: "application/json",
        data: b"{}",
    };
    let sent = request(
        "POST",
        "example.com",
        "/a?b=c",
        &[("Authorization", "Bearer t")],
        Some(body),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(sent).unwrap(),
        "POST /a?b=c HTTP/1.1\r\nHost: example.com\r\nAccept: application/json\r\n\
         Authorization: Bearer t\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}"
    );
}

#[test]
fn refuses_line_breaks_in_the_request() {
    assert!(request("GET", "example.com", "/a\r\nX: 1", &[], None).is_err());
    assert!(request("GET", "example.com", "/a", &[("X", "1\r\nY: 2")], None).is_err());
    assert!(request("GET", "example.com", "a", &[], None).is_err());
}

#[tokio::test]
async fn keeps_the_connection_for_the_next_request() {
    let (client, mut server) = duplex(1 << 16);
    let replies = b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\naHTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nb";
    server.write_all(replies).await.unwrap();
    let conn = Conn::new(client, MAX_BODY);
    let get = request("GET", "h", "/", &[], None).unwrap();
    let Ok((first, Some(conn))) = exchange(conn, &get).await else {
        panic!("first")
    };
    let Ok((second, _)) = exchange(conn, &get).await else {
        panic!("second")
    };
    assert_eq!((first.body, second.body), (b"a".to_vec(), b"b".to_vec()));
}

#[test]
fn parses_a_response_held_in_memory() {
    let response = parse(b"HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\n\r\n{}\r\n");
    assert_eq!(
        response,
        Some(Response {
            status: 404,
            content_type: "application/json".into(),
            location: String::new(),
            body: b"{}\r\n".to_vec(),
        })
    );
    assert_eq!(parse(b"HTTP/1.1 200 OK\r\n"), None);
}

/// A connection whose reads fail as TLS fails one dropped without a goodbye.
struct Dropped;

impl AsyncRead for Dropped {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let e = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "no close_notify");
        std::task::Poll::Ready(Err(e))
    }
}

impl AsyncWrite for Dropped {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn takes_a_connection_dropped_before_answering_as_closed() {
    let answer = exchange(Conn::new(Dropped, MAX_BODY), b"GET / HTTP/1.1\r\n\r\n").await;
    assert!(matches!(answer, Err(Failure::Closed)));
}
