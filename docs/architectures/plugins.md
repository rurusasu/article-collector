# MCP plugin アーキテクチャ

この文書は、optional recommendation source plugin の trust boundary、MCP
protocol、local state、運用手順を定義する。汎用基盤は GitHub issue
[#84](https://github.com/rurusasu/article-collector/issues/84)、最初の production
plugin である X connector は
[#85](https://github.com/rurusasu/article-collector/issues/85) で追跡する。

## 利用者向け command

plugin は任意機能で、core binary のインストール後に追加できる。

```bash
article-collector plugin install example
article-collector plugin install example@1.2.3
article-collector plugin list
article-collector plugin update example
article-collector plugin configure example
article-collector plugin doctor example
article-collector plugin uninstall example
```

`install` / `update` は binary に埋め込まれた trust set と official registry
URL だけを使う。registry URL override、local executable install、third-party
registry は protocol v1 の対象外である。production trust key は provision
済みだが、最初の plugin が #85 で release されるまでは registry の `plugins`
は空で、install 可能な plugin はない。

`uninstall` は指定した plugin の directory だけを削除する。アプリケーション
からは復元できず、必要になった場合は registry から再 install する。

## recommendation への接続

```text
recommend target / config
  -> built-in と installed manifest の共有 source namespace
  -> source owner を process 起動なしで解決
  -> plugin executable を shell を介さず直接起動
  -> MCP initialize / tools/list
  -> article_collector_describe_sources
  -> signed manifest と runtime descriptor を照合
  -> source options を JSON Schema で検証
  -> article_collector_collect
  -> candidate 件数・URL・title・content・metadata を検証
  -> core が source / site / rank を付与
  -> 既存 history / fetch / translation / save / PR pipeline
```

明示した plugin source の失敗は command failure になる。`recommend all` では
失敗を source failure として記録し、他の enabled source を続行する。
`[recommend].sources` が未指定なら built-in の registry 順、その後に plugin
source 名順で実行する。明示 list がある場合は built-in と plugin を混在でき、
その順序を保つ。

plugin source の設定は `options` 配下へ隔離する。

```toml
[recommend.source.example]
limit = 10

[recommend.source.example.options]
mode = "latest"
```

plugin source は built-in 専用の `query` を拒否し、built-in source は
`options` を拒否する。

## MCP protocol v1

transport は stdio 上の MCP で、stdout は MCP frame 専用、stderr は最大
256 KiB の診断用である。必須 tool は次の 2 つ。

- `article_collector_describe_sources`: protocol/plugin identity、source 名、
  alias、options JSON Schema、configuration 対応有無を返す。
- `article_collector_collect`: canonical source、limit、検証済み options を受け、
  `title`、HTTP(S) `url`、`content`、任意 `metadata` を持つ候補を返す。

`article_collector_configure` は OAuth などを行う任意 tool である。token、
client secret、authorization code を core へ返してはいけない。

`source`、`site`、`rank` は予約 metadata field で、plugin は設定できない。
core は response 順から rank を決める。1 response は 16 MiB、title は
8 KiB、各 content は 1 MiB、件数は request limit 以下に制限する。

initialize request、initialized notification、`tools/list` を含む startup
全体は 10 秒、collect は 60 秒、configure は 300 秒、graceful shutdown は
5 秒で timeout する。plugin は専用 process group（Windows では Job Object）
で起動し、timeout または crash 時は同じ group の descendant も終了する。
許可された全環境変数の value は stderr と bounded JSON-RPC error 診断から
redact する。JSON-RPC error data は core の error/log へ転記しない。

## trust boundary

official registry の署名と curation は publisher identity と配布物の完全性を
保証するが、plugin を無害にはしない。plugin は別 process だが完全な OS sandbox
ではなく、実行ユーザーと同じ OS 権限を持つ。signed manifest で許可された環境
変数だけを child に渡す一方、filesystem や network の OS-level capability は
取り消さない。

executable は plugin data root 配下の regular file に限定し、absolute path、
`..`、symlink/reparse point を拒否する。shell は介さず argument vector で直接
起動する。state から実行 path を解決するときは data root と executable を
canonicalize して containment を再確認し、runtime 起動時にも同じ canonical
path であることを要求する。

これは同一ユーザー権限の hostile process に対する完全な filesystem sandbox
ではない。同じユーザーは plugin data root を変更できるため、その脅威まで
防ぐには将来の OS sandbox と handle-relative/no-follow storage API が必要である。

## local state

OS user data directory は次を基準にする。

| OS | 既定 root |
| --- | --- |
| Linux | `${XDG_DATA_HOME:-$HOME/.local/share}/article-collector` |
| macOS | `~/Library/Application Support/article-collector` |
| Windows | `%LOCALAPPDATA%\article-collector` |

```text
<data-root>/
  registry/
    state.json
  plugins/
    <plugin-name>/
      installed.json
      <version>/
        plugin.toml
        plugin[.exe]
```

`registry/state.json` は最後に受理した registry revision を原子的に記録し、
revision rollback を拒否する。install/update は同一 filesystem 上の
`.staging-*` directory へ download、byte length/SHA-256 検証、MCP probe を
行い、成功後だけ version と `installed.json` を原子的に切り替える。失敗時は
以前の active version を保持する。crash で残った staging directory は
`plugin doctor` が決定的順序で報告する。

source catalog は signed installed manifest だけから namespace を構築し、
executable の存在確認は実行直前へ遅延する。欠損 executable はその plugin
source の failure となるため、無関係な built-in source を停止せず、
`recommend all` では他 source の収集を継続する。

## signed registry と release

registry schema v1 は revision、生成/失効時刻、plugin versions、protocol
compatibility、manifest、5 target の artifact URL/byte length/SHA-256 を持つ。
client も options JSON Schema を compile validation し、registry と artifact
の全 redirect hop を HTTPS に限定する。
`plugins-v1.sig` は JSON の再 serialize 結果ではなく `plugins-v1.json` の
exact bytes へ Ed25519 signature を付ける。

署名 key ID は public key bytes の SHA-256 を
`ed25519-sha256:<lowercase-hex>` として表す。rotation は次の順序で行う。

1. old/new public key の両方を trust する core release を配布する。
2. registry を old/new key の両方で署名する overlap 期間を設ける。
3. overlap core が普及した後に old key を revoked にする。

registry release は必ず artifact-first で行う。

1. 全 target の raw plugin executable を先に公開する。
2. byte length と SHA-256 を確定し registry revision を増やす。
3. expiry を更新し exact bytes を protected environment 内で署名する。
4. local verify 後、`plugin-registry-v1` release へ JSON と signature を同時に
   `--clobber` upload する。

publisher は既発行の JSON/signature を download・検証し、
`max(source revision, published revision) + 1` を採番する。同じ source checkout
から繰り返し実行しても revision を再利用しない。publisher 専用の検証は
真正性と schema/semantic を要求するが freshness は要求しないため、失効後も
より大きい revision と新しい expiry へ復旧できる。publish job は
`plugin-registry-publish` concurrency group で直列化する。

core release と独立した `Plugin Registry Refresh` workflow は毎週月曜
03:17 UTC と手動 dispatch で実行する。安定版 `plugin-registry-v1` release
から download した `plugins-v1.json` と `plugins-v1.sig` を
`verify-published` で認証し、その公開済み JSON だけを refresh の入力にする。
`verify-published` は recovery のため expiry だけを許容し、schema、trust key、
signature は通常どおり検証する。認証後は Rust publisher utility が revision を
`u64` として checked increment し、単一の `generated_at` から正確に 30 日後の
`expires_at` を算出する。revision または expiry が overflow する場合は
fail-closed とする。完全な `plugins` payload を含むその他の値が意味的に同一で
あることを署名前に検証する。

refresh workflow も protected `plugin-registry` environment と
`plugin-registry-publish` concurrency group を共有し、実行中の publisher を
cancel しない。repository の `registry/plugins-v1.json` は core release 時の
publisher だけが入力にでき、独立 refresh は参照しない。stable release または
必須 asset が 0 件・欠損・不正な場合、revision が `u64` でないか increment
できない場合、timestamp が overflow する場合、署名検証が失敗した場合は、
release を作成したり repository 内容へ fallback したりせず fail-closed とする。

PR CI は schema と development fixture signature だけを検証し、production
private key を参照しない。production publisher は `needs: build` の既定の
success 条件に従い、5 target の core build がすべて成功した後だけ実行する。
publisher は protected `plugin-registry` environment で実行し、
`plugin-registry-publish` concurrency group で直列化する。実行中の publish は
cancel しない。concurrency は実行順を保証しないため、publisher は Cargo
metadata から現在の core tag を導出し、GitHub の draft/prerelease ではない
release のうち厳密な `v<major>.<minor>.<patch>` tag だけを `publishedAt` で比較
する。現在の tag が最新の場合だけ refresh/sign と upload を実行し、古い queued
run は成功のまま両 step を skip する。対象 release が 0 件の場合、または最大の
`publishedAt` を持つ core release が複数ある場合は、入力順で選ばず安全側に
失敗する。

### production trust root の provision と recovery

初回だけ、password manager に生成・保存した base64 Ed25519 seed を command の
標準入力へ直接渡し、public trust set を生成する。

macOS では password manager から seed だけを clipboard へ copy し、次の pipe
で標準入力へ渡す。`pbpaste` の終了が EOF になるため、追加入力は不要である。

```bash
pbpaste | cargo run --quiet --locked --example plugin_registry -- \
  provision-trust-set registry/trusted-keys.json
```

seed は command argument、file、shell history、output、log へ書かない。
password manager の値を recovery の正本とし、GitHub の
protected `plugin-registry` environment に置く
`PLUGIN_REGISTRY_SIGNING_KEY` は deployment 用の copy とする。environment
secret を失った場合は password manager から復旧し、public key から private
seed を復元しようとしない。environment の deployment branch は `main` だけに
制限する。

初回 production smoke test では公開済み JSON/signature の署名検証が成功し、
registry が fresh かつ単調増加した revision を持ち、`plugins` が空であることを
確認する。#85 の release までは空の authenticated registry が正常状態である。

独立 refresh の手動 recovery は Actions の `Plugin Registry Refresh` を
default branch で dispatch し、成功後に同じ stable release から JSON と
signature を download して次を確認する。

1. `plugins-v1.json`、`plugins-v1.sig`、`registry/trusted-keys.json` を指定した
   `cargo run --locked --example plugin_registry -- verify` で署名と freshness が
   有効である。
2. revision が実行前より 1 大きく、`generated_at` と `expires_at` が更新されて
   いる。
3. 3 metadata field を除いた JSON が実行前の公開済み JSON と意味的に同一で
   ある。

## 保守と cleanup

- protocol/schema を変更するときは後方互換を暗黙に仮定せず version を上げる。
- plugin release 後も artifact を registry より先に公開する。
- stale `.staging-*` は doctor で対象を確認してから exact plugin directory 内で
  cleanup する。
- #84 の branch/worktree は review・merge・release verification 後に削除する。
- X 固有 OAuth、timeline mapping、既存 X direct collector の置換は #85 専用
  branch で行い、汎用基盤 branch に混ぜない。
