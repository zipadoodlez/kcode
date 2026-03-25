#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginProviderAuthKind {
    OAuth,
    ApiKey,
    DeviceCode,
    Cli,
    Hybrid,
}

impl LoginProviderAuthKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::OAuth => "OAuth",
            Self::ApiKey => "API key",
            Self::DeviceCode => "device code",
            Self::Cli => "CLI",
            Self::Hybrid => "API key / CLI",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginProviderTarget {
    Jcode,
    Claude,
    OpenAi,
    OpenRouter,
    Azure,
    OpenAiCompatible(OpenAiCompatibleProfile),
    Cursor,
    Copilot,
    Gemini,
    Antigravity,
    Google,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginProviderAuthStateKey {
    Jcode,
    Anthropic,
    OpenAi,
    Azure,
    OpenRouterLike,
    Copilot,
    Gemini,
    Antigravity,
    Cursor,
    Google,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginProviderSurface {
    CliLogin,
    TuiLogin,
    ServerBootstrap,
    AutoInit,
    AuthStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoginProviderSurfaceOrder {
    pub cli_login: Option<u8>,
    pub tui_login: Option<u8>,
    pub server_bootstrap: Option<u8>,
    pub auto_init: Option<u8>,
    pub auth_status: Option<u8>,
}

impl LoginProviderSurfaceOrder {
    pub const fn new(
        cli_login: Option<u8>,
        tui_login: Option<u8>,
        server_bootstrap: Option<u8>,
        auto_init: Option<u8>,
        auth_status: Option<u8>,
    ) -> Self {
        Self {
            cli_login,
            tui_login,
            server_bootstrap,
            auto_init,
            auth_status,
        }
    }

    pub const fn for_surface(self, surface: LoginProviderSurface) -> Option<u8> {
        match surface {
            LoginProviderSurface::CliLogin => self.cli_login,
            LoginProviderSurface::TuiLogin => self.tui_login,
            LoginProviderSurface::ServerBootstrap => self.server_bootstrap,
            LoginProviderSurface::AutoInit => self.auto_init,
            LoginProviderSurface::AuthStatus => self.auth_status,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoginProviderDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub auth_kind: LoginProviderAuthKind,
    pub auth_state_key: LoginProviderAuthStateKey,
    pub auth_status_method: &'static str,
    pub aliases: &'static [&'static str],
    pub menu_detail: &'static str,
    pub recommended: bool,
    pub target: LoginProviderTarget,
    pub order: LoginProviderSurfaceOrder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenAiCompatibleProfile {
    pub id: &'static str,
    pub display_name: &'static str,
    pub api_base: &'static str,
    pub api_key_env: &'static str,
    pub env_file: &'static str,
    pub setup_url: &'static str,
    pub default_model: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedOpenAiCompatibleProfile {
    pub id: String,
    pub display_name: String,
    pub api_base: String,
    pub api_key_env: String,
    pub env_file: String,
    pub setup_url: String,
    pub default_model: Option<String>,
}

pub const OPENCODE_PROFILE: OpenAiCompatibleProfile = OpenAiCompatibleProfile {
    id: "opencode",
    display_name: "OpenCode Zen",
    api_base: "https://opencode.ai/zen/v1",
    api_key_env: "OPENCODE_API_KEY",
    env_file: "opencode.env",
    setup_url: "https://opencode.ai/docs/providers#opencode-zen",
    default_model: Some("qwen/qwen3-coder-plus"),
};

pub const OPENCODE_GO_PROFILE: OpenAiCompatibleProfile = OpenAiCompatibleProfile {
    id: "opencode-go",
    display_name: "OpenCode Go",
    api_base: "https://opencode.ai/zen/go/v1",
    api_key_env: "OPENCODE_GO_API_KEY",
    env_file: "opencode-go.env",
    setup_url: "https://opencode.ai/docs/providers#opencode-go",
    default_model: Some("THUDM/GLM-4.5"),
};

pub const ZAI_PROFILE: OpenAiCompatibleProfile = OpenAiCompatibleProfile {
    id: "zai",
    display_name: "Z.AI Coding",
    api_base: "https://api.z.ai/api/coding/paas/v4",
    api_key_env: "ZAI_API_KEY",
    env_file: "zai.env",
    setup_url: "https://docs.z.ai/guides/develop/openai/introduction",
    default_model: Some("glm-4.5"),
};

pub const CHUTES_PROFILE: OpenAiCompatibleProfile = OpenAiCompatibleProfile {
    id: "chutes",
    display_name: "Chutes",
    api_base: "https://llm.chutes.ai/v1",
    api_key_env: "CHUTES_API_KEY",
    env_file: "chutes.env",
    setup_url: "https://chutes.ai",
    default_model: Some("Qwen/Qwen3-Coder-480B-A35B-Instruct"),
};

pub const CEREBRAS_PROFILE: OpenAiCompatibleProfile = OpenAiCompatibleProfile {
    id: "cerebras",
    display_name: "Cerebras",
    api_base: "https://api.cerebras.ai/v1",
    api_key_env: "CEREBRAS_API_KEY",
    env_file: "cerebras.env",
    setup_url: "https://inference-docs.cerebras.ai/introduction",
    default_model: Some("qwen-3-coder-480b"),
};

pub const ALIBABA_CODING_PLAN_PROFILE: OpenAiCompatibleProfile = OpenAiCompatibleProfile {
    id: "alibaba-coding-plan",
    display_name: "Alibaba Cloud Coding Plan",
    api_base: "https://coding.dashscope.aliyuncs.com/v1",
    api_key_env: "BAILIAN_CODING_PLAN_API_KEY",
    env_file: "alibaba-coding-plan.env",
    setup_url: "https://www.alibabacloud.com/help/en/model-studio/coding-plan-quickstart",
    default_model: Some("qwen3-coder-plus"),
};

pub const OPENAI_COMPAT_PROFILE: OpenAiCompatibleProfile = OpenAiCompatibleProfile {
    id: "openai-compatible",
    display_name: "OpenAI-compatible",
    api_base: "https://api.openai.com/v1",
    api_key_env: "OPENAI_COMPAT_API_KEY",
    env_file: "openai-compatible.env",
    setup_url: "https://opencode.ai/docs/providers#custom-providers",
    default_model: None,
};

const OPENAI_COMPAT_PROFILES: [OpenAiCompatibleProfile; 7] = [
    OPENCODE_PROFILE,
    OPENCODE_GO_PROFILE,
    ZAI_PROFILE,
    CHUTES_PROFILE,
    CEREBRAS_PROFILE,
    ALIBABA_CODING_PLAN_PROFILE,
    OPENAI_COMPAT_PROFILE,
];

pub const CLAUDE_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "claude",
    display_name: "Anthropic/Claude",
    auth_kind: LoginProviderAuthKind::OAuth,
    auth_state_key: LoginProviderAuthStateKey::Anthropic,
    auth_status_method: "OAuth / API key",
    aliases: &["anthropic"],
    menu_detail: "requires Claude Pro or Max subscription",
    recommended: true,
    target: LoginProviderTarget::Claude,
    order: LoginProviderSurfaceOrder::new(Some(1), Some(1), Some(1), Some(1), Some(1)),
};

pub const JCODE_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "jcode",
    display_name: "Jcode Subscription",
    auth_kind: LoginProviderAuthKind::ApiKey,
    auth_state_key: LoginProviderAuthStateKey::Jcode,
    auth_status_method: "API key",
    aliases: &["subscription", "jcode-subscription"],
    menu_detail: "curated jcode subscription models",
    recommended: false,
    target: LoginProviderTarget::Jcode,
    order: LoginProviderSurfaceOrder::new(Some(3), Some(3), Some(3), Some(3), Some(3)),
};

pub const OPENAI_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "openai",
    display_name: "OpenAI",
    auth_kind: LoginProviderAuthKind::OAuth,
    auth_state_key: LoginProviderAuthStateKey::OpenAi,
    auth_status_method: "OAuth / API key",
    aliases: &[],
    menu_detail: "requires ChatGPT Plus or Pro subscription",
    recommended: true,
    target: LoginProviderTarget::OpenAi,
    order: LoginProviderSurfaceOrder::new(Some(2), Some(2), Some(2), Some(2), Some(2)),
};

pub const OPENROUTER_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "openrouter",
    display_name: "OpenRouter",
    auth_kind: LoginProviderAuthKind::ApiKey,
    auth_state_key: LoginProviderAuthStateKey::OpenRouterLike,
    auth_status_method: "API key",
    aliases: &[],
    menu_detail: "API key, pay-per-token, 200+ models",
    recommended: false,
    target: LoginProviderTarget::OpenRouter,
    order: LoginProviderSurfaceOrder::new(Some(4), Some(3), Some(4), Some(3), Some(3)),
};

pub const AZURE_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "azure",
    display_name: "Azure OpenAI",
    auth_kind: LoginProviderAuthKind::Hybrid,
    auth_state_key: LoginProviderAuthStateKey::Azure,
    auth_status_method: "Entra ID / API key",
    aliases: &["azure-openai", "azure_openai", "aoai"],
    menu_detail: "Microsoft Entra ID or Azure OpenAI API key",
    recommended: false,
    target: LoginProviderTarget::Azure,
    order: LoginProviderSurfaceOrder::new(Some(5), None, None, None, Some(4)),
};

pub const OPENCODE_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "opencode",
    display_name: "OpenCode Zen",
    auth_kind: LoginProviderAuthKind::ApiKey,
    auth_state_key: LoginProviderAuthStateKey::OpenRouterLike,
    auth_status_method: "API key",
    aliases: &["opencode-zen", "zen"],
    menu_detail: "API key",
    recommended: false,
    target: LoginProviderTarget::OpenAiCompatible(OPENCODE_PROFILE),
    order: LoginProviderSurfaceOrder::new(Some(5), Some(4), Some(5), Some(4), Some(4)),
};

pub const OPENCODE_GO_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "opencode-go",
    display_name: "OpenCode Go",
    auth_kind: LoginProviderAuthKind::ApiKey,
    auth_state_key: LoginProviderAuthStateKey::OpenRouterLike,
    auth_status_method: "API key",
    aliases: &["opencodego"],
    menu_detail: "API key",
    recommended: false,
    target: LoginProviderTarget::OpenAiCompatible(OPENCODE_GO_PROFILE),
    order: LoginProviderSurfaceOrder::new(Some(6), Some(5), Some(6), Some(5), Some(5)),
};

pub const ZAI_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "zai",
    display_name: "Z.AI Coding",
    auth_kind: LoginProviderAuthKind::ApiKey,
    auth_state_key: LoginProviderAuthStateKey::OpenRouterLike,
    auth_status_method: "API key",
    aliases: &["z.ai", "z-ai", "zai-coding"],
    menu_detail: "API key",
    recommended: false,
    target: LoginProviderTarget::OpenAiCompatible(ZAI_PROFILE),
    order: LoginProviderSurfaceOrder::new(Some(7), Some(6), Some(7), Some(6), Some(6)),
};

pub const CHUTES_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "chutes",
    display_name: "Chutes",
    auth_kind: LoginProviderAuthKind::ApiKey,
    auth_state_key: LoginProviderAuthStateKey::OpenRouterLike,
    auth_status_method: "API key",
    aliases: &[],
    menu_detail: "API key",
    recommended: false,
    target: LoginProviderTarget::OpenAiCompatible(CHUTES_PROFILE),
    order: LoginProviderSurfaceOrder::new(Some(8), Some(7), Some(8), Some(7), Some(7)),
};

pub const CEREBRAS_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "cerebras",
    display_name: "Cerebras",
    auth_kind: LoginProviderAuthKind::ApiKey,
    auth_state_key: LoginProviderAuthStateKey::OpenRouterLike,
    auth_status_method: "API key",
    aliases: &["cerebrascode", "cerberascode"],
    menu_detail: "API key",
    recommended: false,
    target: LoginProviderTarget::OpenAiCompatible(CEREBRAS_PROFILE),
    order: LoginProviderSurfaceOrder::new(Some(9), Some(8), Some(9), Some(8), Some(8)),
};

pub const ALIBABA_CODING_PLAN_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "alibaba-coding-plan",
    display_name: "Alibaba Cloud Coding Plan",
    auth_kind: LoginProviderAuthKind::ApiKey,
    auth_state_key: LoginProviderAuthStateKey::OpenRouterLike,
    auth_status_method: "API key",
    aliases: &["bailian", "aliyun-bailian", "coding-plan", "alibaba-coding"],
    menu_detail: "API key, dedicated Alibaba coding endpoint",
    recommended: false,
    target: LoginProviderTarget::OpenAiCompatible(ALIBABA_CODING_PLAN_PROFILE),
    order: LoginProviderSurfaceOrder::new(Some(10), Some(9), Some(10), Some(9), Some(9)),
};

pub const OPENAI_COMPAT_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "openai-compatible",
    display_name: "OpenAI-compatible",
    auth_kind: LoginProviderAuthKind::ApiKey,
    auth_state_key: LoginProviderAuthStateKey::OpenRouterLike,
    auth_status_method: "API key",
    aliases: &["openai_compatible", "compat", "custom"],
    menu_detail: "custom OpenAI-compatible endpoint",
    recommended: false,
    target: LoginProviderTarget::OpenAiCompatible(OPENAI_COMPAT_PROFILE),
    order: LoginProviderSurfaceOrder::new(Some(10), Some(9), None, None, Some(9)),
};

pub const CURSOR_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "cursor",
    display_name: "Cursor",
    auth_kind: LoginProviderAuthKind::Hybrid,
    auth_state_key: LoginProviderAuthStateKey::Cursor,
    auth_status_method: "API key / CLI",
    aliases: &[],
    menu_detail: "browser login or API key",
    recommended: false,
    target: LoginProviderTarget::Cursor,
    order: LoginProviderSurfaceOrder::new(Some(11), Some(12), None, Some(9), Some(12)),
};

pub const COPILOT_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "copilot",
    display_name: "GitHub Copilot",
    auth_kind: LoginProviderAuthKind::DeviceCode,
    auth_state_key: LoginProviderAuthStateKey::Copilot,
    auth_status_method: "device code",
    aliases: &[],
    menu_detail: "GitHub device flow",
    recommended: false,
    target: LoginProviderTarget::Copilot,
    order: LoginProviderSurfaceOrder::new(Some(3), Some(10), Some(3), Some(10), Some(10)),
};

pub const GEMINI_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "gemini",
    display_name: "Google Gemini",
    auth_kind: LoginProviderAuthKind::OAuth,
    auth_state_key: LoginProviderAuthStateKey::Gemini,
    auth_status_method: "OAuth",
    aliases: &[],
    menu_detail: "Google Gemini Code Assist OAuth login",
    recommended: false,
    target: LoginProviderTarget::Gemini,
    order: LoginProviderSurfaceOrder::new(Some(13), Some(11), Some(4), Some(11), Some(13)),
};

pub const ANTIGRAVITY_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "antigravity",
    display_name: "Antigravity",
    auth_kind: LoginProviderAuthKind::OAuth,
    auth_state_key: LoginProviderAuthStateKey::Antigravity,
    auth_status_method: "OAuth",
    aliases: &[],
    menu_detail: "Google Antigravity OAuth login",
    recommended: false,
    target: LoginProviderTarget::Antigravity,
    order: LoginProviderSurfaceOrder::new(Some(12), Some(12), None, Some(12), Some(12)),
};

pub const GOOGLE_LOGIN_PROVIDER: LoginProviderDescriptor = LoginProviderDescriptor {
    id: "google",
    display_name: "Google/Gmail",
    auth_kind: LoginProviderAuthKind::OAuth,
    auth_state_key: LoginProviderAuthStateKey::Google,
    auth_status_method: "OAuth",
    aliases: &["gmail"],
    menu_detail: "read, draft, and send emails",
    recommended: false,
    target: LoginProviderTarget::Google,
    order: LoginProviderSurfaceOrder::new(Some(13), None, None, None, None),
};

const LOGIN_PROVIDERS: [LoginProviderDescriptor; 17] = [
    CLAUDE_LOGIN_PROVIDER,
    OPENAI_LOGIN_PROVIDER,
    JCODE_LOGIN_PROVIDER,
    OPENROUTER_LOGIN_PROVIDER,
    AZURE_LOGIN_PROVIDER,
    OPENCODE_LOGIN_PROVIDER,
    OPENCODE_GO_LOGIN_PROVIDER,
    ZAI_LOGIN_PROVIDER,
    CHUTES_LOGIN_PROVIDER,
    CEREBRAS_LOGIN_PROVIDER,
    ALIBABA_CODING_PLAN_LOGIN_PROVIDER,
    OPENAI_COMPAT_LOGIN_PROVIDER,
    CURSOR_LOGIN_PROVIDER,
    COPILOT_LOGIN_PROVIDER,
    GEMINI_LOGIN_PROVIDER,
    ANTIGRAVITY_LOGIN_PROVIDER,
    GOOGLE_LOGIN_PROVIDER,
];

pub fn openai_compatible_profiles() -> &'static [OpenAiCompatibleProfile] {
    &OPENAI_COMPAT_PROFILES
}

pub fn login_providers() -> &'static [LoginProviderDescriptor] {
    &LOGIN_PROVIDERS
}

fn login_providers_for_surface(surface: LoginProviderSurface) -> Vec<LoginProviderDescriptor> {
    let mut providers = login_providers()
        .iter()
        .copied()
        .filter(|provider| provider.order.for_surface(surface).is_some())
        .collect::<Vec<_>>();
    providers.sort_by_key(|provider| provider.order.for_surface(surface).unwrap_or(u8::MAX));
    providers
}

pub fn cli_login_providers() -> Vec<LoginProviderDescriptor> {
    login_providers_for_surface(LoginProviderSurface::CliLogin)
}

pub fn tui_login_providers() -> Vec<LoginProviderDescriptor> {
    login_providers_for_surface(LoginProviderSurface::TuiLogin)
}

pub fn server_bootstrap_login_providers() -> Vec<LoginProviderDescriptor> {
    login_providers_for_surface(LoginProviderSurface::ServerBootstrap)
}

pub fn auto_init_login_providers() -> Vec<LoginProviderDescriptor> {
    login_providers_for_surface(LoginProviderSurface::AutoInit)
}

pub fn auth_status_login_providers() -> Vec<LoginProviderDescriptor> {
    login_providers_for_surface(LoginProviderSurface::AuthStatus)
}

pub fn resolve_login_provider(input: &str) -> Option<LoginProviderDescriptor> {
    let normalized = normalize_provider_input(input)?;
    login_providers().iter().copied().find(|provider| {
        provider.id == normalized || provider.aliases.iter().any(|alias| *alias == normalized)
    })
}

pub fn resolve_login_selection(
    input: &str,
    providers: &[LoginProviderDescriptor],
) -> Option<LoginProviderDescriptor> {
    let trimmed = input.trim();
    if let Ok(index) = trimmed.parse::<usize>() {
        return index
            .checked_sub(1)
            .and_then(|idx| providers.get(idx))
            .copied();
    }

    let provider = resolve_login_provider(trimmed)?;
    providers
        .iter()
        .copied()
        .find(|candidate| candidate.id == provider.id)
}

pub fn is_safe_env_key_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

pub fn is_safe_env_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

pub fn normalize_api_base(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parsed = url::Url::parse(trimmed).ok()?;
    let scheme = parsed.scheme();
    if scheme != "https" && scheme != "http" {
        return None;
    }

    if scheme == "http" {
        let host = parsed.host_str()?.to_ascii_lowercase();
        if host != "localhost" && host != "127.0.0.1" && host != "::1" {
            return None;
        }
    }

    Some(trimmed.trim_end_matches('/').to_string())
}

fn normalize_provider_input(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn matrix_profiles_have_unique_ids_and_safe_metadata() {
        let mut ids = HashSet::new();
        for profile in openai_compatible_profiles() {
            assert!(
                ids.insert(profile.id),
                "duplicate provider profile id: {}",
                profile.id
            );
            assert!(is_safe_env_key_name(profile.api_key_env));
            assert!(is_safe_env_file_name(profile.env_file));
            assert_eq!(
                normalize_api_base(profile.api_base).as_deref(),
                Some(profile.api_base)
            );
        }
    }

    #[test]
    fn matrix_login_provider_aliases_resolve_to_canonical_ids() {
        assert_eq!(
            resolve_login_provider("subscription").map(|provider| provider.id),
            Some("jcode")
        );
        assert_eq!(
            resolve_login_provider("anthropic").map(|provider| provider.id),
            Some("claude")
        );
        assert_eq!(
            resolve_login_provider("opencodego").map(|provider| provider.id),
            Some("opencode-go")
        );
        assert_eq!(
            resolve_login_provider("z.ai").map(|provider| provider.id),
            Some("zai")
        );
        assert_eq!(
            resolve_login_provider("compat").map(|provider| provider.id),
            Some("openai-compatible")
        );
        assert_eq!(
            resolve_login_provider("aoai").map(|provider| provider.id),
            Some("azure")
        );
        assert_eq!(
            resolve_login_provider("cerberascode").map(|provider| provider.id),
            Some("cerebras")
        );
        assert_eq!(
            resolve_login_provider("bailian").map(|provider| provider.id),
            Some("alibaba-coding-plan")
        );
        assert_eq!(
            resolve_login_provider("gmail").map(|provider| provider.id),
            Some("google")
        );
    }

    #[test]
    fn matrix_login_provider_ids_and_aliases_are_unique() {
        let mut seen = HashSet::new();
        for provider in login_providers() {
            assert!(
                seen.insert(provider.id),
                "duplicate login provider identifier: {}",
                provider.id
            );
            for alias in provider.aliases {
                assert!(
                    seen.insert(*alias),
                    "duplicate login provider alias: {}",
                    alias
                );
            }
        }
    }

    #[test]
    fn matrix_tui_login_selection_supports_numbers_and_names() {
        let providers = tui_login_providers();
        assert_eq!(
            resolve_login_selection("1", &providers).map(|provider| provider.id),
            Some("claude")
        );
        assert_eq!(
            resolve_login_selection("14", &providers).map(|provider| provider.id),
            Some("cursor")
        );
        assert_eq!(
            resolve_login_selection("compat", &providers).map(|provider| provider.id),
            Some("openai-compatible")
        );
        assert!(resolve_login_selection("google", &providers).is_none());
    }

    #[test]
    fn matrix_cli_login_selection_preserves_existing_order() {
        let providers = cli_login_providers();
        assert_eq!(
            resolve_login_selection("3", &providers).map(|provider| provider.id),
            Some("jcode")
        );
        assert_eq!(
            resolve_login_selection("4", &providers).map(|provider| provider.id),
            Some("copilot")
        );
        assert_eq!(
            resolve_login_selection("5", &providers).map(|provider| provider.id),
            Some("openrouter")
        );
        assert_eq!(
            resolve_login_selection("6", &providers).map(|provider| provider.id),
            Some("azure")
        );
        assert_eq!(
            resolve_login_selection("16", &providers).map(|provider| provider.id),
            Some("gemini")
        );
        assert_eq!(
            resolve_login_selection("17", &providers).map(|provider| provider.id),
            Some("google")
        );
    }
}
