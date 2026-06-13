#!/usr/bin/env pwsh
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptPath = Join-Path $PSScriptRoot "..\scripts\article-news-report.ps1"
. $scriptPath

$script:AssertionCount = 0

function Assert-Equal {
  param(
    [Parameter(Mandatory = $true)][object]$Expected,
    [Parameter(Mandatory = $true)][object]$Actual,
    [Parameter(Mandatory = $true)][string]$Message
  )

  $script:AssertionCount++
  if ($Expected -ne $Actual) {
    throw "$Message Expected '$Expected', got '$Actual'."
  }
}

function Assert-True {
  param(
    [Parameter(Mandatory = $true)][bool]$Condition,
    [Parameter(Mandatory = $true)][string]$Message
  )

  $script:AssertionCount++
  if (-not $Condition) {
    throw $Message
  }
}

function Assert-False {
  param(
    [Parameter(Mandatory = $true)][bool]$Condition,
    [Parameter(Mandatory = $true)][string]$Message
  )

  Assert-True -Condition (-not $Condition) -Message $Message
}

Assert-Equal `
  -Expected "<https://example.com/a|A - B (C)>" `
  -Actual (ConvertTo-SlackLink -Url "https://example.com/a" -Text "A | B <C>") `
  -Message "Slack links should escape separator and angle brackets."

$validContent = ("This article has enough real body text for a translation smoke test. " * 3).Trim()
Assert-True `
  -Condition (Test-UsableArticleContent -Content $validContent) `
  -Message "Substantial article text should be usable."

Assert-False `
  -Condition (Test-UsableArticleContent -Content "Oops! Something went wrong. Please force reload this page.") `
  -Message "Browser error fallback text should not be treated as article body."

Assert-False `
  -Condition (Test-UsableArticleContent -Content "Cloudflare says verify you are human before continuing.") `
  -Message "Anti-bot interstitial text should not be treated as article body."

$noiseInput = @"
Warning: Skill descriptions were shortened to fit the 2% skills context budget.
Codex can still see every skill, but some descriptions are shorter.
Disable unused skills or plugins to leave more room for the rest.

翻訳本文
"@
Assert-Equal `
  -Expected "翻訳本文" `
  -Actual (Remove-AcpOutputNoise -Text $noiseInput) `
  -Message "ACP adapter noise should be removed from translated text."

$entry = New-TranslationReportEntry -TranslationInput ([pscustomobject]@{
    id = "42"
    title = "Original title"
    fetched_title = "Fetched title"
    url = "https://example.com/article"
    content_note = "article content fetched"
    content_excerpt = "Body text"
  })
Assert-True `
  -Condition ($entry -match "Title: Fetched title") `
  -Message "Fetched title should take priority in translation input."

$parsed = ConvertFrom-TranslatedSection -Section @"
Warning: Skill descriptions were shortened to fit the 2% skills context budget.
### Item 42
Title: 自然な日本語タイトル
Context: article content fetched
Article excerpt:
これは短い要約です。
追加の背景です。
"@
Assert-Equal `
  -Expected "自然な日本語タイトル" `
  -Actual $parsed.translated_title `
  -Message "Translated section should extract explicit title."
Assert-Equal `
  -Expected "これは短い要約です。 追加の背景です。" `
  -Actual $parsed.summary_ja `
  -Message "Translated section should remove metadata lines from summary."

$fallbackParsed = ConvertFrom-TranslatedSection -Section @"
タイトルなしの翻訳タイトル
本文から作った要約です。
"@
Assert-Equal `
  -Expected "タイトルなしの翻訳タイトル" `
  -Actual $fallbackParsed.translated_title `
  -Message "First non-metadata line should be used as fallback title."
Assert-Equal `
  -Expected "本文から作った要約です。" `
  -Actual $fallbackParsed.summary_ja `
  -Message "Remaining fallback lines should become summary."

$pwsh = (Get-Command pwsh -ErrorAction Stop).Source
$processResult = Invoke-ProcessWithTimeout `
  -FilePath $pwsh `
  -ArgumentList @("-NoLogo", "-NoProfile", "-Command", "Write-Output ok") `
  -TimeoutSeconds 10
Assert-Equal `
  -Expected 0 `
  -Actual $processResult.ExitCode `
  -Message "Process helper should return successful exit code."
Assert-Equal `
  -Expected "ok" `
  -Actual $processResult.Output `
  -Message "Process helper should capture stdout."

Write-Host "article-news-report PowerShell tests passed ($script:AssertionCount assertions)."
