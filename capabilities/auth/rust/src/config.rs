// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::AuthError;
use oauth2::url::Url;

/// Product-owned deployment configuration. Secrets are intentionally not Debug.
#[derive(Clone)]
pub struct AuthConfig {
    pub(crate) public_url: Url,
    pub(crate) web_return: Url,
    pub(crate) native_return: Url,
    pub(crate) client_id: Option<String>,
    pub(crate) client_secret: Option<String>,
    pub(crate) secure: bool,
    pub(crate) lifetime_seconds: i64,
    pub(crate) authorize_url: String,
    pub(crate) token_url: String,
    pub(crate) user_url: String,
}

impl AuthConfig {
    pub fn new(
        public_url: &str,
        web_return: &str,
        native_return: &str,
        development: bool,
    ) -> Result<Self, AuthError> {
        let public_url = Url::parse(public_url).map_err(|_| AuthError::configuration())?;
        let web_return = Url::parse(web_return).map_err(|_| AuthError::configuration())?;
        let native_return = Url::parse(native_return).map_err(|_| AuthError::configuration())?;
        for url in [&public_url, &web_return] {
            if !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || (!development && url.scheme() != "https")
                || (development && !matches!(url.scheme(), "http" | "https"))
            {
                return Err(AuthError::configuration());
            }
        }
        if public_url.path() != "/"
            || matches!(
                native_return.scheme(),
                "http" | "https" | "javascript" | "data" | "file"
            )
            || native_return.query().is_some()
            || native_return.fragment().is_some()
            || native_return.host_str() != Some("auth")
            || native_return.path() != "/callback"
        {
            return Err(AuthError::configuration());
        }
        Ok(Self {
            public_url,
            web_return,
            native_return,
            client_id: None,
            client_secret: None,
            secure: !development,
            lifetime_seconds: 7 * 24 * 60 * 60,
            authorize_url: "https://github.com/login/oauth/authorize".into(),
            token_url: "https://github.com/login/oauth/access_token".into(),
            user_url: "https://api.github.com/user".into(),
        })
    }

    pub fn github(mut self, client_id: String, client_secret: String) -> Result<Self, AuthError> {
        if client_id.trim().is_empty() || client_secret.trim().is_empty() {
            return Err(AuthError::configuration());
        }
        self.client_id = Some(client_id);
        self.client_secret = Some(client_secret);
        Ok(self)
    }

    pub fn lifetime_seconds(mut self, seconds: i64) -> Result<Self, AuthError> {
        if !(60..=365 * 24 * 60 * 60).contains(&seconds) {
            return Err(AuthError::configuration());
        }
        self.lifetime_seconds = seconds;
        Ok(self)
    }

    /// Controlled integration fixtures only; production builds expose no endpoint override.
    #[cfg(any(test, feature = "test-provider"))]
    pub fn test_provider(mut self, base_url: &str) -> Result<Self, AuthError> {
        let url = Url::parse(base_url).map_err(|_| AuthError::configuration())?;
        if url.scheme() != "http"
            || !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
        {
            return Err(AuthError::configuration());
        }
        self.authorize_url = url
            .join("authorize")
            .map_err(|_| AuthError::configuration())?
            .to_string();
        self.token_url = url
            .join("token")
            .map_err(|_| AuthError::configuration())?
            .to_string();
        self.user_url = url
            .join("user")
            .map_err(|_| AuthError::configuration())?
            .to_string();
        Ok(self)
    }

    pub(crate) fn available(&self) -> bool {
        self.client_id.is_some() && self.client_secret.is_some()
    }
    pub(crate) fn callback(&self) -> String {
        self.public_url
            .join("auth/github/callback")
            .expect("fixed path")
            .to_string()
    }
    pub(crate) fn cookie_name(&self) -> &'static str {
        if self.secure {
            "__Host-yydra-session"
        } else {
            "yydra-session"
        }
    }
}
