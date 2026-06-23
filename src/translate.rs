use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command as TokioCommand};
use tokio::time::timeout;

use crate::paths;

#[derive(Debug, PartialEq, Eq)]
pub enum TranslateOutcome {
    Translated,
    Skipped,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TranslateContentOutcome {
    Translated(String),
    Skipped,
}

pub async fn translate(input: &Path) -> Result<TranslateOutcome> {
    let Some(agent) = acp_agent_from_env()? else {
        eprintln!("Error: ACP_AGENT is not set. Translation skipped.");
        return Ok(TranslateOutcome::Skipped);
    };

    let lang = std::env::var("TRANSLATE_LANG").unwrap_or_else(|_| "ja".to_string());

    // Extract content from input JSON
    let raw = fs::read_to_string(input).context("Failed to read input file")?;
    let data: Value = serde_json::from_str(&raw).context("Failed to parse input JSON")?;
    let content = extract_content(&data);

    if content.is_empty() || content == "null" {
        bail!("No content extracted from {}", input.display());
    }

    // Translate main content
    let translated = translate_text(agent, &lang, &content).await?;
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

                let emb_translated = translate_text(agent, &lang, &emb_content).await?;
                let emb_path = outdir.join(format!("{basename}_translated.md"));
                fs::write(emb_path, emb_translated)?;
            }
        }
    }

    eprintln!("Translation complete");
    Ok(TranslateOutcome::Translated)
}

pub async fn translate_content(content: &str) -> Result<TranslateContentOutcome> {
    if content.trim().is_empty() {
        bail!("No content extracted for translation");
    }

    let Some(agent) = acp_agent_from_env()? else {
        eprintln!("Error: ACP_AGENT is not set. Translation skipped.");
        return Ok(TranslateContentOutcome::Skipped);
    };

    let lang = std::env::var("TRANSLATE_LANG").unwrap_or_else(|_| "ja".to_string());
    let translated = translate_text(agent, &lang, content).await?;
    Ok(TranslateContentOutcome::Translated(translated))
}

#[cfg(test)]
fn translate_content_outcome_for_agent(
    agent: Option<&str>,
    content: &str,
) -> Result<TranslateContentOutcome> {
    if content.trim().is_empty() {
        bail!("No content extracted for translation");
    }

    let agent = acp_agent_from_value(agent)?;
    if agent.is_none() {
        return Ok(TranslateContentOutcome::Skipped);
    }

    Ok(TranslateContentOutcome::Translated(String::new()))
}

async fn translate_text(agent: AcpAgent, lang: &str, content: &str) -> Result<String> {
    let prompt = format!(
        "以下の記事を{lang}に翻訳してください。Markdown形式を維持し、技術用語は適切に翻訳してください。翻訳結果のみを出力してください。\n\n{content}"
    );

    call_acp_agent(agent, &prompt).await
}

async fn call_acp_agent(agent: AcpAgent, prompt: &str) -> Result<String> {
    eprintln!("Calling ACP agent...");

    let mut client = AcpJsonRpcClient::spawn(agent.command()).await?;

    let initialize = client
        .request("initialize", acp_initialize_params())
        .await
        .context("ACP initialize failed")?;
    let protocol_version = initialize
        .get("protocolVersion")
        .and_then(Value::as_i64)
        .context("ACP initialize response missing protocolVersion")?;
    if protocol_version != 1 {
        bail!("ACP agent negotiated unsupported protocolVersion {protocol_version}");
    }

    let session = client
        .request("session/new", acp_new_session_params()?)
        .await
        .context("ACP session/new failed")?;
    let session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .context("ACP session/new response missing sessionId")?
        .to_string();

    let mut translated = String::new();
    let prompt_response = client
        .request_collecting_text(
            "session/prompt",
            acp_prompt_params(&session_id, prompt),
            &mut translated,
        )
        .await
        .context("ACP session/prompt failed")?;

    let stop_reason = prompt_response
        .get("stopReason")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if stop_reason != "end_turn" {
        bail!("ACP agent stopped before completing translation: {stop_reason}");
    }

    let translated = translated.trim().to_string();
    if translated.is_empty() {
        bail!("ACP agent returned empty translation");
    }

    client.shutdown().await;
    Ok(translated)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum AcpAgent {
    Codex,
    Gemini,
    Claude,
}

impl AcpAgent {
    fn parse(agent: &str) -> Result<Self> {
        match agent.trim().to_ascii_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "gemini" => Ok(Self::Gemini),
            "claude" | "claude-agent" => Ok(Self::Claude),
            other => bail!("Unsupported ACP_AGENT '{other}'. Use one of: codex, gemini, claude"),
        }
    }

    fn command(self) -> AcpAgentCommand {
        match self {
            Self::Codex => AcpAgentCommand::new(
                command_shim("npx"),
                &["-y", "@agentclientprotocol/codex-acp"],
            ),
            Self::Gemini => AcpAgentCommand::new(command_shim("gemini"), &["--acp"]),
            Self::Claude => AcpAgentCommand::new(
                command_shim("npx"),
                &["-y", "@agentclientprotocol/claude-agent-acp"],
            ),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct AcpAgentCommand {
    program: String,
    args: Vec<String>,
}

impl AcpAgentCommand {
    fn new(program: impl Into<String>, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
        }
    }

    fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn acp_agent_from_env() -> Result<Option<AcpAgent>> {
    match std::env::var("ACP_AGENT") {
        Ok(agent) => acp_agent_from_value(Some(&agent)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error).context("Failed to read ACP_AGENT"),
    }
}

fn acp_agent_from_value(agent: Option<&str>) -> Result<Option<AcpAgent>> {
    let Some(agent) = agent else {
        return Ok(None);
    };

    let agent = agent.trim();
    if agent.is_empty() {
        return Ok(None);
    }

    Ok(Some(AcpAgent::parse(agent)?))
}

fn command_shim(command: &str) -> String {
    if cfg!(windows) {
        format!("{command}.cmd")
    } else {
        command.to_string()
    }
}

fn acp_initialize_params() -> Value {
    json!({
        "protocolVersion": 1,
        "clientCapabilities": {},
        "clientInfo": {
            "name": "article-collector",
            "title": "article-collector",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn acp_new_session_params() -> Result<Value> {
    let cwd = std::env::current_dir().context("Failed to resolve current directory for ACP")?;
    Ok(json!({
        "cwd": cwd.to_string_lossy(),
        "mcpServers": []
    }))
}

fn acp_prompt_params(session_id: &str, prompt: &str) -> Value {
    json!({
        "sessionId": session_id,
        "prompt": [
            {
                "type": "text",
                "text": prompt
            }
        ]
    })
}

struct AcpJsonRpcClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl AcpJsonRpcClient {
    async fn spawn(agent_command: AcpAgentCommand) -> Result<Self> {
        let display = agent_command.display();
        let mut command = TokioCommand::new(&agent_command.program);
        command.args(&agent_command.args);

        let mut child = command
            .env("PATH", augmented_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("Failed to run ACP agent command: {display}"))?;

        let stdin = child
            .stdin
            .take()
            .context("Failed to open ACP agent stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Failed to open ACP agent stdout")?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
        })
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let mut ignored_text = String::new();
        self.request_collecting_text(method, params, &mut ignored_text)
            .await
    }

    async fn request_collecting_text(
        &mut self,
        method: &str,
        params: Value,
        text: &mut String,
    ) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        self.send_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))
        .await?;

        loop {
            let message = self.read_message().await?;
            let response_id = json!(id);
            if message.get("id") == Some(&response_id) && message.get("method").is_none() {
                if let Some(error) = message.get("error") {
                    bail!("ACP {method} returned error: {}", format_rpc_error(error));
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }

            self.handle_incoming(message, text).await?;
        }
    }

    async fn send_message(&mut self, message: &Value) -> Result<()> {
        let mut line = serde_json::to_vec(message).context("Failed to encode ACP message")?;
        line.push(b'\n');
        self.stdin
            .write_all(&line)
            .await
            .context("Failed to write ACP message")?;
        self.stdin
            .flush()
            .await
            .context("Failed to flush ACP message")?;
        Ok(())
    }

    async fn read_message(&mut self) -> Result<Value> {
        let mut line = String::new();
        let bytes = self
            .stdout
            .read_line(&mut line)
            .await
            .context("Failed to read ACP message")?;
        if bytes == 0 {
            bail!("ACP agent exited before sending a response");
        }

        let trimmed = line.trim_end();
        serde_json::from_str(trimmed)
            .with_context(|| format!("ACP agent emitted non-JSON stdout: {trimmed}"))
    }

    async fn handle_incoming(&mut self, message: Value, text: &mut String) -> Result<()> {
        if message.get("method").and_then(Value::as_str) == Some("session/update") {
            append_acp_text_update(&message, text);
            return Ok(());
        }

        if message.get("method").is_some() && message.get("id").is_some() {
            self.respond_to_agent_request(&message).await?;
        }

        Ok(())
    }

    async fn respond_to_agent_request(&mut self, request: &Value) -> Result<()> {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");

        let response = match method {
            "session/request_permission" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": acp_cancelled_permission_response()
            }),
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!(
                        "article-collector does not support ACP client method '{method}'"
                    )
                }
            }),
        };

        self.send_message(&response).await
    }

    async fn shutdown(mut self) {
        let _ = self.stdin.shutdown().await;
        if matches!(
            timeout(Duration::from_millis(500), self.child.wait()).await,
            Ok(Ok(_))
        ) {
            return;
        }

        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

impl Drop for AcpJsonRpcClient {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn acp_cancelled_permission_response() -> Value {
    json!({
        "outcome": {
            "outcome": "cancelled"
        }
    })
}

fn append_acp_text_update(message: &Value, output: &mut String) {
    let update = &message["params"]["update"];
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("agent_message_chunk") {
        return;
    }

    let content = &update["content"];
    if content.get("type").and_then(Value::as_str) != Some("text") {
        return;
    }

    if let Some(text) = content.get("text").and_then(Value::as_str) {
        output.push_str(text);
    }
}

fn format_rpc_error(error: &Value) -> String {
    let code = error
        .get("code")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    format!("{code}: {message}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── ACP JSON-RPC ──

    /// 検証: ACP initialize request は protocolVersion 1 と clientInfo を含む
    /// 理由: ACP 接続は initialize で protocol version を合意してから session を作る
    /// リスク: agent が protocol 不一致やクライアント情報不足で接続を拒否する
    #[test]
    fn acp_initialize_params_include_protocol_and_client_info() {
        let params = acp_initialize_params();
        assert_eq!(params["protocolVersion"], json!(1));
        assert_eq!(params["clientInfo"]["name"], json!("article-collector"));
    }

    /// 検証: ACP prompt request は text content block として翻訳 prompt を送る
    /// 理由: ACP では user message を ContentBlock[] として `session/prompt` に渡す
    /// リスク: agent が prompt を読めず翻訳を開始できない
    #[test]
    fn acp_prompt_params_use_text_content_block() {
        let params = acp_prompt_params("sess_1", "translate me");
        assert_eq!(params["sessionId"], json!("sess_1"));
        assert_eq!(params["prompt"][0]["type"], json!("text"));
        assert_eq!(params["prompt"][0]["text"], json!("translate me"));
    }

    /// 検証: ACP agent_message_chunk の text だけを翻訳結果として蓄積する
    /// 理由: plan や tool_call などの非本文 update を translated.md に混ぜない
    /// リスク: 進捗ログや tool 情報が翻訳本文に混入する
    #[test]
    fn append_acp_text_update_collects_agent_text_only() {
        let mut output = String::new();
        append_acp_text_update(
            &json!({
                "method": "session/update",
                "params": {
                    "update": {
                        "sessionUpdate": "agent_message_chunk",
                        "content": {
                            "type": "text",
                            "text": "hello"
                        }
                    }
                }
            }),
            &mut output,
        );
        append_acp_text_update(
            &json!({
                "method": "session/update",
                "params": {
                    "update": {
                        "sessionUpdate": "plan",
                        "entries": []
                    }
                }
            }),
            &mut output,
        );

        assert_eq!(output, "hello");
    }

    /// 検証: permission request は cancelled outcome で返す
    /// 理由: 翻訳用途の ACP client は agent のファイル編集や端末実行を許可しない
    /// リスク: 翻訳実行中に想定外の tool 実行が許可される
    #[test]
    fn acp_permission_response_is_cancelled() {
        assert_eq!(
            acp_cancelled_permission_response(),
            json!({
                "outcome": {
                    "outcome": "cancelled"
                }
            })
        );
    }

    /// 検証: ACP_AGENT の短い名前を enum として解釈する
    /// 理由: agent 名と起動コマンドの対応を型で管理する
    /// リスク: 文字列 match が散らばり、agent 追加時に設定漏れが起きる
    #[test]
    fn parses_acp_agent_names() {
        assert_eq!(AcpAgent::parse("codex").unwrap(), AcpAgent::Codex);
        assert_eq!(AcpAgent::parse("gemini").unwrap(), AcpAgent::Gemini);
        assert_eq!(AcpAgent::parse("claude").unwrap(), AcpAgent::Claude);
        assert_eq!(AcpAgent::parse("claude-agent").unwrap(), AcpAgent::Claude);
    }

    /// 検証: AcpAgent enum から起動コマンドを解決する
    /// 理由: agent 名と command の対応を enum 実装に閉じ込める
    /// リスク: agent 名と実際の adapter 起動コマンドがずれて翻訳が開始できない
    #[test]
    fn maps_acp_agent_to_command() {
        assert_eq!(
            AcpAgent::Codex.command(),
            AcpAgentCommand::new(
                command_shim("npx"),
                &["-y", "@agentclientprotocol/codex-acp"]
            )
        );
        assert_eq!(
            AcpAgent::Gemini.command(),
            AcpAgentCommand::new(command_shim("gemini"), &["--acp"])
        );
        assert_eq!(
            AcpAgent::Claude.command(),
            AcpAgentCommand::new(
                command_shim("npx"),
                &["-y", "@agentclientprotocol/claude-agent-acp"]
            )
        );
    }

    /// 検証: ACP_AGENT は大文字小文字と周辺空白を許容する
    /// 理由: 環境変数の入力ゆれで不要に失敗しないようにする
    /// リスク: `Codex` のような自然な指定で設定エラーになる
    #[test]
    fn acp_agent_mapping_is_case_insensitive() {
        assert_eq!(AcpAgent::parse(" Codex ").unwrap(), AcpAgent::Codex);
    }

    /// 検証: ACP_AGENT 未指定または空文字は未設定として扱う
    /// 理由: 暗黙の既定 agent で翻訳せず、ユーザーが明示した時だけ接続する
    /// リスク: 設定忘れなのに意図せず外部 agent へ翻訳内容を送信する
    #[test]
    fn acp_agent_must_be_configured() {
        assert_eq!(acp_agent_from_value(None).unwrap(), None);
        assert_eq!(acp_agent_from_value(Some("")).unwrap(), None);
        assert_eq!(acp_agent_from_value(Some("  ")).unwrap(), None);
        assert_eq!(
            acp_agent_from_value(Some("codex")).unwrap(),
            Some(AcpAgent::Codex)
        );
    }

    #[test]
    fn translate_content_skips_without_agent_value() {
        assert_eq!(
            translate_content_outcome_for_agent(None, "hello").unwrap(),
            TranslateContentOutcome::Skipped
        );
        assert_eq!(
            translate_content_outcome_for_agent(Some("  "), "hello").unwrap(),
            TranslateContentOutcome::Skipped
        );
    }

    #[test]
    fn translate_content_rejects_empty_content() {
        assert!(translate_content_outcome_for_agent(Some("codex"), "  ").is_err());
    }

    /// 検証: 未対応の ACP_AGENT は明示的に拒否する
    /// 理由: 不明な agent 名を shell command として扱わない
    /// リスク: 入力ミスが予期しないプロセス起動につながる
    #[test]
    fn rejects_unknown_acp_agent() {
        assert!(AcpAgent::parse("unknown").is_err());
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
