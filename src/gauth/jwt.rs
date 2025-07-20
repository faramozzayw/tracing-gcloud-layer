use std::path::Path;

use base64::{Engine as _, engine::general_purpose};
use ring::{rand, signature};
use serde_derive::Deserialize;
use serde_derive::Serialize;

use super::errors::{GAuthError, Result};
use crate::utils::timestamp;

#[derive(Debug, serde_derive::Deserialize)]
pub struct Token {
    pub access_token: String,
    pub expires_in: u64,
    pub token_type: String,
}

impl Token {
    pub fn bearer_token(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct JwtToken {
    private_key: String,
    header: JwtHeader,
    payload: JwtPayload,
}

#[derive(Clone, Debug, Default, Serialize)]
struct JwtHeader {
    alg: String,
    typ: String,
}

#[derive(Clone, Debug, Default, Serialize)]
struct JwtPayload {
    iss: String,
    sub: Option<String>,
    scope: String,
    aud: String,
    exp: u64,
    iat: u64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
pub struct GAuthCredential {
    pub r#type: String,
    pub project_id: String,
    pub private_key_id: String,
    pub private_key: String,
    pub client_email: String,
    pub client_id: String,
    pub auth_uri: String,
    pub token_uri: String,
    pub auth_provider_x509_cert_url: String,
    pub client_x509_cert_url: String,
    pub universe_domain: String,
}

impl GAuthCredential {
    pub fn from_bytes(bytes: &[u8]) -> serde_json::Result<Self> {
        serde_json::from_slice(bytes)
    }
}

impl JwtToken {
    pub fn new(gauth_credential: GAuthCredential) -> Result<Self> {
        let iat = timestamp()?;
        let exp = iat + 3600;

        let private_key = gauth_credential
            .private_key
            .replace('\n', "")
            .replace("-----BEGIN PRIVATE KEY-----", "")
            .replace("-----END PRIVATE KEY-----", "");

        Ok(Self {
            header: JwtHeader {
                alg: String::from("RS256"),
                typ: String::from("JWT"),
            },
            payload: JwtPayload {
                iss: gauth_credential.client_email,
                sub: None,
                scope: String::new(),
                aud: gauth_credential.token_uri,
                exp,
                iat,
            },
            private_key,
        })
    }

    /// Creates a new JWT token from a service account key file
    #[allow(dead_code)]
    pub fn from_file(key_path: impl AsRef<Path>) -> Result<Self> {
        let gauth_credential_bytes = std::fs::read(key_path.as_ref()).map_err(|err| {
            GAuthError::ReadKey(format!("{}: {}", err, key_path.as_ref().display()))
        })?;

        Self::from_bytes(&gauth_credential_bytes)
    }

    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Self::new(serde_json::from_slice::<GAuthCredential>(bytes)?)
    }

    /// Returns a JWT token string
    pub fn to_string(&self) -> Result<String> {
        let header = serde_json::to_vec(&self.header)?;
        let payload = serde_json::to_vec(&self.payload)?;

        let base64_header = general_purpose::STANDARD.encode(header);
        let base64_payload = general_purpose::STANDARD.encode(payload);

        let raw_signature = format!("{}.{}", base64_header, base64_payload);
        let signature = self.sign_rsa(raw_signature)?;

        let base64_signature = general_purpose::STANDARD.encode(signature);

        Ok(format!(
            "{}.{}.{}",
            base64_header, base64_payload, base64_signature
        ))
    }

    /// Returns the token uri
    pub fn token_uri(&self) -> &str {
        &self.payload.aud
    }

    /// Sets the sub field in the payload
    pub fn sub(mut self, sub: String) -> Self {
        self.payload.sub = Some(sub);
        self
    }

    /// Sets the scope field in the payload
    pub fn scope(mut self, scope: String) -> Self {
        self.payload.scope = scope;
        self
    }

    /// Signs a message with the private key
    fn sign_rsa(&self, message: String) -> Result<Vec<u8>> {
        let private_key = self.private_key.as_bytes();
        let decoded = general_purpose::STANDARD.decode(private_key)?;

        let key_pair = signature::RsaKeyPair::from_pkcs8(&decoded)
            .map_err(|err| GAuthError::RsaKeyPair(format!("failed tp create key pair: {}", err)))?;

        // Sign the message, using PKCS#1 v1.5 padding and the SHA256 digest algorithm.
        let rng = rand::SystemRandom::new();
        let mut signature = vec![0; key_pair.public().modulus_len()];
        key_pair
            .sign(
                &signature::RSA_PKCS1_SHA256,
                &rng,
                message.as_bytes(),
                &mut signature,
            )
            .map_err(|err| GAuthError::RsaSign(format!("{}", err)))?;

        Ok(signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICE_ACCOUNT_KEY_PATH: &str = "test_fixtures/service-account-key.json";

    #[test]
    fn test_jwt_token() {
        let mut token = JwtToken::from_file(SERVICE_ACCOUNT_KEY_PATH).unwrap();

        assert_eq!(token.header.alg, "RS256");
        assert_eq!(token.header.typ, "JWT");
        assert!(token.payload.iss.contains("iam.gserviceaccount.com"));
        assert_eq!(token.payload.sub, None);
        assert_eq!(token.payload.scope, "");
        assert_eq!(token.payload.aud, "https://oauth2.googleapis.com/token");
        assert!(token.payload.exp > 0);
        assert_eq!(token.payload.iat, token.payload.exp - 3600);

        token = token
            .sub(String::from("some@email.domain"))
            .scope(String::from("test_scope1 test_scope2 test_scope3"));

        assert_eq!(token.payload.sub, Some(String::from("some@email.domain")));
        assert_eq!(token.payload.scope, "test_scope1 test_scope2 test_scope3");
    }

    #[test]
    fn test_sign_rsa() {
        let message = String::from("hello, world");

        let token = JwtToken::from_file(SERVICE_ACCOUNT_KEY_PATH).unwrap();
        let signature = token.sign_rsa(message).unwrap();

        assert_eq!(signature.len(), 256);
    }

    #[test]
    fn test_token_to_string() {
        let token = JwtToken::from_file(SERVICE_ACCOUNT_KEY_PATH)
            .unwrap()
            .sub(String::from("some@email.com"))
            .scope(String::from("https://www.googleapis.com/auth/pubsub"));

        let token_string = token.to_string();

        assert!(token_string.is_ok(), "token string successfully created");
        assert!(
            !token_string.unwrap().is_empty(),
            "token string is not empty"
        );
    }
}
