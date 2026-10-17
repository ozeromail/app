//! Google sign-in for an installed app: the authorization code flow with PKCE (RFC 7636)
//! through the system browser. Android clients have no secret.

use std::sync::Arc;

use ozero_domain::ProviderError;
use ozero_http::{Body, HttpsClient};
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::Value;
use tokio_rustls::rustls::ClientConfig;

use crate::codec::{base64url, form, query_value};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_HOST: &str = "oauth2.googleapis.com";
/// Read, label and send mail; not delete for good.
pub const SCOPE: &str = "https://www.googleapis.com/auth/gmail.modify";

/// A sign-in under way: the browser opens `url`, and the app keeps the rest until the
/// browser comes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignIn {
    pub url: String,
    /// The PKCE secret; only its hash is in `url`.
    pub verifier: String,
    /// Ties the answer to this sign-in.
    pub state: String,
}

/// Starts a sign-in for the app's OAuth client.
pub fn sign_in(client_id: &str, redirect: &str) -> Result<SignIn, ProviderError> {
    let verifier = base64url(&random::<32>()?);
    let state = base64url(&random::<16>()?);
    let challenge = base64url(digest(&SHA256, verifier.as_bytes()).as_ref());
    let query = form(&[
        ("client_id", client_id),
        ("redirect_uri", redirect),
        ("response_type", "code"),
        ("scope", SCOPE),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("state", &state),
    ]);
    Ok(SignIn {
        url: format!("{AUTH_URL}?{query}"),
        verifier,
        state,
    })
}

/// The code in the address the browser came back to. `Rejected` if the person said no.
pub fn code(redirect: &str, sign_in: &SignIn) -> Result<String, ProviderError> {
    if query_value(redirect, "state").as_deref() != Some(sign_in.state.as_str()) {
        return Err(ProviderError::Protocol(
            "the sign-in answer is not ours".into(),
        ));
    }
    if let Some(error) = query_value(redirect, "error") {
        return Err(ProviderError::Rejected(error));
    }
    query_value(redirect, "code")
        .filter(|c| !c.is_empty())
        .ok_or_else(|| ProviderError::Protocol("no code in the sign-in answer".into()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokens {
    pub access: String,
    /// Seconds the access token lives.
    pub expires_in: u64,
    /// Only after sign-in; it does not change on refresh.
    pub refresh: Option<String>,
}

/// Trades the code for tokens. `Auth` if Gmail access was not granted.
pub async fn exchange(
    tls: Arc<ClientConfig>,
    client_id: &str,
    redirect: &str,
    code: &str,
    sign_in: &SignIn,
) -> Result<Tokens, ProviderError> {
    let tokens = token(
        tls,
        &[
            ("client_id", client_id),
            ("redirect_uri", redirect),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", &sign_in.verifier),
        ],
    )
    .await?;
    if tokens.refresh.is_none() {
        return Err(ProviderError::Protocol("no refresh token".into()));
    }
    Ok(tokens)
}

/// A new access token. `Auth` once the person revoked access or the token expired.
pub async fn refresh(
    tls: Arc<ClientConfig>,
    client_id: &str,
    refresh: &str,
) -> Result<Tokens, ProviderError> {
    token(
        tls,
        &[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
        ],
    )
    .await
}

async fn token(tls: Arc<ClientConfig>, pairs: &[(&str, &str)]) -> Result<Tokens, ProviderError> {
    let mut http = HttpsClient::new(TOKEN_HOST, 443, tls);
    let body = form(pairs);
    let response = http
        .send(
            "POST",
            "/token",
            &[],
            Some(Body {
                content_type: "application/x-www-form-urlencoded",
                data: body.as_bytes(),
            }),
        )
        .await?;
    let json: Value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
    if !response.ok() {
        let error = json["error"].as_str().unwrap_or_default();
        return Err(match (response.status, error) {
            (400, "invalid_grant") | (401, _) => ProviderError::Auth,
            (429 | 500.., _) => ProviderError::Network(format!("token: {}", response.status)),
            (status, error) => ProviderError::Rejected(format!("token: {status} {error}")),
        });
    }
    tokens(&json)
}

fn tokens(json: &Value) -> Result<Tokens, ProviderError> {
    // The person can untick Gmail on the consent screen and still sign in.
    if let Some(scope) = json["scope"].as_str()
        && !scope.split(' ').any(|s| s == SCOPE)
    {
        return Err(ProviderError::Auth);
    }
    let access = json["access_token"]
        .as_str()
        .ok_or_else(|| ProviderError::Protocol("no access token".into()))?;
    Ok(Tokens {
        access: access.to_owned(),
        expires_in: json["expires_in"].as_u64().unwrap_or(0),
        refresh: json["refresh_token"].as_str().map(str::to_owned),
    })
}

fn random<const N: usize>() -> Result<[u8; N], ProviderError> {
    let mut bytes = [0; N];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| ProviderError::Protocol("no random numbers".into()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn signs_in_with_pkce() {
        let start = sign_in("id.apps", "app:/r").unwrap();
        assert_eq!(start.verifier.len(), 43);
        let challenge = base64url(digest(&SHA256, start.verifier.as_bytes()).as_ref());
        assert_eq!(query_value(&start.url, "code_challenge"), Some(challenge));
        assert_eq!(
            query_value(&start.url, "redirect_uri").as_deref(),
            Some("app:/r")
        );
        assert_eq!(query_value(&start.url, "scope").as_deref(), Some(SCOPE));
        assert_ne!(sign_in("id.apps", "app:/r").unwrap().state, start.state);
    }

    #[test]
    fn reads_the_answer() {
        let start = sign_in("id", "app:/r").unwrap();
        let state = &start.state;
        assert_eq!(
            code(&format!("app:/r?state={state}&code=4%2Fx"), &start),
            Ok("4/x".into())
        );
        assert_eq!(
            code(&format!("app:/r?error=access_denied&state={state}"), &start),
            Err(ProviderError::Rejected("access_denied".into()))
        );
        assert!(matches!(
            code("app:/r?state=other&code=x", &start),
            Err(ProviderError::Protocol(_))
        ));
        assert!(matches!(
            code(&format!("app:/r?state={state}"), &start),
            Err(ProviderError::Protocol(_))
        ));
    }

    #[test]
    fn needs_gmail_in_the_granted_scope() {
        let granted =
            json!({"access_token": "a", "expires_in": 3599, "refresh_token": "r", "scope": SCOPE});
        assert_eq!(
            tokens(&granted),
            Ok(Tokens {
                access: "a".into(),
                expires_in: 3599,
                refresh: Some("r".into())
            })
        );
        let without = json!({"access_token": "a", "scope": "openid"});
        assert_eq!(tokens(&without), Err(ProviderError::Auth));
    }
}
