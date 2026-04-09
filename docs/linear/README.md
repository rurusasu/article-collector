# Linear Issue Guide

## Overview

This project uses [Linear](https://linear.app/) for issue tracking.
Follow the rules below when creating issues.

## Team & Project

| Item | Value |
|------|-------|
| Team | `Life-style-base` (LIF) |

### Projects

| Project | Repository | Description |
|---------|-----------|-------------|
| `ArticleCollector` | `article-collector` | Rust製記事収集パイプライン CLI |
| `ContentsForge` | `contentsforge` | ComfyUI 画像生成管理ツール |

## Issue Template

### Title

- Write concisely in Japanese
- Make the task clear at a glance
- Examples: `Phase 2: Tag & Workflow CRUD`, `ComfyUI WebSocket progress monitoring`

### Description

Write in Markdown with the following structure:

```markdown
## Overview

Briefly describe the purpose and background of the change.

## Changes

List changes by category using bullet points.

### Category (e.g., Database / Schema)
- Change 1
- Change 2

### Category (e.g., Frontend / UI)
- Change 1

## Branch

`feat/xxx` or `fix/xxx`
```

### Priority

| Value | Meaning | Usage |
|-------|---------|-------|
| 1 | Urgent | Production incidents, emergency fixes |
| 2 | High | Release blockers |
| 3 | Medium | Normal development tasks |
| 4 | Low | Improvements, refactoring |

### Assignee

- Use `me` when assigning to yourself

## Linear MCP Tool Example

```
mcp__claude_ai_Linear__save_issue:
  title: "Issue title"
  team: "Life-style-base"
  project: "ArticleCollector"
  description: |
    ## Overview
    ...
    ## Changes
    ...
    ## Branch
    `feat/xxx`
  priority: 3
  assignee: "me"
```

## Notes

- Always associate issues with the appropriate project (this repo uses `ArticleCollector`)
- Include the branch name in the issue description
- Organize changes by category
