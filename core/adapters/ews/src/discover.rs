//! Finds a mailbox's EWS server from its address: Autodiscover (POX), as Outlook does, then
//! the usual host names.

use std::sync::Arc;

use ozero_domain::{ProviderError, email_domain};
use ozero_http::split;
use tokio_rustls::rustls::ClientConfig;

use crate::client::Client;
use crate::xml::{self, escape};
use crate::{EwsSession, PATH};

const AUTODISCOVER: &str = "/autodiscover/autodiscover.xml";
/// Hosts that often serve EWS, tried after Autodiscover, each before the domain.
const GUESSES: [&str; 3] = ["mail", "owa", "exchange"];

/// Where EWS answers: a host and port, with the standard path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

/// The EWS endpoint of `email` that takes `login` and `password`, signed in. `Auth` if a
/// server for it refused the login, `Gone` if none was found.
pub async fn discover(
    tls: Arc<ClientConfig>,
    email: &str,
    login: &str,
    password: &str,
) -> Result<(Endpoint, EwsSession), ProviderError> {
    let domain = email_domain(email).ok_or(ProviderError::Gone)?;
    // A refused login ends the search: every other try would count towards locking the account.
    let mut candidates = Vec::new();
    for host in [format!("autodiscover.{domain}"), domain.clone()] {
        let found = autodiscover(tls.clone(), &host, email, login, password).await;
        match found.map(|url| url.as_deref().and_then(endpoint)) {
            Ok(Some(endpoint)) => candidates.push(endpoint),
            Err(ProviderError::Auth) => return Err(ProviderError::Auth),
            _ => {}
        }
    }
    candidates.extend(GUESSES.iter().map(|guess| Endpoint {
        host: format!("{guess}.{domain}"),
        port: 443,
    }));
    for endpoint in candidates {
        match check(tls.clone(), &endpoint, login, password).await {
            Ok(session) => return Ok((endpoint, session)),
            Err(ProviderError::Auth) => return Err(ProviderError::Auth),
            Err(_) => {}
        }
    }
    Err(ProviderError::Gone)
}

/// The endpoint a server address the person typed names: a host, a host and port, or a
/// `https` URL.
pub fn endpoint(text: &str) -> Option<Endpoint> {
    let text = text.trim();
    let url = if text.contains("://") {
        text.to_owned()
    } else {
        format!("https://{text}")
    };
    let (host, port, _) = split(&url).ok()?;
    Some(Endpoint { host, port })
}

/// Logs in to EWS at `endpoint`, and returns the session signed in.
pub async fn check(
    tls: Arc<ClientConfig>,
    endpoint: &Endpoint,
    login: &str,
    password: &str,
) -> Result<EwsSession, ProviderError> {
    let client = Client::new(tls, &endpoint.host, endpoint.port, PATH, login, password).probing();
    let mut session = EwsSession::over(client);
    session.check().await?;
    Ok(session)
}

/// The EWS URL Autodiscover at `host` gives for `email`, following one redirect to another
/// address. None if it gave none.
async fn autodiscover(
    tls: Arc<ClientConfig>,
    host: &str,
    email: &str,
    login: &str,
    password: &str,
) -> Result<Option<String>, ProviderError> {
    let mut email = email.to_owned();
    let mut client = Client::new(tls, host, 443, AUTODISCOVER, login, password).probing();
    for _ in 0..2 {
        let request = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<Autodiscover xmlns=\"http://schemas.microsoft.com/exchange/autodiscover/outlook/requestschema/2006\">\
<Request><EMailAddress>{}</EMailAddress>\
<AcceptableResponseSchema>http://schemas.microsoft.com/exchange/autodiscover/outlook/responseschema/2006a</AcceptableResponseSchema>\
</Request></Autodiscover>",
            escape(&email)
        );
        let response = client
            .post("text/xml; charset=utf-8", request.as_bytes(), true)
            .await?;
        if !response.ok() {
            return Ok(None);
        }
        match answer(&response.body) {
            Found::Url(url) => return Ok(Some(url)),
            Found::Redirect(to) if to != email => email = to,
            _ => return Ok(None),
        }
    }
    Ok(None)
}

#[derive(Debug, PartialEq, Eq)]
enum Found {
    Url(String),
    /// Ask again for this address.
    Redirect(String),
    Nothing,
}

/// What an Autodiscover answer says: the external EWS URL first, then the internal one.
fn answer(body: &[u8]) -> Found {
    let Some(account) = xml::parse(body).and_then(|r| r.at(&["Response", "Account"]).cloned())
    else {
        return Found::Nothing;
    };
    if account.text_at(&["Action"]) == "redirectAddr" {
        let to = account.text_at(&["RedirectAddr"]).trim();
        return if to.contains('@') {
            Found::Redirect(to.to_owned())
        } else {
            Found::Nothing
        };
    }
    for kind in ["EXPR", "EXCH"] {
        let url = account
            .all("Protocol")
            .filter(|p| p.text_at(&["Type"]) == kind)
            .map(|p| p.text_at(&["EwsUrl"]).trim())
            .find(|u| {
                u.get(..8)
                    .is_some_and(|s| s.eq_ignore_ascii_case("https://"))
            });
        if let Some(url) = url {
            return Found::Url(url.to_owned());
        }
    }
    Found::Nothing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_person_typed() {
        let e = |host: &str, port| {
            Some(Endpoint {
                host: host.into(),
                port,
            })
        };
        assert_eq!(endpoint("Mail.Corp.ru"), e("mail.corp.ru", 443));
        assert_eq!(
            endpoint(" https://mail.corp.ru/owa/ "),
            e("mail.corp.ru", 443)
        );
        assert_eq!(
            endpoint("mail.corp.ru:8443/EWS/Exchange.asmx"),
            e("mail.corp.ru", 8443)
        );
        assert_eq!(endpoint("http://mail.corp.ru"), None);
        assert_eq!(endpoint("user@mail.corp.ru"), None);
        assert_eq!(endpoint("mail.corp.ru/a b"), None);
        assert_eq!(endpoint(""), None);
    }

    #[test]
    fn reads_autodiscover() {
        let body = br#"<?xml version="1.0" encoding="utf-8"?>
<Autodiscover xmlns="http://schemas.microsoft.com/exchange/autodiscover/responseschema/2006">
<Response xmlns="http://schemas.microsoft.com/exchange/autodiscover/outlook/responseschema/2006a">
<Account><AccountType>email</AccountType><Action>settings</Action>
<Protocol><Type>EXCH</Type><EwsUrl>https://ex01.corp.local/EWS/Exchange.asmx</EwsUrl></Protocol>
<Protocol><Type>EXPR</Type><EwsUrl>https://mail.corp.ru/EWS/Exchange.asmx</EwsUrl></Protocol>
<Protocol><Type>WEB</Type></Protocol></Account></Response></Autodiscover>"#;
        assert_eq!(
            answer(body),
            Found::Url("https://mail.corp.ru/EWS/Exchange.asmx".into())
        );
        let inside = br#"<Autodiscover><Response><Account><Protocol><Type>EXCH</Type>
<EwsUrl>https://ex01.corp.local/EWS/Exchange.asmx</EwsUrl></Protocol></Account></Response></Autodiscover>"#;
        assert_eq!(
            answer(inside),
            Found::Url("https://ex01.corp.local/EWS/Exchange.asmx".into())
        );
        let redirect = br#"<Autodiscover><Response><Account><Action>redirectAddr</Action>
<RedirectAddr>anna@corp.ru</RedirectAddr></Account></Response></Autodiscover>"#;
        assert_eq!(answer(redirect), Found::Redirect("anna@corp.ru".into()));
        assert_eq!(answer(b"<html>"), Found::Nothing);
        assert_eq!(
            answer(br#"<Autodiscover><Response><Error><ErrorCode>600</ErrorCode></Error></Response></Autodiscover>"#),
            Found::Nothing
        );
    }
}
