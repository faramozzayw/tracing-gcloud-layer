use std::path::Path;

use errors::Result;
use reqwest::Client;

use self::jwt::{JwtToken, Token};
use crate::utils::timestamp;

pub use self::errors::GAuthError;
pub use jwt::GAuthCredential;

mod errors;
mod jwt;

#[derive(Debug, Default, Clone)]
pub struct GAuth {
    scopes: String,
    gauth_key_bytes: Vec<u8>,
    user_email: Option<String>,

    access_token: Option<String>,
    expires_at: Option<u64>,

    http_client: Client,
}

impl GAuth {
    #[allow(dead_code)]
    /// Creates a new service account from a key file and scopes
    pub fn from_file(key_path: impl AsRef<Path>, scopes: &[&str]) -> Result<Self> {
        let bytes = std::fs::read(key_path.as_ref()).map_err(|err| {
            GAuthError::ReadKey(format!("{}: {}", err, key_path.as_ref().display()))
        })?;

        Ok(Self::from_bytes(&bytes, scopes))
    }

    pub fn from_bytes(bytes: &[u8], scopes: &[&str]) -> Self {
        Self {
            scopes: scopes.join(" "),
            gauth_key_bytes: bytes.to_vec(),
            ..Default::default()
        }
    }

    fn access_token_inner(&mut self, token: Token) -> Result<String> {
        match (self.access_token.as_ref(), self.expires_at) {
            (Some(access_token), Some(expires_at)) if expires_at > timestamp()? => {
                Ok(access_token.to_string())
            }
            _ => {
                let expires_at = timestamp()? + token.expires_in - 30;

                self.access_token = Some(token.bearer_token());
                self.expires_at = Some(expires_at);

                Ok(token.bearer_token())
            }
        }
    }

    /// Returns an access token
    /// If the access token is not expired, it will return the cached access token
    /// Otherwise, it will exchange the JWT token for an access token
    pub async fn access_token(&mut self) -> Result<String> {
        let jwt_token = self.jwt_token()?;
        let token = self.exchange_jwt_token_for_access_token(jwt_token).await?;

        self.access_token_inner(token)
    }

    async fn exchange_jwt_token_for_access_token(&mut self, jwt_token: JwtToken) -> Result<Token> {
        self.http_client
            .post(jwt_token.token_uri())
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &jwt_token.to_string()?),
            ])
            .send()
            .await?
            .json::<Token>()
            .await
            .map_err(Into::into)
    }

    fn jwt_token(&self) -> Result<JwtToken> {
        let token = JwtToken::from_bytes(&self.gauth_key_bytes)?;

        Ok(match self.user_email {
            Some(ref user_email) => token.sub(user_email.to_string()),
            None => token,
        }
        .scope(self.scopes.clone()))
    }
}
