#!/usr/bin/env pwsh
[CmdletBinding()]
param(
  [ValidateRange(1, 20)]
  [int]$Count = 5,

  [string]$OutDir = (Join-Path $env:TEMP ("article-news-report-" + (Get-Date -Format "yyyyMMdd-HHmmss"))),

  [ValidateSet("hackernews")]
  [string]$Source = "hackernews",

  [switch]$Translate,

  [ValidateSet("ja")]
  [string]$TranslationLanguage = "ja"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-ArticleCollector {
  $candidates = @(
    (Join-Path $env:USERPROFILE ".local\bin\article-collector.exe"),
    (Join-Path $env:USERPROFILE ".local\bin\article-collector")
  )

  foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
      return $candidate
    }
  }

  $command = Get-Command article-collector -ErrorAction SilentlyContinue
  if ($command) {
    return $command.Source
  }

  $command = Get-Command article-collector.exe -ErrorAction SilentlyContinue
  if ($command) {
    return $command.Source
  }

  throw "article-collector is not installed. Expected it in ~/.local/bin."
}

function ConvertTo-SlackLink {
  param(
    [Parameter(Mandatory = $true)][string]$Url,
    [Parameter(Mandatory = $true)][string]$Text
  )

  $safeText = $Text.Replace("|", "-").Replace("<", "(").Replace(">", ")")
  return "<$Url|$safeText>"
}

function Limit-Text {
  param(
    [AllowNull()][string]$Text,
    [int]$MaxLength = 5000
  )

  if ([string]::IsNullOrWhiteSpace($Text)) {
    return ""
  }

  $normalized = ($Text -replace "\s+", " ").Trim()
  if ($normalized.Length -le $MaxLength) {
    return $normalized
  }

  return $normalized.Substring(0, $MaxLength) + "..."
}

function Test-UsableArticleContent {
  param([AllowNull()][string]$Content)

  if ([string]::IsNullOrWhiteSpace($Content)) {
    return $false
  }

  $trimmed = $Content.Trim()
  if ($trimmed.Length -lt 80) {
    return $false
  }

  foreach ($blocked in @("Just a moment", "Enable JavaScript", "Access denied", "403 Forbidden", "Cloudflare", "verify you are human", "Something went wrong", "style got mangled", "force-reload", "force reload", "Oops!", "Oops, something went wrong")) {
    if ($trimmed -match [regex]::Escape($blocked)) {
      return $false
    }
  }

  return $true
}

function Invoke-ArticleCollectorFetch {
  param(
    [Parameter(Mandatory = $true)][string]$ArticleCollector,
    [Parameter(Mandatory = $true)][string]$Url,
    [Parameter(Mandatory = $true)][string]$ItemDir
  )

  New-Item -ItemType Directory -Path $ItemDir -Force | Out-Null

  $oldOutDir = [Environment]::GetEnvironmentVariable("ARTICLE_COLLECTOR_OUTDIR", "Process")
  try {
    $env:ARTICLE_COLLECTOR_OUTDIR = $ItemDir
    $output = & $ArticleCollector fetch $Url 2>&1
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
      $joined = ($output | Out-String).Trim()
      throw "article-collector fetch failed (exit=$exitCode): $joined"
    }

    $rawPath = Join-Path $ItemDir "raw.json"
    if (-not (Test-Path -LiteralPath $rawPath -PathType Leaf)) {
      throw "article-collector did not create raw.json for $Url"
    }

    $raw = Get-Content -LiteralPath $rawPath -Raw | ConvertFrom-Json
    if ($raw -is [array]) {
      return $raw[0]
    }

    return $raw
  } finally {
    if ($null -eq $oldOutDir) {
      Remove-Item Env:\ARTICLE_COLLECTOR_OUTDIR -ErrorAction SilentlyContinue
    } else {
      $env:ARTICLE_COLLECTOR_OUTDIR = $oldOutDir
    }
  }
}

function Get-HackerNewsReportItems {
  param(
    [Parameter(Mandatory = $true)][string]$ArticleCollector,
    [Parameter(Mandatory = $true)][string]$ReportOutDir,
    [Parameter(Mandatory = $true)][int]$Limit
  )

  $topIdsJson = (Invoke-WebRequest -UseBasicParsing -Uri "https://hacker-news.firebaseio.com/v0/topstories.json" -TimeoutSec 20).Content
  $topIds = [System.Text.Json.JsonSerializer]::Deserialize[long[]]($topIdsJson)
  if ($topIds.Count -eq 0) {
    throw "Hacker News topstories returned no IDs."
  }

  $items = New-Object System.Collections.Generic.List[object]
  $failures = New-Object System.Collections.Generic.List[string]
  $scanLimit = [Math]::Min($topIds.Count, [Math]::Max($Limit * 4, $Limit))

  for ($index = 0; $index -lt $scanLimit -and $items.Count -lt $Limit; $index++) {
    $id = [string]$topIds[$index]
    $itemUrl = "https://news.ycombinator.com/item?id=$id"
    $itemDir = Join-Path $ReportOutDir ("hn-" + $id)

    try {
      $item = Invoke-ArticleCollectorFetch -ArticleCollector $ArticleCollector -Url $itemUrl -ItemDir $itemDir
      if (-not $item) {
        continue
      }

      $title = if ($item.title) { [string]$item.title } else { "Untitled" }
      $link = if ($item.url) { [string]$item.url } else { $itemUrl }
      $author = if ($item.author) { [string]$item.author } else { "unknown" }
      $score = if ($null -ne $item.score) { [int]$item.score } else { 0 }
      $comments = if ($null -ne $item.descendants) { [int]$item.descendants } else { 0 }

      $items.Add([pscustomobject]@{
          Id = $id
          Title = $title
          Url = $link
          HnUrl = $itemUrl
          Author = $author
          Score = $score
          Comments = $comments
        }) | Out-Null
    } catch {
      $failures.Add("${itemUrl}: $($_.Exception.Message)") | Out-Null
    }
  }

  return [pscustomobject]@{
    Items = $items.ToArray()
    Failures = $failures.ToArray()
  }
}

function Get-ArticleTranslationInputs {
  param(
    [Parameter(Mandatory = $true)][string]$ArticleCollector,
    [Parameter(Mandatory = $true)][string]$ReportOutDir,
    [Parameter(Mandatory = $true)][object[]]$Items
  )

  $inputs = New-Object System.Collections.Generic.List[object]
  $warnings = New-Object System.Collections.Generic.List[string]

  foreach ($item in $Items) {
    $articleUrl = [string]($item.Url)
    $hnUrl = [string]($item.HnUrl)
    $contentExcerpt = ""
    $contentNote = "article content not fetched"
    $fetchedTitle = ""

    if (-not [string]::IsNullOrWhiteSpace($articleUrl) -and $articleUrl -ne $hnUrl) {
      $articleDir = Join-Path $ReportOutDir ("article-" + [string]($item.Id))

      try {
        $article = Invoke-ArticleCollectorFetch -ArticleCollector $ArticleCollector -Url $articleUrl -ItemDir $articleDir
        if ($article.title) {
          $fetchedTitle = [string]($article.title)
        }

        $rawContent = ""
        if ($article.content) {
          $rawContent = [string]($article.content)
        } elseif ($article.text) {
          $rawContent = [string]($article.text)
        }

        if (Test-UsableArticleContent -Content $rawContent) {
          $contentExcerpt = Limit-Text -Text $rawContent -MaxLength 1200
          $contentNote = "article content fetched"
        } else {
          $contentNote = "no usable article content fetched; use title only"
        }
      } catch {
        $contentNote = "article fetch failed; use title only"
        $warnings.Add(("translation fetch failed for {0}: {1}" -f $articleUrl, $_.Exception.Message)) | Out-Null
      }
    } else {
      $contentNote = "HN discussion only; use title only"
    }

    $inputs.Add([pscustomobject]@{
        id = [string]($item.Id)
        title = [string]($item.Title)
        fetched_title = $fetchedTitle
        url = $articleUrl
        hn_url = $hnUrl
        content_note = $contentNote
        content_excerpt = $contentExcerpt
      }) | Out-Null
  }

  return [pscustomobject]@{
    Inputs = $inputs.ToArray()
    Warnings = $warnings.ToArray()
  }
}

function Remove-AcpOutputNoise {
  param([AllowNull()][string]$Text)

  if ([string]::IsNullOrWhiteSpace($Text)) {
    return ""
  }

  $noisePatterns = @(
    '^Warning: Skill descriptions were shortened',
    '^Codex can still see every skill',
    '^Disable unused skills or plugins'
  )

  $lines = foreach ($line in ($Text -split '\r?\n')) {
    $trimmed = $line.Trim()
    $isNoise = $false
    foreach ($pattern in $noisePatterns) {
      if ($trimmed -match $pattern) {
        $isNoise = $true
        break
      }
    }
    if (-not $isNoise) {
      $line
    }
  }

  return (($lines -join [Environment]::NewLine).Trim())
}

function New-TranslationReportEntry {
  param([Parameter(Mandatory = $true)][object]$TranslationInput)

  $title = if (-not [string]::IsNullOrWhiteSpace([string]($TranslationInput.fetched_title))) {
    [string]($TranslationInput.fetched_title)
  } else {
    [string]($TranslationInput.title)
  }

  $lines = New-Object System.Collections.Generic.List[string]
  $lines.Add(("### Item {0}" -f $TranslationInput.id)) | Out-Null
  $lines.Add(("Title: {0}" -f $title)) | Out-Null
  $lines.Add(("URL: {0}" -f $TranslationInput.url)) | Out-Null
  $lines.Add(("Context: {0}" -f $TranslationInput.content_note)) | Out-Null
  $lines.Add("") | Out-Null

  if ([string]::IsNullOrWhiteSpace([string]($TranslationInput.content_excerpt))) {
    $lines.Add("No usable article body was fetched. Translate the title naturally for Japanese technical readers.") | Out-Null
  } else {
    $lines.Add("Article excerpt:") | Out-Null
    $lines.Add([string]($TranslationInput.content_excerpt)) | Out-Null
  }

  return ($lines -join "`n")
}

function ConvertFrom-TranslatedSection {
  param([Parameter(Mandatory = $true)][string]$Section)

  $clean = Remove-AcpOutputNoise -Text $Section
  $lines = @(
    $clean -split '\r?\n' |
      ForEach-Object { $_.Trim() } |
      Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
  )

  if ($lines.Count -eq 0) {
    return [pscustomobject]@{
      translated_title = ""
      summary_ja = ""
    }
  }

  $titleLine = $null
  foreach ($line in $lines) {
    if ($line -match '^\s*(?:[-*]\s*)?(?:#{1,6}\s*)?(?:タイトル|Title)\s*[:：]\s*(.+)$') {
      $titleLine = $Matches[1].Trim()
      break
    }
  }

  if (-not $titleLine) {
    foreach ($line in $lines) {
      if ($line -match '^\s*#{1,6}\s*(?:Item|アイテム)\s+\S+') {
        continue
      }
      if ($line -match '^\s*(?:URL|リンク)\s*[:：]') {
        continue
      }
      $titleLine = $line
      break
    }
  }

  $titleLine = ([string]$titleLine) -replace '^\s*#{1,6}\s*', ''
  $titleLine = $titleLine -replace '^\s*(?:タイトル|Title)\s*[:：]\s*', ''

  $summaryLines = foreach ($line in $lines) {
    if ($line -eq $titleLine) {
      continue
    }
    if ($line -match '^\s*#{1,6}\s*(?:Item|アイテム)\s+\S+') {
      continue
    }
    if ($line -match '^\s*(?:タイトル|Title|URL|リンク|Context|文脈|Article excerpt|記事抜粋)\s*[:：]') {
      continue
    }
    $line
  }

  return [pscustomobject]@{
    translated_title = (Limit-Text -Text $titleLine -MaxLength 160)
    summary_ja = (Limit-Text -Text ($summaryLines -join " ") -MaxLength 220)
  }
}

function Invoke-ProcessWithTimeout {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [Parameter(Mandatory = $true)][string[]]$ArgumentList,
    [int]$TimeoutSeconds = 240
  )

  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $FilePath
  foreach ($argument in $ArgumentList) {
    [void]$startInfo.ArgumentList.Add($argument)
  }
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true

  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  [void]$process.Start()

  if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
    try {
      $process.Kill($true)
    } catch {
      $process.Kill()
    }
    throw "$FilePath timed out after $TimeoutSeconds seconds"
  }

  $stdout = $process.StandardOutput.ReadToEnd()
  $stderr = $process.StandardError.ReadToEnd()
  return [pscustomobject]@{
    ExitCode = $process.ExitCode
    Output = (($stdout, $stderr | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join [Environment]::NewLine).Trim()
  }
}

function Invoke-ArticleCollectorReportTranslation {
  param(
    [Parameter(Mandatory = $true)][string]$ArticleCollector,
    [Parameter(Mandatory = $true)][object[]]$Inputs,
    [Parameter(Mandatory = $true)][string]$ReportOutDir,
    [Parameter(Mandatory = $true)][string]$Language
  )

  $lookup = @{}
  if ($Inputs.Count -eq 0) {
    return ,$lookup
  }

  $codexHome = Join-Path $env:USERPROFILE ".codex"
  if (-not (Test-Path -LiteralPath (Join-Path $codexHome "auth.json") -PathType Leaf)) {
    throw "Codex OAuth login was not found at $codexHome. Run codex login before enabling translation."
  }

  $translationDir = Join-Path $ReportOutDir "translation-report"
  New-Item -ItemType Directory -Path $translationDir -Force | Out-Null

  $rawItems = foreach ($translationInput in $Inputs) {
    [pscustomobject]@{
      content = (New-TranslationReportEntry -TranslationInput $translationInput)
    }
  }
  $rawPath = Join-Path $translationDir "raw.json"
  $rawItems | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $rawPath -Encoding UTF8

  $oldOutDir = [Environment]::GetEnvironmentVariable("ARTICLE_COLLECTOR_OUTDIR", "Process")
  $oldAgent = [Environment]::GetEnvironmentVariable("ACP_AGENT", "Process")
  $oldLang = [Environment]::GetEnvironmentVariable("TRANSLATE_LANG", "Process")
  $oldCodexHome = [Environment]::GetEnvironmentVariable("CODEX_HOME", "Process")

  try {
    $env:ARTICLE_COLLECTOR_OUTDIR = $translationDir
    $env:ACP_AGENT = "codex"
    $env:TRANSLATE_LANG = $Language
    $env:CODEX_HOME = $codexHome

    $result = Invoke-ProcessWithTimeout `
      -FilePath $ArticleCollector `
      -ArgumentList @("translate", $rawPath) `
      -TimeoutSeconds 240
    $output = $result.Output
    $exitCode = $result.ExitCode
  } finally {
    if ($null -eq $oldOutDir) {
      Remove-Item Env:\ARTICLE_COLLECTOR_OUTDIR -ErrorAction SilentlyContinue
    } else {
      $env:ARTICLE_COLLECTOR_OUTDIR = $oldOutDir
    }
    if ($null -eq $oldAgent) {
      Remove-Item Env:\ACP_AGENT -ErrorAction SilentlyContinue
    } else {
      $env:ACP_AGENT = $oldAgent
    }
    if ($null -eq $oldLang) {
      Remove-Item Env:\TRANSLATE_LANG -ErrorAction SilentlyContinue
    } else {
      $env:TRANSLATE_LANG = $oldLang
    }
    if ($null -eq $oldCodexHome) {
      Remove-Item Env:\CODEX_HOME -ErrorAction SilentlyContinue
    } else {
      $env:CODEX_HOME = $oldCodexHome
    }
  }

  if ($exitCode -ne 0) {
    $joined = ($output | Out-String).Trim()
    throw "article-collector translate failed (exit=$exitCode): $joined"
  }

  $translatedPath = Join-Path $translationDir "translated.md"
  if (-not (Test-Path -LiteralPath $translatedPath -PathType Leaf)) {
    $joined = ($output | Out-String).Trim()
    throw "article-collector translate did not create translated.md: $joined"
  }

  $translatedText = Remove-AcpOutputNoise -Text (Get-Content -LiteralPath $translatedPath -Raw)
  $sections = @(
    $translatedText -split '(?m)^\s*---\s*$' |
      ForEach-Object { $_.Trim() } |
      Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
  )

  for ($index = 0; $index -lt $Inputs.Count; $index++) {
    $translationInput = $Inputs[$index]
    $section = if ($index -lt $sections.Count) { $sections[$index] } else { "" }
    $lookup[[string]($translationInput.id)] = ConvertFrom-TranslatedSection -Section $section
  }

  return ,$lookup
}

function Invoke-ArticleNewsReport {
  [CmdletBinding()]
  param(
    [ValidateRange(1, 20)]
    [int]$Count = 5,

    [string]$OutDir = (Join-Path $env:TEMP ("article-news-report-" + (Get-Date -Format "yyyyMMdd-HHmmss"))),

    [ValidateSet("hackernews")]
    [string]$Source = "hackernews",

    [switch]$Translate,

    [ValidateSet("ja")]
    [string]$TranslationLanguage = "ja"
  )

$articleCollector = Resolve-ArticleCollector
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null

if ($Source -eq "hackernews") {
  $result = Get-HackerNewsReportItems -ArticleCollector $articleCollector -ReportOutDir $OutDir -Limit $Count
} else {
  throw "Unsupported source: $Source"
}

$items = @($result.Items)
$failures = @($result.Failures)
$translationWarnings = New-Object System.Collections.Generic.List[string]
$translationsById = @{}

if ($items.Count -eq 0) {
  $failureText = if ($failures.Count -gt 0) { $failures -join "`n" } else { "No items collected." }
  throw "article-news-report produced no report items. $failureText"
}

if ($Translate) {
  try {
    $translationInputs = Get-ArticleTranslationInputs -ArticleCollector $articleCollector -ReportOutDir $OutDir -Items $items
    foreach ($warning in @($translationInputs.Warnings)) {
      $translationWarnings.Add($warning) | Out-Null
    }

    $translationsById = Invoke-ArticleCollectorReportTranslation `
      -ArticleCollector $articleCollector `
      -Inputs @($translationInputs.Inputs) `
      -ReportOutDir $OutDir `
      -Language $TranslationLanguage
  } catch {
    $translationWarnings.Add("article-collector ACP translation failed: $($_.Exception.Message)") | Out-Null
  }
}

$timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss zzz"
$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("*Hourly article-collector report*") | Out-Null
$lines.Add("Generated: $timestamp") | Out-Null
$lines.Add("Source: Hacker News top stories") | Out-Null
if ($Translate) {
  $lines.Add("Translation: article-collector ACP/codex -> $TranslationLanguage") | Out-Null
}
$lines.Add(("OutDir: ``{0}``" -f $OutDir)) | Out-Null
$lines.Add("") | Out-Null

for ($i = 0; $i -lt $items.Count; $i++) {
  $item = $items[$i]
  $rank = $i + 1
  $articleUrl = [string]($item.Url)
  $articleTitle = [string]($item.Title)
  $hnUrl = [string]($item.HnUrl)
  $articleLink = ConvertTo-SlackLink -Url $articleUrl -Text $articleTitle
  $hnLink = ConvertTo-SlackLink -Url $hnUrl -Text "HN"
  $lines.Add(("{0}. {1}" -f $rank, $articleLink)) | Out-Null
  $lines.Add(("   {0} points, {1} comments, by {2} ({3})" -f $item.Score, $item.Comments, $item.Author, $hnLink)) | Out-Null

  if ($Translate -and $translationsById.ContainsKey([string]($item.Id))) {
    $translation = $translationsById[[string]($item.Id)]
    $translatedTitle = if ($translation.translated_title) { Limit-Text -Text ([string]($translation.translated_title)) -MaxLength 160 } else { "" }
    $summaryJa = if ($translation.summary_ja) { Limit-Text -Text ([string]($translation.summary_ja)) -MaxLength 220 } else { "" }

    if (-not [string]::IsNullOrWhiteSpace($translatedTitle)) {
      $lines.Add(("   訳: {0}" -f $translatedTitle)) | Out-Null
    }
    if (-not [string]::IsNullOrWhiteSpace($summaryJa)) {
      $lines.Add(("   要約: {0}" -f $summaryJa)) | Out-Null
    }
  }
}

$allWarnings = @($failures) + @($translationWarnings.ToArray())
if ($allWarnings.Count -gt 0) {
  $lines.Add("") | Out-Null
  $lines.Add("*Warnings*") | Out-Null
  foreach ($failure in $allWarnings | Select-Object -First 8) {
    $lines.Add("- $failure") | Out-Null
  }
}

return ($lines -join "`n")
}

if ($MyInvocation.InvocationName -ne ".") {
  Invoke-ArticleNewsReport @PSBoundParameters
}
