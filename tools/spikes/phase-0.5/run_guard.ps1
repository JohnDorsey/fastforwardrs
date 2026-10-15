param([string]$Shader, [string]$Cap, [int]$HardLimitSeconds = 60)
# Runs s2_guard in its own process and kills THAT process by id when it is still alive after the hard limit.
$sp = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $sp
$out = Join-Path $sp "guard_$($Shader)_$($Cap).log"
$err = Join-Path $sp "guard_$($Shader)_$($Cap).err"
$p = Start-Process -FilePath (Join-Path $sp "target\debug\s2_guard.exe") -ArgumentList @($Shader, $Cap) -NoNewWindow -PassThru -RedirectStandardOutput $out -RedirectStandardError $err
$started = Get-Date
if (-not $p.WaitForExit($HardLimitSeconds * 1000)) {
    "[$Shader, cap $Cap] STILL RUNNING after $HardLimitSeconds s: stopping pid $($p.Id)"
    Stop-Process -Id $p.Id -Force
    $p.WaitForExit(10000) | Out-Null
} else {
    "[$Shader, cap $Cap] finished in $([math]::Round(((Get-Date) - $started).TotalSeconds, 2)) s"
}
Get-Content $out
$e = Get-Content $err -TotalCount 8
if ($e) { "stderr:"; $e }
