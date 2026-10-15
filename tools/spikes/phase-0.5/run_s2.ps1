param([string]$Mode, [string]$Arg1 = "", [string]$Arg2 = "", [int]$HardLimitSeconds = 90)
# Runs s2_hang in its own process (software adapter only, see the program) and kills THAT process by id if it is still alive after the hard limit.
$sp = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $sp "target\debug\s2_hang.exe"
$arguments = @($Mode)
if ($Arg1 -ne "") { $arguments += $Arg1 }
if ($Arg2 -ne "") { $arguments += $Arg2 }
$out = Join-Path $sp "s2_$($Mode)_$($Arg1).log"
$err = Join-Path $sp "s2_$($Mode)_$($Arg1).err"
$p = Start-Process -FilePath $exe -ArgumentList $arguments -NoNewWindow -PassThru -RedirectStandardOutput $out -RedirectStandardError $err
$started = Get-Date
# While it runs, sample the CPU time it has used (to see whether runaway work goes on after the recovery).
$samples = @()
while (-not $p.HasExited -and ((Get-Date) - $started).TotalSeconds -lt $HardLimitSeconds) {
    Start-Sleep -Milliseconds 2000
    try { $p.Refresh(); $samples += "{0:N1}s wall: cpu {1:N1} s" -f ((Get-Date) - $started).TotalSeconds, $p.TotalProcessorTime.TotalSeconds } catch {}
}
if (-not $p.WaitForExit(1000)) {
    "HARD LIMIT of $HardLimitSeconds s reached: the process (pid $($p.Id)) was still running; stopping that pid"
    Stop-Process -Id $p.Id -Force
    $p.WaitForExit(10000) | Out-Null
} else {
    "process exited with code $($p.ExitCode) after $([math]::Round(((Get-Date) - $started).TotalSeconds, 2)) s"
}
Get-Content $out
"--- cpu samples every 2 s:"
$samples
$e = Get-Content $err -TotalCount 15
if ($e) { "--- stderr:"; $e }
