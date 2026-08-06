# Plugin secrets

pluginのOAuth credential、runtime secret、registry署名鍵の取り扱いを定義する。
MCP protocol、plugin lifecycleの仕様は[README.md](README.md)、registry schemaと
配布workflowの仕様は[distribution.md](distribution.md)を参照する。

## X OAuth credential

| credential | 取り扱い |
| --- | --- |
| `X_OAUTH_CLIENT_ID` | X Developer Platformのpublic OAuth 2.0 PKCE client ID。secretではなく、`plugin configure x`の前に環境変数へ設定する。 |
| client secret | X pluginでは使用しない。 |
| authorization code | plugin内でOAuth交換にだけ使用し、coreへ返さない。 |
| access token / refresh token | plugin専用のuser-data storageへ保存し、coreのresponse、log、MCP errorへ返さない。 |
| OAuth scope | read-only scopeに限定する。 |

`article_collector_configure` はOAuthなどを行う任意toolである。token、client
secret、authorization codeをcoreへ返してはいけない。

## X OAuth setup

| 項目 | 値 |
| --- | --- |
| callback URL | `http://127.0.0.1:53681/callback`。X Developer Platformにも登録する。 |
| authorization | `plugin configure x`が表示するauthorization URLを開き、read-only権限で同意する。 |
| scope | `tweet.read users.read bookmark.read offline.access`。write scopeは要求しない。 |

## Runtime secret handling

| 対象 | ルール |
| --- | --- |
| childへ渡す環境変数 | signed manifestで許可された環境変数だけを渡す。WindowsではOS起動に必要な`SystemRoot`を追加で渡す。 |
| stderr | 許可された環境変数のvalueをredactする。stderrはbounded diagnostic outputである。 |
| JSON-RPC error | bounded error診断から環境変数のvalueをredactし、`data`をcoreのerror/logへ転記しない。 |
| plugin process | coreとは別processだが、完全なOS sandboxではない。filesystemとnetworkのOS-level capabilityは取り消さない。 |

## Registry signing secret

| 項目 | ルール |
| --- | --- |
| seedの正本 | password managerに生成・保存したbase64 Ed25519 seed。 |
| 入力経路 | commandの標準入力へ直接渡す。macOSではclipboardから`pbpaste`でpipeする。 |
| public trust set | public keyだけを`registry/trusted-keys.json`へ生成する。 |
| GitHub secret | `PLUGIN_REGISTRY_SIGNING_KEY`はprotected `plugin-registry` environmentに置くdeployment用copy。deployment branchは`main`だけに制限する。 |
| recovery | password managerを正本とし、public keyからprivate seedを復元しない。 |
| 禁止事項 | seedをcommand argument、file、shell history、output、logへ書かない。 |

```bash
pbpaste | cargo run --quiet --locked --example plugin_registry -- \
  provision-trust-set registry/trusted-keys.json
```

registryの署名処理では、private seedをrepository、registry JSON、signature
assetへ保存しない。公開されるのはpublic trust setと署名済みregistryだけである。
