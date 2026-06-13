use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::paths;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LlmProvider {
    OpenAI,
    Anthropic,
    ClaudeCode,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ApiTokenRule {
    env_var: &'static str,
    prefix: &'static str,
    min_chars: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ModelConfig {
    env_var: &'static str,
    default_model: &'static str,
}

impl LlmProvider {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "openai" => Ok(LlmProvider::OpenAI),
            "anthropic" => Ok(LlmProvider::Anthropic),
            "claude-code" | "claude_code" => Ok(LlmProvider::ClaudeCode),
            other => bail!(
                "Unsupported LLM_PROVIDER '{other}'. Use one of: openai, anthropic, claude-code"
            ),
        }
    }

    fn from_env() -> Result<Self> {
        let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "claude-code".to_string());
        Self::parse(&provider)
    }

    pub fn api_endpoint(self) -> Option<&'static str> {
        match self {
            LlmProvider::OpenAI => Some("https://api.openai.com/v1/chat/completions"),
            LlmProvider::Anthropic => Some("https://api.anthropic.com/v1/messages"),
            LlmProvider::ClaudeCode => None,
        }
    }

    pub fn token_rule(self) -> Option<ApiTokenRule> {
        match self {
            LlmProvider::OpenAI => Some(ApiTokenRule {
                env_var: "OPENAI_API_KEY",
                prefix: "sk-proj-",
                min_chars: 40,
            }),
            LlmProvider::Anthropic => Some(ApiTokenRule {
                env_var: "ANTHROPIC_API_KEY",
                prefix: "sk-ant-api03-",
                min_chars: 40,
            }),
            LlmProvider::ClaudeCode => None,
        }
    }

    pub fn model_config(self) -> Option<ModelConfig> {
        match self {
            LlmProvider::OpenAI => Some(ModelConfig {
                env_var: "OPENAI_MODEL",
                default_model: "gpt-4o",
            }),
            LlmProvider::Anthropic => Some(ModelConfig {
                env_var: "ANTHROPIC_MODEL",
                default_model: "claude-sonnet-4-20250514",
            }),
            LlmProvider::ClaudeCode => None,
        }
    }

    fn api_token(self) -> Result<Option<String>> {
        let Some(rule) = self.token_rule() else {
            return Ok(None);
        };
        let token = std::env::var(rule.env_var)
            .with_context(|| format!("{} env var required", rule.env_var))?;
        rule.validate(&token)?;
        Ok(Some(token))
    }

    fn model(self) -> Option<String> {
        let config = self.model_config()?;
        Some(std::env::var(config.env_var).unwrap_or_else(|_| config.default_model.to_string()))
    }
}

impl ApiTokenRule {
    pub fn validate(&self, token: &str) -> Result<()> {
        if !token.starts_with(self.prefix) {
            bail!(
                "{} must start with '{}' for the selected provider",
                self.env_var,
                self.prefix
            );
        }

        let char_count = token.chars().count();
        if char_count < self.min_chars {
            bail!(
                "{} must be at least {} characters long; got {}",
                self.env_var,
                self.min_chars,
                char_count
            );
        }

        Ok(())
    }
}

pub async fn translate(input: &Path) -> Result<()> {
    let provider = LlmProvider::from_env()?;
    let lang = std::env::var("TRANSLATE_LANG").unwrap_or_else(|_| "ja".to_string());

    // Extract content from input JSON
    let raw = fs::read_to_string(input).context("Failed to read input file")?;
    let data: Value = serde_json::from_str(&raw).context("Failed to parse input JSON")?;
    let content = extract_content(&data);

    if content.is_empty() || content == "null" {
        bail!("No content extracted from {}", input.display());
    }

    // Translate main content
    let translated = translate_text(&provider, &lang, &content).await?;
    let outdir = paths::outdir();
    fs::create_dir_all(&outdir)?;
    let translated_path = paths::translated_md_path();
    fs::write(&translated_path, &translated)?;

    // Translate embedded articles if they exist
    if let Ok(entries) = fs::read_dir(&outdir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("embedded_") && name.ends_with(".json") {
                let basename = name.trim_end_matches(".json");
                let emb_raw = fs::read_to_string(entry.path())?;
                let emb_data: Value = serde_json::from_str(&emb_raw).unwrap_or(Value::Null);
                let emb_content = extract_single_content(&emb_data);

                if emb_content.is_empty() || emb_content == "null" {
                    continue;
                }

                let emb_translated = translate_text(&provider, &lang, &emb_content).await?;
                let emb_path = outdir.join(format!("{basename}_translated.md"));
                fs::write(emb_path, emb_translated)?;
            }
        }
    }

    eprintln!("Translation complete");
    Ok(())
}

async fn translate_text(provider: &LlmProvider, lang: &str, content: &str) -> Result<String> {
    let prompt = format!(
        "以下の記事を{lang}に翻訳してください。Markdown形式を維持し、技術用語は適切に翻訳してください。翻訳結果のみを出力してください。\n\n{content}"
    );

    match provider {
        LlmProvider::ClaudeCode => call_claude_code(&prompt),
        LlmProvider::Anthropic | LlmProvider::OpenAI => {
            let api_token = provider
                .api_token()?
                .context("API token config missing for API provider")?;
            let model = provider
                .model()
                .context("Model config missing for API provider")?;
            let endpoint = provider
                .api_endpoint()
                .context("API endpoint config missing for API provider")?;
            let client = reqwest::Client::new();
            call_llm_api(&client, endpoint, &api_token, &model, &prompt, provider).await
        }
    }
}

fn call_claude_code(prompt: &str) -> Result<String> {
    use std::io::Write;
    use std::process::Stdio;

    eprintln!("Calling claude CLI...");

    let path = augmented_path();

    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/c", "claude", "-p", "-"]);
        command
    } else {
        let mut command = Command::new("claude");
        command.args(["-p", "-"]);
        command
    };

    // Pipe prompt via stdin to avoid argument escaping issues.
    let mut child = command
        .env("PATH", &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to run 'claude' CLI. Is Claude Code installed?")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())?;
    }

    let output = child
        .wait_with_output()
        .context("Failed to wait for claude CLI")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("claude CLI failed: {stderr}");
    }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if result.is_empty() {
        bail!("claude CLI returned empty output");
    }

    Ok(result)
}

fn augmented_path() -> std::ffi::OsString {
    let mut paths = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();

    if let Some(home) = dirs::home_dir() {
        for extra in [
            home.join("AppData/Local/pnpm"),
            home.join(".npm-global/bin"),
            home.join(".local/bin"),
        ]
        .into_iter()
        .rev()
        {
            if extra.exists() {
                paths.insert(0, extra);
            }
        }
    }

    std::env::join_paths(paths).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

pub fn extract_content(data: &Value) -> String {
    if let Some(arr) = data.as_array() {
        arr.iter()
            .filter_map(|item| {
                item.get("text")
                    .or_else(|| item.get("content"))
                    .or_else(|| item.get("title"))
                    .and_then(|v| v.as_str())
            })
            .collect::<Vec<&str>>()
            .join("\n\n---\n\n")
    } else {
        extract_single_content(data)
    }
}

pub fn extract_single_content(data: &Value) -> String {
    data.get("text")
        .or_else(|| data.get("content"))
        .or_else(|| data.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

async fn call_llm_api(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    model: &str,
    prompt: &str,
    provider: &LlmProvider,
) -> Result<String> {
    let resp: Value = match provider {
        LlmProvider::Anthropic => {
            let body = json!({
                "model": model,
                "max_tokens": 8192,
                "messages": [{"role": "user", "content": prompt}]
            });
            client
                .post(endpoint)
                .header("x-api-key", token)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .context("Anthropic API request failed")?
                .json()
                .await
                .context("Failed to parse Anthropic API response")?
        }
        LlmProvider::OpenAI => {
            let body = json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}]
            });
            client
                .post(endpoint)
                .header("Authorization", format!("Bearer {token}"))
                .json(&body)
                .send()
                .await
                .context("LLM API request failed")?
                .json()
                .await
                .context("Failed to parse LLM API response")?
        }
        LlmProvider::ClaudeCode => unreachable!(),
    };

    let translated = match provider {
        LlmProvider::Anthropic => resp
            .pointer("/content/0/text")
            .and_then(|c| c.as_str())
            .unwrap_or(""),
        LlmProvider::OpenAI => resp
            .pointer("/choices/0/message/content")
            .and_then(|c| c.as_str())
            .unwrap_or(""),
        LlmProvider::ClaudeCode => unreachable!(),
    };

    if translated.is_empty() || translated == "null" {
        let preview: String = resp.to_string().chars().take(500).collect();
        bail!("Translation API returned empty/null response. Response: {preview}");
    }

    Ok(translated.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── LlmProvider ──

    /// 検証: LLM_PROVIDER の "claude-code" を ClaudeCode プロバイダーとして解釈する
    /// 理由: Claude Code CLI はローカル実行で HTTP API とは異なる呼び出し方法を使う
    /// リスク: Claude Code が HTTP API として呼び出され、接続エラーになる
    #[test]
    fn parses_claude_code_provider() {
        assert_eq!(
            LlmProvider::parse("claude-code").unwrap(),
            LlmProvider::ClaudeCode
        );
    }

    /// 検証: LLM_PROVIDER の "anthropic" を Anthropic プロバイダーとして解釈する
    /// 理由: Anthropic は独自の API フォーマット（messages API）を使う
    /// リスク: Anthropic API に OpenAI フォーマットのリクエストを送信し、400 エラーになる
    #[test]
    fn parses_anthropic_provider() {
        assert_eq!(
            LlmProvider::parse("anthropic").unwrap(),
            LlmProvider::Anthropic
        );
    }

    /// 検証: LLM_PROVIDER の "openai" を OpenAI プロバイダーとして解釈する
    /// 理由: OpenAI は chat/completions エンドポイントを使う
    /// リスク: OpenAI API に Anthropic フォーマットのリクエストを送信し、エラーになる
    #[test]
    fn parses_openai_provider() {
        assert_eq!(LlmProvider::parse("openai").unwrap(), LlmProvider::OpenAI);
    }

    /// 検証: 未知の provider 名を拒否する
    /// 理由: URL 文字列からの暗黙 fallback をなくし、設定ミスを早期に検出する
    /// リスク: 意図しない provider として実行され、別 API に誤送信される
    #[test]
    fn rejects_unknown_provider() {
        assert!(LlmProvider::parse("http://localhost:8080").is_err());
    }

    // ── provider config ──

    /// 検証: OpenAI の endpoint は provider enum の固定値として定義される
    /// 理由: provider 選択と API endpoint を別概念として扱う
    /// リスク: 無効な URL 指定で翻訳が失敗する
    #[test]
    fn openai_endpoint_is_fixed() {
        assert_eq!(
            LlmProvider::OpenAI.api_endpoint(),
            Some("https://api.openai.com/v1/chat/completions")
        );
    }

    /// 検証: Anthropic の endpoint は provider enum の固定値として定義される
    /// 理由: 公式 API の endpoint をユーザー設定に委ねない
    /// リスク: messages API 以外へ送信され、API 呼び出しが失敗する
    #[test]
    fn anthropic_endpoint_is_fixed() {
        assert_eq!(
            LlmProvider::Anthropic.api_endpoint(),
            Some("https://api.anthropic.com/v1/messages")
        );
    }

    /// 検証: Claude Code は HTTP endpoint を持たない
    /// 理由: Claude Code は CLI 経由で呼び出すため、HTTP エンドポイントは不要
    /// リスク: HTTP リクエストが誤って送信される
    #[test]
    fn claude_code_endpoint_is_none() {
        assert_eq!(LlmProvider::ClaudeCode.api_endpoint(), None);
    }

    /// 検証: OpenAI の token 設定は provider 固有の env var と prefix を持つ
    /// 理由: provider ごとの API key 形式を明示する
    /// リスク: 別 provider の token を OpenAI に送信する
    #[test]
    fn openai_token_rule_is_provider_specific() {
        assert_eq!(
            LlmProvider::OpenAI.token_rule(),
            Some(ApiTokenRule {
                env_var: "OPENAI_API_KEY",
                prefix: "sk-proj-",
                min_chars: 40
            })
        );
    }

    /// 検証: Anthropic の token 設定は provider 固有の env var と prefix を持つ
    /// 理由: provider ごとの API key 形式を明示する
    /// リスク: 別 provider の token を Anthropic に送信する
    #[test]
    fn anthropic_token_rule_is_provider_specific() {
        assert_eq!(
            LlmProvider::Anthropic.token_rule(),
            Some(ApiTokenRule {
                env_var: "ANTHROPIC_API_KEY",
                prefix: "sk-ant-api03-",
                min_chars: 40
            })
        );
    }

    /// 検証: OpenAI token は prefix と文字数の両方を満たす必要がある
    /// 理由: provider を間違えた token や途中で欠けた token を早期検出する
    /// リスク: API 呼び出し後まで設定ミスに気づけない
    #[test]
    fn validates_openai_token_prefix_and_length() {
        let rule = LlmProvider::OpenAI.token_rule().unwrap();
        assert!(rule
            .validate("sk-proj-abcdefghijklmnopqrstuvwxyz123456")
            .is_ok());
        assert!(rule
            .validate("sk-ant-api03-abcdefghijklmnopqrstuvwxyz123456")
            .is_err());
        assert!(rule.validate("sk-proj-short").is_err());
    }

    /// 検証: Anthropic token は prefix と文字数の両方を満たす必要がある
    /// 理由: provider を間違えた token や途中で欠けた token を早期検出する
    /// リスク: API 呼び出し後まで設定ミスに気づけない
    #[test]
    fn validates_anthropic_token_prefix_and_length() {
        let rule = LlmProvider::Anthropic.token_rule().unwrap();
        assert!(rule
            .validate("sk-ant-api03-abcdefghijklmnopqrstuvwxyz123456")
            .is_ok());
        assert!(rule
            .validate("sk-proj-abcdefghijklmnopqrstuvwxyz123456")
            .is_err());
        assert!(rule.validate("sk-ant-api03-short").is_err());
    }

    /// 検証: provider ごとに model env var と default model を持つ
    /// 理由: 共通 model env var では provider 切替時に不正な model を指定しやすい
    /// リスク: OpenAI 用 model を Anthropic に渡す等の設定事故が起きる
    #[test]
    fn model_config_is_provider_specific() {
        assert_eq!(
            LlmProvider::OpenAI.model_config(),
            Some(ModelConfig {
                env_var: "OPENAI_MODEL",
                default_model: "gpt-4o"
            })
        );
        assert_eq!(
            LlmProvider::Anthropic.model_config(),
            Some(ModelConfig {
                env_var: "ANTHROPIC_MODEL",
                default_model: "claude-sonnet-4-20250514"
            })
        );
        assert_eq!(LlmProvider::ClaudeCode.model_config(), None);
    }

    // ── extract_content (array) ──

    /// 検証: content フィールドから記事本文を抽出する
    /// 理由: fetch で取得した JSON の content フィールドが翻訳対象
    /// リスク: 記事本文が空のまま翻訳に渡され、空の翻訳結果が保存される
    #[test]
    fn extract_content_from_array_with_content_field() {
        let data = json!([{"title": "test", "content": "hello world"}]);
        assert_eq!(extract_content(&data), "hello world");
    }

    /// 検証: content がない場合に title をフォールバックとして使う
    /// 理由: 一部の取得先（ツイート等）は content なしで title のみの場合がある
    /// リスク: title がフォールバックされず、翻訳対象が空になる
    #[test]
    fn extract_content_from_array_with_title_only() {
        let data = json!([{"title": "my title"}]);
        assert_eq!(extract_content(&data), "my title");
    }

    /// 検証: 複数記事を区切り文字で結合する
    /// 理由: 埋め込み記事等で複数要素が1つの JSON に含まれる場合がある
    /// リスク: 記事が結合されず、最初の1つだけが翻訳される
    #[test]
    fn extract_content_from_array_joins_multiple() {
        let data = json!([
            {"content": "first"},
            {"content": "second"}
        ]);
        assert_eq!(extract_content(&data), "first\n\n---\n\nsecond");
    }

    /// 検証: 空配列で空文字列を返す（パニックしない）
    /// 理由: fetch が空の結果を返す可能性がある
    /// リスク: 空配列で unwrap パニックが発生する
    #[test]
    fn extract_content_from_empty_array() {
        let data = json!([]);
        assert_eq!(extract_content(&data), "");
    }

    /// 検証: フィールドが空のオブジェクトで空文字列を返す
    /// 理由: JSON 構造が期待と異なる場合にも安全に動作する必要がある
    /// リスク: None に対する unwrap でパニックが発生する
    #[test]
    fn extract_content_from_array_with_empty_objects() {
        let data = json!([{}]);
        assert_eq!(extract_content(&data), "");
    }

    // ── extract_single_content ──

    /// 検証: text, content, title の優先順位で text を優先する
    /// 理由: text は HN 等で本文全体を含むフィールド
    /// リスク: 短い title が text より優先され、本文が翻訳されない
    #[test]
    fn extract_single_prefers_text() {
        let data = json!({"text": "from text", "content": "from content", "title": "from title"});
        assert_eq!(extract_single_content(&data), "from text");
    }

    /// 検証: text がない場合に content をフォールバックする
    /// 理由: Dev.to 等は content フィールドに本文が格納される
    /// リスク: content がスキップされ、title のみが翻訳される
    #[test]
    fn extract_single_falls_back_to_content() {
        let data = json!({"content": "from content", "title": "from title"});
        assert_eq!(extract_single_content(&data), "from content");
    }

    /// 検証: text も content もない場合に title をフォールバックする
    /// 理由: 最低限タイトルだけでも翻訳対象として抽出する
    /// リスク: 翻訳対象が完全に空になる
    #[test]
    fn extract_single_falls_back_to_title() {
        let data = json!({"title": "from title"});
        assert_eq!(extract_single_content(&data), "from title");
    }

    /// 検証: 全フィールドが欠如する JSON で空文字列を返す
    /// 理由: 予期しない JSON 構造でもパニックしない安全性が必要
    /// リスク: unwrap パニックでパイプライン全体が停止する
    #[test]
    fn extract_single_returns_empty_for_null() {
        let data = json!({});
        assert_eq!(extract_single_content(&data), "");
    }
}
