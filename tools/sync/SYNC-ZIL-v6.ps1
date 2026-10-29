#requires -Version 5.1
<#
.SYNOPSIS
  自动发现当前目录中的 Git Bundle，交互映射分支并安全同步至 GitHub。
.DESCRIPTION
  无 -Bundle 时：先搜索当前工作目录，再搜索脚本目录；优先 .bundle，兼容含单个 .bundle 的 ZIP。
  有多个候选包时列出供选择，回车选最近修改的包。
  无论 Bundle 只有一个还是多个分支，默认逐个提示目标分支；回车保留默认，- 跳过。
  默认为同名映射，包括 main；Enter 保留 main -> main。
  -MainOnly 仅同步 main，但必须先证明 Bundle 的其他分支均已被 main 包含。
  -SkipMain 保留旧版跳过 main 的行为；-AllowMain 仍兼容但已不再需要。
  同步只快进/创建；不自动合并、不强推、不删除远端分支。
.EXAMPLE
  .\SYNC-ZIL-v6.ps1
  .\SYNC-ZIL-v6.ps1 -Bundle .\zil.bundle
  .\SYNC-ZIL-v6.ps1 -MainOnly
  .\SYNC-ZIL-v6.ps1 -MainOnly -NonInteractive
  .\SYNC-ZIL-v6.ps1 -DryRun
  .\SYNC-ZIL-v6.ps1 -SkipMain
  .\SYNC-ZIL-v6.ps1 -NetworkTimeoutSec 60
#>
[CmdletBinding()]
param(
  [Parameter(Position=0)][string]$Bundle = '',
  [string]$Repo = 'https://github.com/zil-lang/zil.git',
  [switch]$DryRun,
  [switch]$NonInteractive,
  [switch]$AllowMain,
  [switch]$MainOnly,
  [switch]$SkipMain,
  [string]$CacheDir = '',
  [ValidateRange(10,3600)][int]$NetworkTimeoutSec = 120
)

$ErrorActionPreference = 'Stop'
$script:TemporaryDirectory = ''
$script:Cache = ''
$script:GitExe = ''
$script:ExitCode = 0

function Invoke-Git {
  param([Parameter(Mandatory=$true)][string[]]$Arguments, [switch]$AllowFailure)
  # 使用已解析的 git.exe 可执行文件，不能用 function Git 与外部 git 同名。
  $previous = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    $lines = @(& $script:GitExe @Arguments 2>&1 | ForEach-Object { $_.ToString() })
    $exitCode = $LASTEXITCODE
  } finally { $ErrorActionPreference = $previous }
  $output = ($lines -join "`n").Trim()
  if ($exitCode -ne 0 -and -not $AllowFailure) {
    throw "Git 失败 (exit=$exitCode): git $($Arguments -join ' ')`n$output"
  }
  return [pscustomobject]@{ Code = $exitCode; Text = $output }
}

# 网络 Git 命令独立进程执行：实时标明步骤，强制超时，避免凭据弹窗静默等待。
function Quote-WindowsArg([string]$Value) {
  if ($Value -notmatch '[\s\"]') { return $Value }
  $r = '"'; $slashes = 0
  foreach ($ch in $Value.ToCharArray()) {
    if ($ch -eq '\') { $slashes++; continue }
    if ($ch -eq '"') { $r += ('\' * (2 * $slashes + 1)) + '"'; $slashes = 0; continue }
    $r += ('\' * $slashes) + $ch; $slashes = 0
  }
  return $r + ('\' * ($slashes * 2)) + '"'
}
function Invoke-GitNetwork {
  param([string[]]$Arguments, [string]$Step, [switch]$AllowFailure)
  Write-Host ("  [网络] {0}（最长 {1}s）" -f $Step, $NetworkTimeoutSec) -ForegroundColor Cyan
  $stdoutPath = Join-Path ([IO.Path]::GetTempPath()) ('zil-git-out-' + [guid]::NewGuid().ToString('N') + '.txt')
  $stderrPath = Join-Path ([IO.Path]::GetTempPath()) ('zil-git-err-' + [guid]::NewGuid().ToString('N') + '.txt')
  $oldPrompt = $env:GIT_TERMINAL_PROMPT; $oldInteractive = $env:GCM_INTERACTIVE
  $env:GIT_TERMINAL_PROMPT = '0'; $env:GCM_INTERACTIVE = 'Never'
  $proc = $null
  try {
    # 文件重定向防止 Git 输出占满管道；WaitForExit 有明确时限。
    $line = ($Arguments | ForEach-Object { Quote-WindowsArg $_ }) -join ' '
    $proc = Start-Process -FilePath $script:GitExe -ArgumentList $line -PassThru -NoNewWindow -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    if (-not $proc.WaitForExit($NetworkTimeoutSec * 1000)) {
      try { & taskkill.exe /PID $proc.Id /T /F 2>$null | Out-Null } catch { }
      try { if (-not $proc.HasExited) { $proc.Kill() } } catch { }
      throw "网络超时：$Step（${NetworkTimeoutSec}s）。检查 GitHub/VPN/代理/凭据配置。"
    }
    $code = $proc.ExitCode
    $output = (([IO.File]::ReadAllText($stdoutPath) + "`n" + [IO.File]::ReadAllText($stderrPath)).Trim())
    if ($code -ne 0 -and -not $AllowFailure) { throw "Git 网络操作失败 (exit=$code): $Step`n$output" }
    Write-Host ("  [完成] {0}（exit={1}）" -f $Step,$code) -ForegroundColor $(if ($code -eq 0) { 'Green' } else { 'Yellow' })
    return [pscustomobject]@{ Code=$code; Text=$output }
  } finally {
    $env:GIT_TERMINAL_PROMPT=$oldPrompt; $env:GCM_INTERACTIVE=$oldInteractive
    if ($null -ne $proc) { $proc.Dispose() }
    Remove-Item -LiteralPath $stdoutPath,$stderrPath -Force -ErrorAction SilentlyContinue
  }
}

function Assert-BranchName {
  param([string]$Name)
  if ([string]::IsNullOrWhiteSpace($Name) -or $Name -ieq 'HEAD') { throw "非法分支名：$Name" }
  if ((Invoke-Git -Arguments @('check-ref-format', '--branch', $Name) -AllowFailure).Code -ne 0) {
    throw "非法分支名：$Name"
  }
}

function Test-ZipWithBundle {
  param([string]$Path)
  $zip = $null
  try {
    $zip = [IO.Compression.ZipFile]::OpenRead($Path)
    $matches = @($zip.Entries | Where-Object {
      $_.Name -and $_.FullName.EndsWith('.bundle', [StringComparison]::OrdinalIgnoreCase)
    })
    return ($matches.Count -eq 1)
  } catch { return $false }
  finally { if ($null -ne $zip) { $zip.Dispose() } }
}

function Select-BundleCandidate {
  param([object[]]$Files, [string]$Directory)
  $items = @($Files | Sort-Object LastWriteTimeUtc -Descending)
  if ($items.Count -eq 1) {
    Write-Host "自动发现 Bundle：$($items[0].FullName)"
    return $items[0].FullName
  }
  if ($NonInteractive) {
    throw "目录 $Directory 中找到 $($items.Count) 个候选包；-NonInteractive 时请用 -Bundle 指定。"
  }
  Write-Host "`n在 $Directory 找到 $($items.Count) 个候选包："
  for ($i = 0; $i -lt $items.Count; $i++) {
    $file = $items[$i]
    Write-Host ("  [{0}] {1} ({2:yyyy-MM-dd HH:mm}, {3:N1} MB)" -f ($i + 1), $file.Name, $file.LastWriteTime, ($file.Length / 1MB))
  }
  while ($true) {
    $answer = Read-Host '请选择编号（Enter=最新，即 1）'
    if ([string]::IsNullOrWhiteSpace($answer)) { return $items[0].FullName }
    $index = 0
    if ([int]::TryParse($answer.Trim(), [ref]$index) -and $index -ge 1 -and $index -le $items.Count) {
      return $items[$index - 1].FullName
    }
    Write-Host '输入无效，请输入列表编号或直接回车。' -ForegroundColor Yellow
  }
}

function Resolve-BundleFile {
  if (-not [string]::IsNullOrWhiteSpace($Bundle)) {
    $file = Get-Item -LiteralPath $Bundle -ErrorAction Stop
    if ($file.PSIsContainer) { throw '-Bundle 必须是文件，不能是目录。' }
    return $file.FullName
  }
  $directories = @((Get-Location).ProviderPath)
  if ($PSScriptRoot -and -not [string]::Equals($directories[0], $PSScriptRoot, [StringComparison]::OrdinalIgnoreCase)) {
    $directories += $PSScriptRoot
  }
  foreach ($directory in $directories) {
    # 先找当前工作目录的 .bundle；找不到时才考虑脚本目录。
    $bundles = @(Get-ChildItem -LiteralPath $directory -File -Filter '*.bundle' -ErrorAction Stop)
    if ($bundles.Count -gt 0) { return (Select-BundleCandidate -Files $bundles -Directory $directory) }
    $zips = @(Get-ChildItem -LiteralPath $directory -File -Filter '*.zip' -ErrorAction Stop |
      Where-Object { Test-ZipWithBundle -Path $_.FullName })
    if ($zips.Count -gt 0) { return (Select-BundleCandidate -Files $zips -Directory $directory) }
  }
  throw '当前目录和脚本目录都没有找到 .bundle（或含单个 Bundle 的 ZIP）；可通过 -Bundle 显式指定。'
}

function Expand-BundleIfZip {
  param([string]$Path)
  $extension = [IO.Path]::GetExtension($Path)
  if ($extension -ieq '.bundle') { return $Path }
  if ($extension -ine '.zip') { throw '输入必须为 .bundle 或包含恰好一个 .bundle 的 ZIP。' }
  $script:TemporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ('zil-sync-' + [guid]::NewGuid().ToString('N'))
  [void](New-Item -ItemType Directory -Force -Path $script:TemporaryDirectory)
  $zip = [IO.Compression.ZipFile]::OpenRead($Path)
  try {
    $entries = @($zip.Entries | Where-Object {
      $_.Name -and $_.FullName.EndsWith('.bundle', [StringComparison]::OrdinalIgnoreCase)
    })
    if ($entries.Count -ne 1) { throw "ZIP 必须包含恰好一个 .bundle，实际为 $($entries.Count) 个。" }
    $outFile = Join-Path $script:TemporaryDirectory 'source.bundle'
    $inputStream = $entries[0].Open()
    try {
      $outputStream = [IO.File]::Create($outFile)
      try { $inputStream.CopyTo($outputStream) }
      finally { $outputStream.Dispose() }
    } finally { $inputStream.Dispose() }
    Write-Host "从 ZIP 解出 Bundle：$($entries[0].FullName)"
    return $outFile
  } finally { $zip.Dispose() }
}

function Get-RemoteSha {
  param([string]$Name)
  $result = Invoke-GitNetwork -Arguments @('ls-remote', '--heads', $Repo, "refs/heads/$Name") -Step "查询远端 $Name"
  if (-not $result.Text) { return '' }
  $match = [regex]::Match($result.Text, '^([0-9a-fA-F]{40,64})\s+refs/heads/')
  if (-not $match.Success) { throw "无法解析远端分支 $Name：$($result.Text)" }
  return $match.Groups[1].Value.ToLowerInvariant()
}

function Test-Ancestor {
  param([string]$Older, [string]$Newer)
  $result = Invoke-Git -Arguments @('-C', $script:Cache, 'merge-base', '--is-ancestor', $Older, $Newer) -AllowFailure
  if ($result.Code -eq 0) { return $true }
  if ($result.Code -eq 1) { return $false }
  throw "祖先关系检查失败：$($result.Text)；远端可能更新，请重新执行。"
}

function Select-BranchTargets {
  param([object[]]$Heads)
  if ($MainOnly) {
    $main = @($Heads | Where-Object { $_.Source -ceq 'main' })
    if ($main.Count -ne 1) { throw '-MainOnly 要求 Bundle 含 refs/heads/main。' }
    foreach ($head in $Heads) {
      $head.Target = if ($head.Source -ceq 'main') { 'main' } else { '' }
    }
    Write-Host "`n主线同步：main -> main；先核验其已包含其余 $($Heads.Count - 1) 个 Bundle 分支。"
    return
  }
  Write-Host "`nBundle 含 $($Heads.Count) 个开发分支，逐个确认远端目标。"
  Write-Host 'Enter=保留默认；-=跳过；输入分支名=修改远端目标。'
  for ($i = 0; $i -lt $Heads.Count; $i++) {
    $head = $Heads[$i]
    $default = if ($head.Source -ieq 'main' -and $SkipMain) { '' } else { $head.Source }
    $label = if ($default) { $default } else { '跳过（-SkipMain）' }
    while ($true) {
      if ($NonInteractive) {
        $answer = ''
        Write-Host "  [$($i+1)/$($Heads.Count)] $($head.Source) -> $label (-NonInteractive)"
      } else {
        $answer = Read-Host "[$($i+1)/$($Heads.Count)] $($head.Source) -> 目标 [$label]"
      }
      $target = if ([string]::IsNullOrWhiteSpace($answer)) { $default } elseif ($answer.Trim() -eq '-') { '' } else { $answer.Trim() }
      if ($target) {
        try {
          Assert-BranchName -Name $target
          if ($target -ieq 'main' -and $SkipMain) {
            throw '当前指定了 -SkipMain，不能推送 main；去掉该参数即可同步 main。'
          }
        } catch {
          if ($NonInteractive) { throw }
          Write-Host $_.Exception.Message -ForegroundColor Yellow
          continue
        }
      }
      $head.Target = $target
      break
    }
  }
  $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
  foreach ($head in $Heads) {
    if ($head.Target -and -not $seen.Add($head.Target)) {
      throw "多个源分支映射到同一目标 $($head.Target)，为防止覆盖已停止。请重试并修改映射。"
    }
  }
}

function Write-Summary {
  param([object[]]$Heads, [bool]$IsDryRun)
  $changed = 0; $already = 0; $skipped = 0; $failed = 0; $preview = 0
  Write-Host "`n========== ZIL 同步结果 =========="
  foreach ($head in $Heads) {
    $target = if ($head.Target) { $head.Target } else { '(跳过)' }
    Write-Host ("{0,-29} -> {1,-29} {2}" -f $head.Source, $target, $head.State)
    if ($head.Detail) { Write-Host "    $($head.Detail)" -ForegroundColor Yellow }
    switch -Regex ($head.State) {
      '^(CREATED|UPDATED)$' { $changed++; break }
      '^(ALREADY|CONTAINED)$' { $already++; break }
      '^SKIP$' { $skipped++; break }
      '^(WOULD-CREATE|WOULD-UPDATE)$' { $preview++; break }
      default { $failed++; break }
    }
  }
  Write-Host "已创建/更新: $changed；已同步/包含: $already；跳过: $skipped；预览待推送: $preview；失败/分叉: $failed"
  if ($IsDryRun) {
    Write-Host '预览完成：未向远端推送任何分支。' -ForegroundColor Cyan
  } elseif ($failed -gt 0) {
    Write-Host '同步未全部成功；成功的分支已保留，失败/分叉分支没有被覆盖。' -ForegroundColor Yellow
  } elseif (($changed + $already) -eq 0) {
    Write-Host '本次没有需要同步的分支（全部跳过）。' -ForegroundColor Yellow
  } else {
    Write-Host '同步成功：所有选中的分支均已更新或确认远端包含；仅创建或快进，没有强推。' -ForegroundColor Green
  }
  return $failed
}

try {
  if ($SkipMain -and ($MainOnly -or $AllowMain)) {
    throw '-SkipMain 不能与 -MainOnly 或 -AllowMain 同时使用。'
  }
  $gitCommand = Get-Command git -CommandType Application -ErrorAction Stop | Select-Object -First 1
  $script:GitExe = $gitCommand.Source
  Add-Type -AssemblyName System.IO.Compression
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $picked = Resolve-BundleFile
  $bundleFile = Expand-BundleIfZip -Path $picked
  Write-Host "`n使用 Bundle：$picked"

  $listed = (Invoke-Git -Arguments @('bundle', 'list-heads', $bundleFile)).Text
  $heads = @()
  foreach ($line in ($listed -split "`n")) {
    $match = [regex]::Match($line.Trim(), '^([0-9a-fA-F]{40,64})\s+refs/heads/(.+)$')
    if ($match.Success) {
      $heads += [pscustomobject]@{
        Source = $match.Groups[2].Value; Ref = "refs/heads/$($match.Groups[2].Value)"
        Sha = $match.Groups[1].Value.ToLowerInvariant(); Target = ''; State = 'PENDING'; Detail = ''
      }
    }
  }
  if ($heads.Count -eq 0) {
    throw 'Bundle 没有 refs/heads/*。请检查 git bundle list-heads，并从实际工作分支导出 Bundle。'
  }
  $heads = @($heads | Sort-Object Source)
  Select-BranchTargets -Heads $heads
  $selected = @($heads | Where-Object { $_.Target })
  if ($selected.Count -eq 0) {
    foreach ($head in $heads) { $head.State = 'SKIP' }
    [void](Write-Summary -Heads $heads -IsDryRun ([bool]$DryRun))
  } else {
    if (-not $CacheDir) {
      $cacheRoot = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { [IO.Path]::GetTempPath() }
      $CacheDir = Join-Path $cacheRoot 'zil-sync-cache'
    }
    $script:Cache = Join-Path $CacheDir 'repository.git'
    if (-not (Test-Path -LiteralPath (Join-Path $script:Cache 'HEAD'))) {
      [void](New-Item -ItemType Directory -Force -Path $script:Cache)
      [void](Invoke-Git -Arguments @('init', '--bare', $script:Cache))
    }
    $existing = Invoke-Git -Arguments @('-C', $script:Cache, 'remote', 'get-url', 'origin') -AllowFailure
    if ($existing.Code -eq 0) {
      if ($existing.Text -ne $Repo) { throw "缓存已绑定其他仓库：$($existing.Text)；请使用独立 -CacheDir。" }
    } else {
      [void](Invoke-Git -Arguments @('-C', $script:Cache, 'remote', 'add', 'origin', $Repo))
    }

    Write-Host "`n验证 Bundle 并获取本次选中的 GitHub 远端分支（不再抓取全部分支）..."
    # 远端 heads 只查询 SHA，不下载全部分支；随后只下载本次有目标的分支。
    $remoteHeads = Invoke-GitNetwork -Arguments @('ls-remote', '--heads', $Repo) -Step '读取远端分支列表（只含引用）'
    $remoteMap = @{}
    foreach ($line in ($remoteHeads.Text -split "`n")) {
      $m = [regex]::Match($line.Trim(), '^([0-9a-fA-F]{40,64})\s+refs/heads/(.+)$')
      if ($m.Success) { $remoteMap[$m.Groups[2].Value] = $m.Groups[1].Value.ToLowerInvariant() }
    }
    foreach ($head in $selected) {
      if ($remoteMap.ContainsKey($head.Target)) {
        $ref = "+refs/heads/$($head.Target):refs/remotes/origin/$($head.Target)"
        [void](Invoke-GitNetwork -Arguments @('-C', $script:Cache, 'fetch', '--no-tags', 'origin', $ref) -Step "获取目标分支 $($head.Target) 的必要历史")
      }
    }
    Write-Host '  [本地] 校验 Bundle 所需 Git 对象...'
    [void](Invoke-Git -Arguments @('-C', $script:Cache, 'bundle', 'verify', $bundleFile))
    $importHeads = if ($MainOnly) { $heads } else { $selected }
    foreach ($head in $importHeads) {
      $incoming = "refs/zil-sync/incoming/$($head.Source)"
      # + 仅作用于私有缓存中的 incoming 引用，永不用于 GitHub push。
      [void](Invoke-Git -Arguments @('-C', $script:Cache, 'fetch', '--no-tags', $bundleFile, "+$($head.Ref):$incoming"))
      $actual = (Invoke-Git -Arguments @('-C', $script:Cache, 'rev-parse', '--verify', "${incoming}^{commit}")).Text
      if ($actual -ne $head.Sha) { throw "Bundle 分支 $($head.Source) 的提交 SHA 不符。" }
    }
    Write-Host '  [本地] 检查对象连通性...'
    [void](Invoke-Git -Arguments @('-C', $script:Cache, 'fsck', '--connectivity-only', '--no-reflogs'))

    if ($MainOnly) {
      $mainSha = (@($heads | Where-Object { $_.Source -ceq 'main' }))[0].Sha
      $unmerged = @($heads | Where-Object {
        $_.Source -cne 'main' -and -not (Test-Ancestor -Older $_.Sha -Newer $mainSha)
      })
      if ($unmerged.Count -gt 0) {
        throw "-MainOnly 已停止，main 尚未包含这些 Bundle 分支：$($unmerged.Source -join ', ')。请先在 sandbox 合并，或去掉 -MainOnly 分别同步；本次未推送。"
      }
      foreach ($head in $heads) {
        if ($head.Source -cne 'main') { $head.Detail = '已证明该提交包含在 Bundle main；本次仅同步 main。' }
      }
      Write-Host '  [本地] 已确认 Bundle 全部分支均被 main 包含，仅推送 main。' -ForegroundColor Green
    }

    foreach ($head in $heads) {
      if (-not $head.Target) { $head.State = 'SKIP'; continue }
      try {
        $remote = Get-RemoteSha -Name $head.Target
        if ($remote -eq $head.Sha) { $head.State = 'ALREADY'; continue }
        if ($remote) {
          if (Test-Ancestor -Older $head.Sha -Newer $remote) { $head.State = 'CONTAINED'; continue }
          if (-not (Test-Ancestor -Older $remote -Newer $head.Sha)) {
            $head.State = 'DIVERGED'
            $head.Detail = '远端和 Bundle 各有独立提交：未推送，需在集成工作树处理。'
            continue
          }
        }
        if ($DryRun) {
          $head.State = if ($remote) { 'WOULD-UPDATE' } else { 'WOULD-CREATE' }
          continue
        }
        # 双次核对；最终 Git push 仍由服务端拒绝非快进更新。
        if ((Get-RemoteSha -Name $head.Target) -ne $remote) {
          $head.State = 'REMOTE-CHANGED'
          $head.Detail = '远端分支在同步期间发生变化；请重跑。'
          continue
        }
        $push = Invoke-GitNetwork -Arguments @('-C', $script:Cache, 'push', 'origin', "$($head.Sha):refs/heads/$($head.Target)") -Step "推送 $($head.Target)" -AllowFailure
        if ($push.Code -ne 0) {
          $head.State = 'PUSH-FAILED'
          $head.Detail = $push.Text
          continue
        }
        if ((Get-RemoteSha -Name $head.Target) -ne $head.Sha) {
          $head.State = 'VERIFY-FAILED'
          $head.Detail = 'Git push 返回成功，但远端 SHA 回读不一致。'
          continue
        }
        $head.State = if ($remote) { 'UPDATED' } else { 'CREATED' }
      } catch {
        $head.State = 'ERROR'
        $head.Detail = $_.Exception.Message
      }
    }
    $failed = Write-Summary -Heads $heads -IsDryRun ([bool]$DryRun)
    if ($failed -gt 0) { $script:ExitCode = 2 }
  }
} catch {
  Write-Host "`n========== ZIL 同步失败 ==========" -ForegroundColor Red
  Write-Host $_.Exception.Message -ForegroundColor Red
  Write-Host '本次没有完成所有选定分支的同步；请检查上方失败原因。' -ForegroundColor Yellow
  $script:ExitCode = 1
} finally {
  if ($script:TemporaryDirectory -and (Test-Path -LiteralPath $script:TemporaryDirectory)) {
    Remove-Item -LiteralPath $script:TemporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
  }
}
exit $script:ExitCode
