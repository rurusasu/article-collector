use crate::plugins::installer::{validate_descriptor, PluginProbe};
use crate::plugins::types::{
    CollectRequest, CollectResponse, InstalledPlugin, PluginCandidate, PluginDescriptor,
};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use command_group::{AsyncCommandGroup, AsyncGroupChild};
use futures::{SinkExt, StreamExt};
use rmcp::transport::async_rw::JsonRpcMessageCodec;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedRead, FramedWrite};

pub const MAX_MCP_FRAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_TITLE_BYTES: usize = 8 * 1024;
const MAX_CONTENT_BYTES: usize = 1024 * 1024;
const MAX_STDERR_BYTES: usize = 256 * 1024;
const MAX_PLUGIN_DIAGNOSTIC_BYTES: usize = 8 * 1024;
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);
const COLLECT_TIMEOUT: Duration = Duration::from_secs(60);
const CONFIGURE_TIMEOUT: Duration = Duration::from_secs(300);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

const DESCRIBE_TOOL: &str = "article_collector_describe_sources";
const COLLECT_TOOL: &str = "article_collector_collect";
const CONFIGURE_TOOL: &str = "article_collector_configure";

#[derive(Debug, Clone, Default)]
pub struct PluginRuntime;

impl PluginRuntime {
    pub async fn describe(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
        let mut session = McpSession::start(plugin).await?;
        let result = session
            .call_tool(DESCRIBE_TOOL, json!({}), COLLECT_TIMEOUT)
            .await;
        let close_result = session.close().await;
        let descriptor: PluginDescriptor =
            serde_json::from_value(structured_content(&result?)?.clone())
                .context("Invalid structured describe_sources response")?;
        close_result?;
        Ok(descriptor)
    }

    pub async fn collect(
        &self,
        plugin: &InstalledPlugin,
        request: CollectRequest,
    ) -> Result<Vec<PluginCandidate>> {
        let maximum = request.limit;
        let mut session = McpSession::start(plugin).await?;
        let descriptor_result = session
            .call_tool(DESCRIBE_TOOL, json!({}), COLLECT_TIMEOUT)
            .await
            .and_then(|result| {
                serde_json::from_value::<PluginDescriptor>(structured_content(&result)?.clone())
                    .context("Invalid structured describe_sources response")
            })
            .and_then(|descriptor| {
                validate_descriptor(&plugin.manifest, &descriptor)?;
                Ok(descriptor)
            });
        if let Err(error) = descriptor_result {
            let _ = session.close().await;
            return Err(error);
        }

        let arguments =
            serde_json::to_value(request).context("Failed to encode collect request")?;
        let result = session
            .call_tool(COLLECT_TOOL, arguments, COLLECT_TIMEOUT)
            .await;
        let close_result = session.close().await;
        let response: CollectResponse =
            serde_json::from_value(structured_content(&result?)?.clone())
                .context("Invalid structured collect response")?;
        validate_collect_response(maximum, &response)?;
        close_result?;
        Ok(response.items)
    }

    pub async fn configure(&self, plugin: &InstalledPlugin) -> Result<Value> {
        if !plugin.manifest.supports_configuration {
            bail!("Plugin '{}' does not support configuration", plugin.name);
        }
        let mut session = McpSession::start(plugin).await?;
        let result = session
            .call_tool(CONFIGURE_TOOL, json!({}), CONFIGURE_TIMEOUT)
            .await;
        let close_result = session.close().await;
        let value = structured_content(&result?)?.clone();
        reject_secret_fields(&value)?;
        close_result?;
        Ok(value)
    }

    pub async fn doctor(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
        let descriptor = self.describe(plugin).await?;
        validate_descriptor(&plugin.manifest, &descriptor)?;
        Ok(descriptor)
    }
}

#[async_trait]
impl PluginProbe for PluginRuntime {
    async fn verify(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
        self.describe(plugin).await
    }
}

struct McpSession {
    child: AsyncGroupChild,
    reader: FramedRead<ChildStdout, JsonRpcMessageCodec<Value>>,
    writer: FramedWrite<ChildStdin, JsonRpcMessageCodec<Value>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    stderr_redactions: Vec<String>,
    stderr_task: Option<JoinHandle<()>>,
    next_id: u64,
}

impl McpSession {
    async fn start(plugin: &InstalledPlugin) -> Result<Self> {
        if !plugin.executable.is_absolute() {
            bail!(
                "Plugin executable must be an absolute canonical path: {}",
                plugin.executable.display()
            );
        }
        let executable = plugin.executable.canonicalize().with_context(|| {
            format!(
                "Failed to resolve plugin executable {}",
                plugin.executable.display()
            )
        })?;
        if !executable.is_file() {
            bail!("Plugin executable is not a file: {}", executable.display());
        }
        if executable != plugin.executable {
            bail!(
                "Plugin executable changed after safe path resolution: {}",
                plugin.executable.display()
            );
        }
        let mut command = Command::new(&executable);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env_clear();
        copy_allowed_environment(&mut command, &plugin.manifest.environment)?;
        let mut child = command
            .group_spawn()
            .with_context(|| format!("Failed to start plugin {}", plugin.name))?;
        let stdin = child
            .inner()
            .stdin
            .take()
            .context("Plugin stdin is unavailable")?;
        let stdout = child
            .inner()
            .stdout
            .take()
            .context("Plugin stdout is unavailable")?;
        let stderr = child
            .inner()
            .stderr
            .take()
            .context("Plugin stderr is unavailable")?;
        let stderr_buffer = Arc::new(Mutex::new(Vec::new()));
        let stderr_task = tokio::spawn(capture_stderr(stderr, stderr_buffer.clone()));
        let stderr_redactions = allowed_environment_values(&plugin.manifest.environment);

        let mut session = Self {
            child,
            reader: FramedRead::new(
                stdout,
                JsonRpcMessageCodec::new_with_max_length(MAX_MCP_FRAME_BYTES),
            ),
            writer: FramedWrite::new(
                stdin,
                JsonRpcMessageCodec::new_with_max_length(MAX_MCP_FRAME_BYTES),
            ),
            stderr: stderr_buffer,
            stderr_redactions,
            stderr_task: Some(stderr_task),
            next_id: 1,
        };
        let startup = tokio::time::timeout(INITIALIZE_TIMEOUT, session.initialize()).await;
        match startup {
            Ok(Ok(())) => Ok(session),
            Ok(Err(error)) => {
                let _ = session.terminate().await;
                Err(error)
            }
            Err(_) => {
                let _ = session.terminate().await;
                bail!("Plugin MCP startup timed out");
            }
        }
    }

    async fn initialize(&mut self) -> Result<()> {
        let initialize = json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {
                "name": "article-collector",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
        self.request("initialize", initialize).await?;
        self.writer
            .send(json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .await
            .context("Failed to send MCP initialized notification")?;
        let tools = self
            .request("tools/list", json!({}))
            .await
            .context("Failed to list plugin MCP tools")?;
        let names = tools
            .get("tools")
            .and_then(Value::as_array)
            .context("MCP tools/list response has no tools array")?
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str));
        validate_required_tools(names)?;
        Ok(())
    }

    async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        tokio::time::timeout(
            timeout,
            self.request(
                "tools/call",
                json!({
                    "name": name,
                    "arguments": arguments
                }),
            ),
        )
        .await
        .with_context(|| format!("Plugin MCP tool '{name}' timed out"))?
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.writer
            .send(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            }))
            .await
            .with_context(|| format!("Failed to send MCP request '{method}'"))?;
        while let Some(frame) = self.reader.next().await {
            let frame = frame
                .with_context(|| format!("Invalid or oversized MCP response for '{method}'"))?;
            if frame.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = frame.get("error") {
                let diagnostic = sanitize_json_rpc_error(error, &self.stderr_redactions);
                bail!("Plugin MCP request '{method}' failed: {diagnostic}");
            }
            return frame
                .get("result")
                .cloned()
                .with_context(|| format!("MCP response for '{method}' has no result"));
        }
        self.wait_for_stderr().await;
        let stderr = sanitized_stderr(&self.stderr, &self.stderr_redactions).await;
        bail!("Plugin exited before responding to '{method}'{stderr}")
    }

    async fn close(mut self) -> Result<()> {
        self.writer
            .close()
            .await
            .context("Failed to close plugin stdin")?;
        match tokio::time::timeout(SHUTDOWN_TIMEOUT, self.child.wait()).await {
            Ok(status) => {
                let status = status.context("Failed to wait for plugin process")?;
                self.wait_for_stderr().await;
                if !status.success() {
                    let stderr = sanitized_stderr(&self.stderr, &self.stderr_redactions).await;
                    bail!("Plugin exited abnormally with {status}{stderr}");
                }
            }
            Err(_) => {
                self.terminate().await?;
                self.wait_for_stderr().await;
            }
        }
        Ok(())
    }

    async fn wait_for_stderr(&mut self) {
        let Some(mut task) = self.stderr_task.take() else {
            return;
        };
        if tokio::time::timeout(SHUTDOWN_TIMEOUT, &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }

    async fn terminate(&mut self) -> Result<()> {
        match self.child.kill().await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
            Err(error) => Err(error).context("Failed to kill plugin process group"),
        }
    }
}

pub fn validate_collect_response(maximum: usize, response: &CollectResponse) -> Result<()> {
    if response.items.len() > maximum {
        bail!(
            "Plugin returned more items ({}) than requested ({maximum})",
            response.items.len()
        );
    }
    for (index, item) in response.items.iter().enumerate() {
        if !matches!(item.url.scheme(), "http" | "https") {
            bail!("Plugin candidate {index} URL must use HTTP or HTTPS");
        }
        if item.title.len() > MAX_TITLE_BYTES {
            bail!("Plugin candidate {index} title exceeds {MAX_TITLE_BYTES} bytes");
        }
        if item.content.len() > MAX_CONTENT_BYTES {
            bail!("Plugin candidate {index} content exceeds {MAX_CONTENT_BYTES} bytes");
        }
        for field in ["source", "site", "rank"] {
            if item.metadata.contains_key(field) {
                bail!("Plugin candidate contains reserved metadata field '{field}'");
            }
        }
    }
    Ok(())
}

pub fn validate_required_tools<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let names = names.into_iter().collect::<HashSet<_>>();
    for required in [DESCRIBE_TOOL, COLLECT_TOOL] {
        if !names.contains(required) {
            bail!("Plugin MCP server is missing required tool '{required}'");
        }
    }
    Ok(())
}

pub fn structured_content(result: &Value) -> Result<&Value> {
    result
        .get("structuredContent")
        .context("Plugin MCP tool response must include structuredContent")
}

fn copy_allowed_environment(command: &mut Command, allowed: &[String]) -> Result<()> {
    #[cfg(windows)]
    if let Some(value) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", value);
    }
    for name in allowed {
        if !valid_environment_name(name) {
            bail!("Invalid plugin environment variable name '{name}'");
        }
        if matches!(
            name.as_str(),
            "LD_PRELOAD" | "LD_LIBRARY_PATH" | "DYLD_INSERT_LIBRARIES" | "DYLD_LIBRARY_PATH"
        ) {
            bail!("Plugin environment variable '{name}' is not allowed");
        }
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    Ok(())
}

fn valid_environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first == b'_' || first.is_ascii_uppercase())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

async fn capture_stderr(mut stderr: tokio::process::ChildStderr, buffer: Arc<Mutex<Vec<u8>>>) {
    let mut chunk = [0_u8; 8192];
    while let Ok(count) = stderr.read(&mut chunk).await {
        if count == 0 {
            break;
        }
        let mut buffer = buffer.lock().await;
        buffer.extend_from_slice(&chunk[..count]);
        if buffer.len() > MAX_STDERR_BYTES {
            let excess = buffer.len() - MAX_STDERR_BYTES;
            buffer.drain(..excess);
        }
    }
}

async fn sanitized_stderr(buffer: &Arc<Mutex<Vec<u8>>>, secrets: &[String]) -> String {
    let buffer = buffer.lock().await;
    if buffer.is_empty() {
        String::new()
    } else {
        format!("; stderr tail: {}", sanitize_stderr_bytes(&buffer, secrets))
    }
}

fn sanitize_stderr_bytes(bytes: &[u8], secrets: &[String]) -> String {
    let mut output = String::from_utf8_lossy(bytes).replace(['\r', '\n'], " ");
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        output = output.replace(secret, "[REDACTED]");
    }
    output
}

fn allowed_environment_values(allowed: &[String]) -> Vec<String> {
    allowed
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .filter(|value| !value.is_empty())
        .collect()
}

fn sanitize_json_rpc_error(error: &Value, secrets: &[String]) -> String {
    let code = error
        .get("code")
        .and_then(Value::as_i64)
        .map_or_else(|| "unknown".to_string(), |code| code.to_string());
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("plugin returned an unspecified error");
    let sanitized = sanitize_stderr_bytes(message.as_bytes(), secrets);
    let bounded = truncate_utf8(&sanitized, MAX_PLUGIN_DIAGNOSTIC_BYTES);
    format!("code {code}: {bounded}")
}

fn truncate_utf8(value: &str, maximum_bytes: usize) -> &str {
    if value.len() <= maximum_bytes {
        return value;
    }
    let mut end = maximum_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn reject_secret_fields(value: &Value) -> Result<()> {
    const SECRET_PARTS: [&str; 4] = ["token", "secret", "authorization", "code"];
    if let Some(object) = value.as_object() {
        for (key, nested) in object {
            let lowercase = key.to_ascii_lowercase();
            if SECRET_PARTS.iter().any(|part| lowercase.contains(part)) {
                bail!("Plugin configure response contains forbidden secret-like field '{key}'");
            }
            reject_secret_fields(nested)?;
        }
    } else if let Some(array) = value.as_array() {
        for nested in array {
            reject_secret_fields(nested)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::types::{
        CollectResponse, InstalledPlugin, ManifestSource, PluginCandidate, PluginManifest,
    };
    use rmcp::transport::async_rw::JsonRpcMessageCodec;
    use semver::{Version, VersionReq};
    use serde_json::{json, Map, Value};
    use tokio_util::bytes::BytesMut;
    use tokio_util::codec::Decoder;
    use url::Url;

    fn candidate() -> PluginCandidate {
        PluginCandidate {
            title: "candidate".to_string(),
            url: Url::parse("https://example.com/article").unwrap(),
            content: "body".to_string(),
            metadata: Map::new(),
        }
    }

    /// 検証: plugin metadataはcore所有fieldを偽装できない
    /// 理由: source/site/rankの所有権をcoreに限定するため
    /// リスク: pluginが履歴・表示上のsource attributionを書き換える
    #[test]
    fn collect_rejects_reserved_metadata() {
        for field in ["source", "site", "rank"] {
            let mut item = candidate();
            item.metadata.insert(field.to_string(), json!("spoofed"));
            let error =
                validate_collect_response(1, &CollectResponse { items: vec![item] }).unwrap_err();
            assert!(error
                .to_string()
                .contains(&format!("reserved metadata field '{field}'")));
        }
    }

    /// 検証: response件数・URL scheme・title/content byte上限を強制する
    /// 理由: plugin processを信頼境界外としてメモリと下流inputを制限するため
    /// リスク: 過大responseや危険schemeが既存pipelineへ入る
    #[test]
    fn collect_rejects_semantically_invalid_candidates() {
        let too_many = CollectResponse {
            items: vec![candidate(), candidate()],
        };
        assert!(validate_collect_response(1, &too_many)
            .unwrap_err()
            .to_string()
            .contains("more items"));

        let mut invalid_url = candidate();
        invalid_url.url = Url::parse("ftp://example.com/file").unwrap();
        assert!(validate_collect_response(
            1,
            &CollectResponse {
                items: vec![invalid_url]
            }
        )
        .unwrap_err()
        .to_string()
        .contains("HTTP"));

        let mut long_title = candidate();
        long_title.title = "t".repeat(8 * 1024 + 1);
        assert!(validate_collect_response(
            1,
            &CollectResponse {
                items: vec![long_title]
            }
        )
        .unwrap_err()
        .to_string()
        .contains("title"));

        let mut long_content = candidate();
        long_content.content = "c".repeat(1024 * 1024 + 1);
        assert!(validate_collect_response(
            1,
            &CollectResponse {
                items: vec![long_content]
            }
        )
        .unwrap_err()
        .to_string()
        .contains("content"));
    }

    /// 検証: MCP codecは16MiBを超えるnewlineなしframeを拒否する
    /// 理由: child stdoutでunbounded allocationを起こさないため
    /// リスク: 悪意あるpluginがcoreをOOMにする
    #[test]
    fn mcp_codec_rejects_oversized_frame() {
        let mut codec = JsonRpcMessageCodec::<Value>::new_with_max_length(MAX_MCP_FRAME_BYTES);
        let mut bytes = BytesMut::from(&vec![b'a'; MAX_MCP_FRAME_BYTES + 1][..]);

        let error = codec.decode(&mut bytes).unwrap_err();

        assert!(error.to_string().contains("max line length"));
    }

    #[test]
    fn requires_describe_and_collect_tools_and_structured_content() {
        assert!(
            validate_required_tools(["article_collector_describe_sources"])
                .unwrap_err()
                .to_string()
                .contains("article_collector_collect")
        );
        let error =
            structured_content(&json!({"content": [{"type": "text", "text": "{}"}]})).unwrap_err();
        assert!(error.to_string().contains("structuredContent"));
    }

    #[cfg(unix)]
    fn fixture_plugin(script: &str) -> (tempfile::TempDir, InstalledPlugin) {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("plugin");
        std::fs::write(&executable, format!("#!/bin/sh\nset -eu\n{script}\n")).unwrap();
        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&executable, permissions).unwrap();
        let plugin = InstalledPlugin {
            schema_version: 1,
            registry_revision: 1,
            name: "example".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            executable: executable.canonicalize().unwrap(),
            manifest: PluginManifest {
                name: "example".to_string(),
                version: Version::parse("1.0.0").unwrap(),
                protocol: VersionReq::parse("^1.0").unwrap(),
                capabilities: vec!["recommend-source".to_string()],
                sources: vec![ManifestSource {
                    name: "example".to_string(),
                    aliases: vec![],
                    options_schema: json!({
                        "type": "object",
                        "additionalProperties": false
                    }),
                }],
                supports_configuration: false,
                environment: vec![],
            },
        };
        (temp, plugin)
    }

    #[cfg(unix)]
    fn handshake_script(tool_names: &[&str], call_body: Option<&str>) -> String {
        let tools = tool_names
            .iter()
            .map(|name| format!(r#"{{"name":"{name}","inputSchema":{{"type":"object"}}}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let mut script = format!(
            r#"
IFS= read -r initialize
printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":"2025-06-18","capabilities":{{}},"serverInfo":{{"name":"fixture","version":"1"}}}}}}'
IFS= read -r initialized
IFS= read -r list_tools
printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"tools":[{tools}]}}}}'
"#
        );
        if let Some(call_body) = call_body {
            script.push_str("IFS= read -r call_tool\n");
            script.push_str(&format!(
                "printf '%s\\n' '{}'\n",
                call_body.replace('\'', "'\\''")
            ));
        }
        script
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn describe_completes_real_mcp_stdio_handshake() {
        let body = r#"{"jsonrpc":"2.0","id":3,"result":{"structuredContent":{"protocol_version":"1.0.0","name":"example","version":"1.0.0","sources":[{"name":"example","aliases":[],"options_schema":{"type":"object","additionalProperties":false}}],"supports_configuration":false}}}"#;
        let script = handshake_script(&[DESCRIBE_TOOL, COLLECT_TOOL], Some(body));
        let (_temp, plugin) = fixture_plugin(&script);

        let descriptor = PluginRuntime.describe(&plugin).await.unwrap();

        assert_eq!(descriptor.name, "example");
        assert_eq!(descriptor.version, Version::parse("1.0.0").unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn missing_required_tool_is_rejected_after_initialize() {
        let script = handshake_script(&[DESCRIBE_TOOL], None);
        let (_temp, plugin) = fixture_plugin(&script);

        let error = PluginRuntime.describe(&plugin).await.unwrap_err();

        assert!(error.to_string().contains(COLLECT_TOOL));
    }

    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    async fn initialize_timeout_terminates_hanging_plugin() {
        let (_temp, plugin) = fixture_plugin("IFS= read -r initialize\nsleep 60");

        let error = PluginRuntime.describe(&plugin).await.unwrap_err();

        assert!(error.to_string().contains("startup timed out"));
    }

    /// 検証: initialize応答後にtools/listが停止してもstartup deadlineで終了する
    /// 理由: required tool discoveryもMCP startup lifecycleの一部だから
    /// リスク: install/doctor/recommendが無期限に停止する
    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    async fn tools_list_timeout_terminates_hanging_plugin() {
        let script = r#"
IFS= read -r initialize
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}}}'
IFS= read -r initialized
IFS= read -r list_tools
sleep 60
"#;
        let (_temp, plugin) = fixture_plugin(script);

        let error = PluginRuntime.describe(&plugin).await.unwrap_err();

        assert!(error.to_string().contains("startup timed out"));
    }

    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    async fn collect_timeout_terminates_hanging_plugin() {
        let mut script = handshake_script(&[DESCRIBE_TOOL, COLLECT_TOOL], None);
        script.push_str("IFS= read -r call_tool\nsleep 600\n");
        let (_temp, plugin) = fixture_plugin(&script);
        let request = crate::plugins::types::CollectRequest {
            source: "example".to_string(),
            limit: 1,
            options: Map::new(),
        };

        let error = PluginRuntime.collect(&plugin, request).await.unwrap_err();

        assert!(error.to_string().contains("timed out"));
    }

    /// 検証: collect 前に runtime descriptor を signed manifest と照合する
    /// 理由: install 後に executable の振る舞いが変わっても source contract を再確認するため
    /// リスク: manifest と異なる source/options contract で候補収集が実行される
    #[cfg(unix)]
    #[tokio::test]
    async fn collect_rejects_runtime_descriptor_mismatch_before_collection() {
        let body = r#"{"jsonrpc":"2.0","id":3,"result":{"structuredContent":{"protocol_version":"1.0.0","name":"different","version":"1.0.0","sources":[{"name":"example","aliases":[],"options_schema":{"type":"object","additionalProperties":false}}],"supports_configuration":false}}}"#;
        let script = handshake_script(&[DESCRIBE_TOOL, COLLECT_TOOL], Some(body));
        let (_temp, plugin) = fixture_plugin(&script);
        let request = crate::plugins::types::CollectRequest {
            source: "example".to_string(),
            limit: 1,
            options: Map::new(),
        };

        let error = PluginRuntime.collect(&plugin, request).await.unwrap_err();

        assert!(error.to_string().contains("signed manifest"));
    }

    /// 検証: descriptor 照合後に同じ MCP session で collect を呼び出す
    /// 理由: source contract 照合と候補取得の間で別 process に切り替わらないようにするため
    /// リスク:正しい plugin でも追加 handshake に追従できず収集不能になる
    #[cfg(unix)]
    #[tokio::test]
    async fn collect_describes_then_collects_in_one_mcp_session() {
        let descriptor = r#"{"jsonrpc":"2.0","id":3,"result":{"structuredContent":{"protocol_version":"1.0.0","name":"example","version":"1.0.0","sources":[{"name":"example","aliases":[],"options_schema":{"type":"object","additionalProperties":false}}],"supports_configuration":false}}}"#;
        let candidates = r#"{"jsonrpc":"2.0","id":4,"result":{"structuredContent":{"items":[{"title":"Example","url":"https://example.com/article","content":"Body","metadata":{}}]}}}"#;
        let mut script = handshake_script(&[DESCRIBE_TOOL, COLLECT_TOOL], None);
        script.push_str("IFS= read -r describe\n");
        script.push_str(&format!("printf '%s\\n' '{descriptor}'\n"));
        script.push_str("IFS= read -r collect\n");
        script.push_str(&format!("printf '%s\\n' '{candidates}'\n"));
        let (_temp, plugin) = fixture_plugin(&script);
        let request = crate::plugins::types::CollectRequest {
            source: "example".to_string(),
            limit: 1,
            options: Map::new(),
        };

        let items = PluginRuntime.collect(&plugin, request).await.unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Example");
    }

    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    async fn configure_timeout_terminates_hanging_plugin() {
        let mut script = handshake_script(&[DESCRIBE_TOOL, COLLECT_TOOL, CONFIGURE_TOOL], None);
        script.push_str("IFS= read -r call_tool\nsleep 600\n");
        let (_temp, mut plugin) = fixture_plugin(&script);
        plugin.manifest.supports_configuration = true;

        let error = PluginRuntime.configure(&plugin).await.unwrap_err();

        assert!(error.to_string().contains("timed out"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn abnormal_exit_reports_bounded_stderr_tail() {
        let script = r#"
head -c 300000 /dev/zero | tr '\000' 'x' >&2
printf '\nfixture-crash\n' >&2
exit 9
"#;
        let (_temp, plugin) = fixture_plugin(script);

        let error = PluginRuntime.describe(&plugin).await.unwrap_err();
        let message = error.to_string();

        assert!(message.contains("fixture-crash"));
        assert!(message.len() <= MAX_STDERR_BYTES + 1024);
    }

    #[test]
    fn stderr_sanitization_redacts_secret_values() {
        let output = sanitize_stderr_bytes(
            b"request failed token=very-secret-value",
            &["very-secret-value".to_string()],
        );

        assert!(!output.contains("very-secret-value"));
        assert!(output.contains("[REDACTED]"));
    }

    /// 検証: plugin制御のJSON-RPC error dataを診断へ出さずmessageもbounded/redactedにする
    /// 理由: error chainはrecommend allのstructured logへ永続化されるため
    /// リスク: credential漏えいと最大frame相当のlog膨張
    #[test]
    fn json_rpc_error_diagnostic_is_bounded_and_redacted() {
        let secret = "cookie-value";
        let error = json!({
            "code": -32000,
            "message": format!("{secret}{}", "x".repeat(32 * 1024)),
            "data": {"credential": secret}
        });

        let output = sanitize_json_rpc_error(&error, &[secret.to_string()]);

        assert!(output.contains("code -32000"));
        assert!(!output.contains(secret));
        assert!(!output.contains("credential"));
        assert!(output.len() <= MAX_PLUGIN_DIAGNOSTIC_BYTES + 128);
    }

    /// 検証: timeout cleanupがpluginから起動された同一process-groupの子も終了する
    /// 理由:直系processだけのkillではresource handleを持つ孫processが残るため
    /// リスク:繰り返しtimeoutでorphan processが蓄積する
    #[cfg(unix)]
    #[tokio::test]
    async fn terminate_process_tree_kills_descendants() {
        let temp = tempfile::tempdir().unwrap();
        let pid_file = temp.path().join("descendant.pid");
        let script = format!("sleep 60 & echo $! > '{}'; wait", pid_file.display());
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(script);
        let mut child = command.group_spawn().unwrap();
        for _ in 0..100 {
            if pid_file.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let descendant = std::fs::read_to_string(&pid_file).unwrap();

        child.kill().await.unwrap();

        let status = std::process::Command::new("kill")
            .args(["-0", descendant.trim()])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "descendant process remained alive");
    }
}
