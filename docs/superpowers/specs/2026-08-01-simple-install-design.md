# 簡単インストール設計

## 目的

Rust toolchain を利用者側に要求せず、利用中の OS と CPU に対応する GitHub Release asset を自動選択して `article-collector` をインストールできるようにする。

## 対象範囲

- POSIX shell 用の `install.sh` を追加する。
- macOS と Linux の amd64 / arm64 を自動判定する。
- GitHub Releases の `latest` asset を一時ファイルへ取得し、成功時だけ `~/.local/bin/article-collector` に配置する。
- README の macOS / Linux 手順を installer の1コマンドに置き換える。
- Windows の既存 PowerShell 手順は変更しない。

## 非対象

- Homebrew、winget、Chocolatey などのパッケージマネージャー対応。
- Windows 用 installer の新規追加。
- checksum / 署名検証の導入。これは別途 release metadata の設計が必要なため対象外とする。

## 動作仕様

`install.sh` は次の順で処理する。

1. `curl`, `uname`, `mkdir`, `mv`, `chmod` の存在を確認する。
2. `uname -s` と `uname -m` を組み合わせ、次の asset に変換する。
   - `Darwin/arm64` または `Darwin/aarch64` → `article-collector-macos-arm64`
   - `Darwin/x86_64` または `Darwin/amd64` → `article-collector-macos-amd64`
   - `Linux/aarch64` または `Linux/arm64` → `article-collector-linux-arm64`
   - `Linux/x86_64` または `Linux/amd64` → `article-collector-linux-amd64`
3. 未対応の組み合わせは asset を取得せず、対応 platform を stderr に表示して終了する。
4. `~/.local/bin` を作成し、一時ファイルへ `curl -fL` で取得する。
5. ダウンロード成功後に実行権限を付け、`mv` で目的地へ移動する。
6. `~/.local/bin` が PATH にない場合の追加例を表示する。

一時ファイルは `mktemp` と `trap` で管理し、ダウンロード失敗時に既存の実行ファイルを変更しない。

## エラー処理

- 必須コマンド不足、未知の OS / CPU、HTTP エラー、ファイル配置エラーは非ゼロ終了する。
- エラーメッセージは stderr に出す。
- 既存インストールの上書きは意図した更新操作として許可するが、更新前に削除は行わない。

## ドキュメント

README では次の導入方法を主手順にする。

```bash
curl -fsSL https://raw.githubusercontent.com/rurusasu/article-collector/main/install.sh | sh
```

Rust が既にある場合の `cargo install` と Windows の手順は代替手順として残す。

## 検証

- `bash -n install.sh`
- `shellcheck install.sh`
- OS / CPU の判定分岐を一時的なコマンド差し替えで検証し、対応 asset と未対応エラーを確認する。
- `git diff --check`
