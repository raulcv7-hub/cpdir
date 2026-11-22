//! Domain models representing heuristic secret detection findings.

/// Category of potential secret matched during local scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretRuleKind {
    /// PEM private key block header.
    PrivateKey,
    /// AWS IAM access key identifier.
    AwsAccessKey,
    /// GitHub personal or fine-grained access token.
    GitHubToken,
    /// Google API key (`AIza...`).
    GoogleApiKey,
    /// Slack bot, user, or app token.
    SlackToken,
    /// Stripe live secret or restricted key.
    StripeLiveKey,
    /// `OpenAI` or Anthropic provider API key.
    AiProviderKey,
    /// Connection string or URL carrying embedded password credentials.
    CredentialUrl,
    /// JSON web token (`eyJ...`).
    JwtToken,
    /// Quoted or `ENV=` hardcoded secret/password assignment.
    HardcodedSecret,
}

/// Single secret alert recorded without capturing the raw credential value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretFinding {
    /// One-based line number where the match occurred.
    pub line: usize,
    /// Classification of the matched credential pattern.
    pub rule: SecretRuleKind,
}
