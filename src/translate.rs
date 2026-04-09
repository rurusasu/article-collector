use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const OUTDIR: &str = "/tmp/collect";

#[derive(Debug, PartialEq)]
pub enum LlmProvider {
    OpenAI,
    Anthropic,
    ClaudeCode,
}

pub fn detect_provider(api_url: &str) -> LlmProvider {
    if api_url == "claude-code" {
        LlmProvider::ClaudeCode
    } else if api_url.contains("anthropic.com") {
        LlmProvider::Anthropic
    } else {
        LlmProvider::OpenAI
    }
}

pub async fn translate(input: &Path) -> Result<()> {
    let api_url = std::env::var("LLM_API_URL").unwrap_or_else(|_| "claude-code".to_string());
    let provider = detect_provider(&api_url);
    let lang = std::env::var("TRANSLATE_LANG").unwrap_or_else(|_| "ja".to_string());

    // Extract content from input JSON
    let raw = fs::read_to_string(input).context("Failed to read input file")?;
    let data: Value = serde_json::from_str(&raw).context("Failed to parse input JSON")?;
    let content = extract_content(&data);

    if content.is_empty() || content == "null" {
        bail!("No content extracted from {}", input.display());
    }

    // Translate main content
    let translated = translate_text(&provider, &api_url, &lang, &content).await?;
    let translated_path = PathBuf::from(OUTDIR).join("translated.md");
    fs::write(&translated_path, &translated)?;

    // Translate embedded articles if they exist
    let outdir = Path::new(OUTDIR);
    if let Ok(entries) = fs::read_dir(outdir) {
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

                let emb_translated =
                    translate_text(&provider, &api_url, &lang, &emb_content).await?;
                let emb_path = outdir.join(format!("{basename}_translated.md"));
                fs::write(emb_path, emb_translated)?;
            }
        }
    }

    eprintln!("Translation complete");
    Ok(())
}

async fn translate_text(
    provider: &LlmProvider,
    api_url: &str,
    lang: &str,
    content: &str,
) -> Result<String> {
    let prompt = format!(
        "以下の記事を{lang}に翻訳してください。Markdown形式を維持し、技術用語は適切に翻訳してください。翻訳結果のみを出力してください。\n\n{content}"
    );

    match provider {
        LlmProvider::ClaudeCode => call_claude_code(&prompt),
        LlmProvider::Anthropic | LlmProvider::OpenAI => {
            let api_token =
                std::env::var("LLM_API_TOKEN").context("LLM_API_TOKEN env var required")?;
            let default_model = match provider {
                LlmProvider::Anthropic => "claude-sonnet-4-20250514",
                _ => "gpt-4o",
            };
            let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| default_model.to_string());
            let endpoint = resolve_endpoint(api_url, provider);
            let client = reqwest::Client::new();
            call_llm_api(&client, &endpoint, &api_token, &model, &prompt, provider).await
        }
    }
}

fn call_claude_code(prompt: &str) -> Result<String> {
    use std::io::Write;
    use std::process::Stdio;

    eprintln!("Calling claude CLI...");

    // Ensure pnpm/npm global bin dirs are in PATH
    let mut path = std::env::var("PATH").unwrap_or_default();
    if let Some(home) = dirs::home_dir() {
        for extra in [
            home.join("AppData/Local/pnpm"),
            home.join(".npm-global/bin"),
            home.join(".local/bin"),
        ] {
            if extra.exists() {
                path = format!("{};{}", extra.display(), path);
            }
        }
    }

    // Pipe prompt via stdin to avoid argument escaping issues on Windows
    let mut child = Command::new("cmd")
        .args(["/c", "claude", "-p", "-"])
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

pub fn resolve_endpoint(api_url: &str, provider: &LlmProvider) -> String {
    match provider {
        LlmProvider::Anthropic => {
            if api_url.ends_with("/messages") {
                api_url.to_string()
            } else {
                let base = api_url.trim_end_matches('/');
                format!("{base}/v1/messages")
            }
        }
        LlmProvider::OpenAI => {
            if api_url.ends_with("/chat/completions") {
                api_url.to_string()
            } else {
                let base = api_url.trim_end_matches('/');
                format!("{base}/chat/completions")
            }
        }
        LlmProvider::ClaudeCode => String::new(),
    }
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

    // ── detect_provider ──

    #[test]
    fn detects_claude_code_provider() {
        assert_eq!(detect_provider("claude-code"), LlmProvider::ClaudeCode);
    }

    #[test]
    fn detects_anthropic_provider() {
        assert_eq!(
            detect_provider("https://api.anthropic.com"),
            LlmProvider::Anthropic
        );
    }

    #[test]
    fn detects_openai_provider() {
        assert_eq!(
            detect_provider("https://api.openai.com/v1"),
            LlmProvider::OpenAI
        );
    }

    #[test]
    fn detects_generic_as_openai() {
        assert_eq!(
            detect_provider("http://localhost:8080"),
            LlmProvider::OpenAI
        );
    }

    // ── resolve_endpoint ──

    #[test]
    fn openai_endpoint_already_has_chat_completions() {
        assert_eq!(
            resolve_endpoint(
                "http://localhost:8080/chat/completions",
                &LlmProvider::OpenAI
            ),
            "http://localhost:8080/chat/completions"
        );
    }

    #[test]
    fn openai_endpoint_with_v1_trailing_slash() {
        assert_eq!(
            resolve_endpoint("http://localhost:8080/v1/", &LlmProvider::OpenAI),
            "http://localhost:8080/v1/chat/completions"
        );
    }

    #[test]
    fn openai_endpoint_bare_url() {
        assert_eq!(
            resolve_endpoint("http://localhost:8080", &LlmProvider::OpenAI),
            "http://localhost:8080/chat/completions"
        );
    }

    #[test]
    fn anthropic_endpoint_already_has_messages() {
        assert_eq!(
            resolve_endpoint(
                "https://api.anthropic.com/v1/messages",
                &LlmProvider::Anthropic
            ),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn anthropic_endpoint_bare_url() {
        assert_eq!(
            resolve_endpoint("https://api.anthropic.com", &LlmProvider::Anthropic),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn claude_code_endpoint_is_empty() {
        assert_eq!(
            resolve_endpoint("claude-code", &LlmProvider::ClaudeCode),
            ""
        );
    }

    // ── extract_content (array) ──

    #[test]
    fn extract_content_from_array_with_content_field() {
        let data = json!([{"title": "test", "content": "hello world"}]);
        assert_eq!(extract_content(&data), "hello world");
    }

    #[test]
    fn extract_content_from_array_with_title_only() {
        let data = json!([{"title": "my title"}]);
        assert_eq!(extract_content(&data), "my title");
    }

    #[test]
    fn extract_content_from_array_joins_multiple() {
        let data = json!([
            {"content": "first"},
            {"content": "second"}
        ]);
        assert_eq!(extract_content(&data), "first\n\n---\n\nsecond");
    }

    #[test]
    fn extract_content_from_empty_array() {
        let data = json!([]);
        assert_eq!(extract_content(&data), "");
    }

    #[test]
    fn extract_content_from_array_with_empty_objects() {
        let data = json!([{}]);
        assert_eq!(extract_content(&data), "");
    }

    // ── extract_single_content ──

    #[test]
    fn extract_single_prefers_text() {
        let data = json!({"text": "from text", "content": "from content", "title": "from title"});
        assert_eq!(extract_single_content(&data), "from text");
    }

    #[test]
    fn extract_single_falls_back_to_content() {
        let data = json!({"content": "from content", "title": "from title"});
        assert_eq!(extract_single_content(&data), "from content");
    }

    #[test]
    fn extract_single_falls_back_to_title() {
        let data = json!({"title": "from title"});
        assert_eq!(extract_single_content(&data), "from title");
    }

    #[test]
    fn extract_single_returns_empty_for_null() {
        let data = json!({});
        assert_eq!(extract_single_content(&data), "");
    }
}
