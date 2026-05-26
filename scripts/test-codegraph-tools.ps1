# Integration test for Locus CodeGraph builtin tools (mirrors src-tauri/src/tool/builtins/codegraph.rs)
$ErrorActionPreference = "Stop"
$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Failed = 0
$Passed = 0

function Assert-True($cond, [string]$name, [string]$detail = "") {
    if ($cond) {
        $script:Passed++
        Write-Host "[PASS] $name" -ForegroundColor Green
    } else {
        $script:Failed++
        Write-Host "[FAIL] $name" -ForegroundColor Red
        if ($detail) { Write-Host "       $detail" -ForegroundColor DarkRed }
    }
}

function Invoke-CodegraphTool {
    param(
        [string]$Name,
        [string[]]$Args
    )
    Push-Location $ProjectRoot
    try {
        $stdout = & codegraph @Args 2>&1 | Out-String
        $exit = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    return @{
        Name = $Name
        ExitCode = $exit
        Stdout = $stdout.Trim()
        Stderr = ""
    }
}

Write-Host "=== CodeGraph tool integration test ===" -ForegroundColor Cyan
Write-Host "Project: $ProjectRoot`n"

# --- Static checks: tools registered in codebase ---
$modRs = Get-Content (Join-Path $ProjectRoot "src-tauri\src\tool\builtins\mod.rs") -Raw
$promptRs = Get-Content (Join-Path $ProjectRoot "src-tauri\src\prompt.rs") -Raw
$devConfig = Get-Content (Join-Path $ProjectRoot "agent\dev\config.json") -Raw | ConvertFrom-Json

Assert-True ($modRs -match "codegraph::register_all") "builtins/mod.rs registers codegraph module"
Assert-True ($promptRs -match "CODEGRAPH_SEARCH") "prompt.rs embeds CODEGRAPH tool schemas"

foreach ($tool in @(
    "codegraph_search", "codegraph_context", "codegraph_callers", "codegraph_callees",
    "codegraph_impact", "codegraph_files", "codegraph_status", "codegraph_sync"
)) {
    Assert-True (Test-Path (Join-Path $ProjectRoot "tools\$tool.json")) "schema: $tool.json"
    Assert-True ($devConfig.tools -contains $tool) "dev agent: $tool"
}

# --- CLI availability ---
$ver = & codegraph --version 2>&1
Assert-True ($LASTEXITCODE -eq 0) "codegraph CLI on PATH" $ver

# --- Tool executions (same args as Rust wrappers) ---
$r = Invoke-CodegraphTool "codegraph_status" @("status", $ProjectRoot)
Assert-True ($r.ExitCode -eq 0) "codegraph_status" 
Assert-True ($r.Stdout -match "Index Statistics") "status output has statistics" $r.Stdout.Substring(0, [Math]::Min(200, $r.Stdout.Length))

$r = Invoke-CodegraphTool "codegraph_search" @("query", "ToolRegistry", "-p", $ProjectRoot, "-j", "-l", "2")
Assert-True ($r.ExitCode -eq 0) "codegraph_search"
try {
    $parsed = $r.Stdout | ConvertFrom-Json
    Assert-True ($parsed.Count -ge 1) "search returns JSON array"
    Assert-True ($null -ne $parsed[0].node.name) "search result has node.name"
} catch {
    Assert-True $false "search JSON parse" $_.Exception.Message
}

$r = Invoke-CodegraphTool "codegraph_callers" @("callers", "ToolRegistry::execute", "-p", $ProjectRoot, "-j", "-l", "3")
Assert-True ($r.ExitCode -eq 0) "codegraph_callers"
Assert-True ($r.Stdout.Length -gt 10) "callers has output"

$r = Invoke-CodegraphTool "codegraph_callees" @("callees", "ToolRegistry::new", "-p", $ProjectRoot, "-j", "-l", "3")
Assert-True ($r.ExitCode -eq 0) "codegraph_callees"

$r = Invoke-CodegraphTool "codegraph_impact" @("impact", "ToolRegistry::execute", "-p", $ProjectRoot, "-j", "-d", "1")
Assert-True ($r.ExitCode -eq 0) "codegraph_impact"
try {
    $impact = $r.Stdout | ConvertFrom-Json
    Assert-True ($impact.symbol -match "ToolRegistry") "impact has symbol"
    Assert-True ($impact.affected.Count -ge 1) "impact lists affected nodes"
} catch {
    Assert-True $false "impact JSON parse" $_.Exception.Message
}

$r = Invoke-CodegraphTool "codegraph_files" @("files", "-p", $ProjectRoot, "--filter", "src-tauri/src/tool", "--format", "flat")
Assert-True ($r.ExitCode -eq 0) "codegraph_files"
Assert-True ($r.Stdout -match "tool") "files lists tool directory"

$r = Invoke-CodegraphTool "codegraph_context" @(
    "context", "tool registry builtins registration",
    "-p", $ProjectRoot, "--max-nodes", "5", "--max-code", "2", "--no-code"
)
Assert-True ($r.ExitCode -eq 0) "codegraph_context"
Assert-True ($r.Stdout.Length -gt 50) "context has markdown output"

# Missing query should fail at Rust layer; CLI with empty query:
$r = Invoke-CodegraphTool "codegraph_search_empty" @("query", "", "-p", $ProjectRoot)
# CLI may still exit 0 with empty results — Rust validates required param before spawn

# Lazy load mode in mod.rs
$toolMod = Get-Content (Join-Path $ProjectRoot "src-tauri\src\tool\mod.rs") -Raw
Assert-True ($toolMod -match "codegraph_search") "default_load_mode lists codegraph_search as lazy"

Write-Host "`n=== Summary: $Passed passed, $Failed failed ===" -ForegroundColor Cyan
if ($Failed -gt 0) { exit 1 }
exit 0
