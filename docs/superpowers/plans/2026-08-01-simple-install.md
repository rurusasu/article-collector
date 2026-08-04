# 簡単インストーラー Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** macOS / Linux の環境を自動判定し、GitHub Releases から `article-collector` を1コマンドでインストールできるようにする。

**Architecture:** `install.sh` は POSIX shell の単一 installer とし、`uname` で release asset を選び、`curl` の取得先を一時ファイルにして成功後にだけ `~/.local/bin` へ移動する。判定とダウンロードは外部コマンドを PATH 差し替えできるシェルテストから検証し、README には installer の利用方法を主手順として記載する。

**Tech Stack:** POSIX shell, curl, GitHub Releases, shellcheck, Bash test harness, Markdown

## Global Constraints

- 対応 platform は macOS / Linux の amd64 / arm64 とする。
- GitHub Releases の `latest` asset を利用し、Rust toolchain を要求しない。
- ダウンロード失敗時は既存の `article-collector` を変更しない。
- Windows 用の既存 PowerShell 手順は変更しない。
- 実行時に必要な外部コマンド不足・未対応 platform・HTTP エラーは非ゼロ終了する。

---

### Task 1: Installer の失敗する受け入れテストを作る

**Files:**
- Create: `tests/install.sh`
- Test: `tests/install.sh`

**Interfaces:**
- Consumes: repository root の `install.sh`（この task の時点では未作成）
- Produces: fake `uname` / `curl` を使った installer の black-box test harness

- [x] **Step 1: 対応 platform のインストールテストを書く**

テスト用一時ディレクトリに fake command を作り、`uname -s` / `uname -m` が Darwin / arm64 を返す環境で、fake `curl` が受け取った URL と出力先へ書き込む内容を検証する。インストール先は `HOME` 配下の一時ディレクトリに限定する。

- [x] **Step 2: Linux arm64 の asset 選択テストを書く**

同じ harness を再利用し、Linux / aarch64 が `article-collector-linux-arm64` を選ぶことを検証する。

- [x] **Step 3: 未対応 platform の失敗テストを書く**

FreeBSD / x86_64 などの入力では `curl` を呼ばず、非ゼロ終了することを検証する。

- [x] **Step 4: ダウンロード失敗時の非破壊テストを書く**

既存のインストール先に sentinel 内容を置き、fake `curl` が失敗した場合に installer が非ゼロ終了し、sentinel が変わらないことを検証する。

- [x] **Step 5: テストが期待どおり失敗することを確認する**

Run: `bash tests/install.sh`

Expected: `install.sh` がまだ存在しないため、対応 platform テストが失敗する。

### Task 2: 自動判定 installer を実装する

**Files:**
- Create: `install.sh`
- Test: `tests/install.sh`
- Modify: `Taskfile.yml`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `uname -s`, `uname -m`, GitHub Releases URL
- Produces: 実行可能な `~/.local/bin/article-collector`

- [x] **Step 1: 必須コマンド確認と platform 変換を実装する**

`command -v` で `curl`, `uname`, `mkdir`, `mktemp`, `mv`, `chmod` を確認し、次の asset 名へ変換する。

```sh
Darwin/arm64    -> article-collector-macos-arm64
Darwin/aarch64  -> article-collector-macos-arm64
Darwin/x86_64   -> article-collector-macos-amd64
Darwin/amd64    -> article-collector-macos-amd64
Linux/aarch64   -> article-collector-linux-arm64
Linux/arm64     -> article-collector-linux-arm64
Linux/x86_64    -> article-collector-linux-amd64
Linux/amd64     -> article-collector-linux-amd64
```

- [x] **Step 2: 一時ファイル経由の取得と配置を実装する**

`mktemp` で一時ファイルを作り、`trap` で終了時に削除する。`curl -fL` が成功した後に `chmod +x` と `mv` を実行し、失敗時には目的地を変更しない。

- [x] **Step 3: テストを実行して green を確認する**

Run: `bash tests/install.sh`

Expected: 対応 platform、未対応 platform、ダウンロード失敗時の非破壊テストがすべて PASS。

- [ ] **Step 4: installer を shellcheck で検証する**

Run: `shellcheck install.sh tests/install.sh`

Expected: 警告・エラーなし。現在の開発環境では `shellcheck` 未導入のため未実行。

### Task 3: README の導入手順を更新する

**Files:**
- Modify: `README.md:9-58`

**Interfaces:**
- Consumes: `install.sh` の raw GitHub URL と install directory の仕様
- Produces: 初回利用者向けの1コマンド導入手順、Windows / Cargo fallback の説明

- [x] **Step 1: macOS / Linux の主手順を installer 1行に置き換える**

次のコマンドを主手順に掲載する。

```bash
curl -fsSL https://raw.githubusercontent.com/rurusasu/article-collector/main/install.sh | sh
```

- [x] **Step 2: PATH と fallback の説明を残す**

`~/.local/bin` が PATH にない場合の `export PATH="$HOME/.local/bin:$PATH"` を示し、Windows の PowerShell 手順と `cargo install` を代替手順として残す。

- [x] **Step 3: README のリンク・コマンドを確認する**

Run: `git diff --check`

Expected: whitespace error なし。installer URL が `main/install.sh` を指し、README の asset 名と release workflow の asset 名が一致する。

### Task 4: 最終検証と変更確認

**Files:**
- Modify: none
- Test: `install.sh`, `tests/install.sh`, `README.md`

- [x] **Step 1: shell syntax を検証する**

Run: `bash -n install.sh tests/install.sh`

Expected: exit code 0。

- [x] **Step 2: installer test を再実行する**

Run: `bash tests/install.sh`

Expected: 全テスト PASS。

- [x] **Step 3: Rust プロジェクトの既存検証を実行する**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked`

Expected: 既存 Rust コードに失敗なし。

- [x] **Step 4: 最終 diff と status を確認する**

Run: `git diff --check && git status --short && git diff --stat HEAD`

Expected: 意図した installer、test、README、docs のみが変更対象として表示される。
