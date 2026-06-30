# Push Timeout Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Treat a timed-out target repo `git push` as successful only when the remote branch is already observable.

**Architecture:** Keep the change inside `src/target_repos.rs`. Add a push-specific helper that calls the existing timed command runner, then checks `git ls-remote --exit-code --heads origin <branch>` only when the push error is the known timeout shape.

**Tech Stack:** Rust stable, anyhow, std::process, existing `target_repos` unit tests.

---

### Task 1: Plane Tracking

**Files:**
- No repo files.
- Plane project: `ACS`

- [x] **Step 1: Create Plane issue before implementation**

Create ACS-96 with purpose, scope, work units, acceptance criteria, verification commands, and release/cleanup notes.

Expected: issue is `In Progress` before any code implementation starts.

### Task 2: Push Timeout Regression Test

**Files:**
- Modify: `src/target_repos.rs`

- [ ] **Step 1: Write the failing test**

Add a unit test in `target_repos::tests`:

```rust
#[test]
fn push_timeout_is_recovered_when_remote_branch_exists() {
    let _guard = ENV_LOCK.lock().unwrap();
    let sandbox = normalized_temp_path("target-repo-push-timeout-recovered");
    let target_dir = sandbox.join("target-repo");
    let bin_dir = sandbox.join("bin");
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&target_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_git_push_timeout(&bin_dir);

    let previous_path = std::env::var_os("PATH");
    std::env::set_var("PATH", prepend_path(&bin_dir, previous_path.as_ref()));

    let result = push_branch(&target_dir, "article/recovered");

    assert!(result.is_ok(), "push timeout should be recovered: {result:#?}");

    restore_env("PATH", previous_path);
    let _ = fs::remove_dir_all(sandbox);
}
```

- [ ] **Step 2: Run RED verification**

Run:

```powershell
cargo test --locked target_repos::tests::push_timeout_is_recovered_when_remote_branch_exists
```

Expected: FAIL because `push_branch` or the recovery behavior does not exist yet.

### Task 3: Minimal Push Recovery

**Files:**
- Modify: `src/target_repos.rs`

- [ ] **Step 1: Add narrow helper functions**

Add helpers near the command wrappers:

```rust
fn push_branch(target_dir: &Path, branch: &str) -> Result<()> {
    let result = run_git(target_dir, &["push", "-u", "origin", branch]);
    if result.is_ok() {
        return Ok(());
    }

    let error = result.unwrap_err();
    if !is_timeout_error(&error) || !remote_branch_exists(target_dir, branch) {
        return Err(error);
    }

    Ok(())
}

fn remote_branch_exists(target_dir: &Path, branch: &str) -> bool {
    run_git(
        target_dir,
        &["ls-remote", "--exit-code", "--heads", "origin", branch],
    )
    .is_ok()
}

fn is_timeout_error(error: &anyhow::Error) -> bool {
    error.to_string().contains("timed out after")
}
```

- [ ] **Step 2: Route PR creation through the helper**

Replace:

```rust
run_git(target_dir, &["push", "-u", "origin", &branch])?;
```

with:

```rust
push_branch(target_dir, &branch)?;
```

- [ ] **Step 3: Run GREEN verification**

Run:

```powershell
cargo test --locked target_repos::tests::push_timeout_is_recovered_when_remote_branch_exists
```

Expected: PASS.

### Task 4: Failure Semantics and Full Verification

**Files:**
- Modify: `src/target_repos.rs`

- [ ] **Step 1: Add non-recovery regression**

Add a test where fake `git push` times out and fake `git ls-remote` exits non-zero.

Expected assertion:

```rust
assert!(error.to_string().contains("git push -u origin article/missing timed out after"));
```

- [ ] **Step 2: Verify target tests**

Run:

```powershell
cargo test --locked target_repos::tests
```

Expected: PASS.

- [ ] **Step 3: Verify required checks**

Run:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

Expected: all commands PASS.

### Task 5: Plane Completion

**Files:**
- No repo files.

- [ ] **Step 1: Update ACS-96 status**

Set ACS-96 to `Done` only after verification reflects the actual result.

Expected: final report can say Plane status matches completed work.
