# ccam installer for Windows.
#
#   irm https://raw.githubusercontent.com/jatinkrmalik/ccam/main/install.ps1 | iex
#
# Downloads the latest release binary, verifies its checksum, runs `ccam setup`
# and puts ~\.ccam\bin at the front of your user PATH.
#
# Options (environment variables):
#   CCAM_VERSION=v0.4.0     install a specific release instead of the latest
#   CCAM_NO_MODIFY_PATH=1   do not change the user PATH
#   CCAM_HOME=C:\some\dir   install somewhere other than ~\.ccam

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = 'jatinkrmalik/ccam'
$target = 'x86_64-pc-windows-msvc'

$version = $env:CCAM_VERSION
if (-not $version) {
    $request = [Net.HttpWebRequest]::Create("https://github.com/$repo/releases/latest")
    $request.AllowAutoRedirect = $false
    $request.UserAgent = 'ccam-installer'
    $response = $request.GetResponse()
    $location = $response.Headers['Location']
    $response.Close()
    if (-not $location) { throw 'ccam install: could not find the latest release' }
    $version = ($location -split '/')[-1]
}

$name = "ccam-$version-$target"
$archive = "$name.zip"
$base = "https://github.com/$repo/releases/download/$version"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("ccam-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    Write-Host "Downloading ccam $version ($target)"
    Invoke-WebRequest -UseBasicParsing "$base/$archive" -OutFile (Join-Path $tmp $archive)
    Invoke-WebRequest -UseBasicParsing "$base/$archive.sha256" -OutFile (Join-Path $tmp "$archive.sha256")

    $expected = ((Get-Content (Join-Path $tmp "$archive.sha256") -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash (Join-Path $tmp $archive) -Algorithm SHA256).Hash
    if ($actual -ne $expected) { throw 'ccam install: checksum did not match; nothing was installed' }

    Expand-Archive -Path (Join-Path $tmp $archive) -DestinationPath $tmp -Force
    & (Join-Path $tmp "$name\ccam.exe") setup | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'ccam install: ccam setup failed' }
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

$ccamHome = if ($env:CCAM_HOME) { $env:CCAM_HOME } else { Join-Path $env:USERPROFILE '.ccam' }
$bin = Join-Path $ccamHome 'bin'
Write-Host "Installed ccam $version in $bin"

if ($env:CCAM_NO_MODIFY_PATH) {
    Write-Host "Add this folder to the front of your PATH: $bin"
}
else {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @($userPath -split ';' | Where-Object { $_ })
    if ($parts -contains $bin) {
        Write-Host 'PATH is already set.'
    }
    else {
        [Environment]::SetEnvironmentVariable('Path', (@($bin) + $parts) -join ';', 'User')
        Write-Host "Added $bin to your user PATH."
    }
    if (-not (($env:PATH -split ';') -contains $bin)) { $env:PATH = "$bin;$env:PATH" }
}

Write-Host ''
Write-Host 'Open a new terminal, then:'
Write-Host '  ccam account add work'
Write-Host '  ccam login work'
Write-Host '  ccam use work'
