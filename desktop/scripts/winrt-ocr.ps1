# Runs Windows.Media.Ocr (en-US) over each *.png in -Dir and prints
# "== <file>" followed by one "[<line>]" per OCR line: the engine the desktop
# app reads boss-loot labels with, for re-reading committed capture fixtures.
#
# From WSL, at the checkout root (copies the script and fixture to %TEMP%):
#   D="$(wslpath "$(cmd.exe /c 'echo %TEMP%' 2>/dev/null | tr -d '\r')")/poe414-probe"; mkdir -p "$D" && cp desktop/scripts/winrt-ocr.ps1 desktop/src-tauri/tests/fixtures/boss-loot/catarina-drops-2026-11-18.png "$D"/ && powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$D")\winrt-ocr.ps1" -Dir "$(wslpath -w "$D")"; rm -rf "$D"
param(
	[Parameter(Mandatory = $true)][string]$Dir
)
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Storage.StorageFile, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Globalization.Language, Windows.Foundation, ContentType = WindowsRuntime]

$asTaskGeneric = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
	$_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
	$_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
} | Select-Object -First 1

function Await($operation, [Type]$resultType) {
	$task = $asTaskGeneric.MakeGenericMethod($resultType).Invoke($null, @($operation))
	$task.Wait() | Out-Null
	$task.Result
}

$engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage([Windows.Globalization.Language]::new('en-US'))
if ($null -eq $engine) {
	throw 'en-US OCR language is not installed'
}

Get-ChildItem -Path $Dir -Filter '*.png' | Sort-Object Name | ForEach-Object {
	$file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync($_.FullName)) ([Windows.Storage.StorageFile])
	$stream = Await ($file.OpenReadAsync()) ([Windows.Storage.Streams.IRandomAccessStreamWithContentType])
	$decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
	$bitmap = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
	$result = Await ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
	Write-Output "== $($_.Name)"
	foreach ($line in $result.Lines) {
		Write-Output "[$($line.Text)]"
	}
	$stream.Dispose()
}
