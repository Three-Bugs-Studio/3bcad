# Signs the files given with the Authenticode certificate in WINDOWS_CERTIFICATE (a base64 .pfx)
# and WINDOWS_CERTIFICATE_PASSWORD, timestamped (RFC 3161) so the signature outlives the
# certificate, then checks the signatures. Run by the Package workflow; see RELEASING.md.
$ErrorActionPreference = 'Stop'
$signtool = (Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\signtool.exe" |
    Sort-Object FullName | Select-Object -Last 1).FullName
$pfx = Join-Path $env:RUNNER_TEMP 'codesign.pfx'
[IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
& $signtool sign /f $pfx /p $env:WINDOWS_CERTIFICATE_PASSWORD /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $args
$signed = $LASTEXITCODE
Remove-Item $pfx
if ($signed) { exit $signed }
& $signtool verify /pa /v $args
exit $LASTEXITCODE
