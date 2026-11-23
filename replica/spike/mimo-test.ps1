$cfg = Get-Content "$env:USERPROFILE\.config\opencode\opencode.json" -Raw | ConvertFrom-Json
$key = $cfg.provider.mimo.options.apiKey
$base = $cfg.provider.mimo.options.baseURL
$tmp = Join-Path $env:TEMP "mimo-req.json"

function Test-TTFT($model, $label) {
  $body = @{model=$model; stream=$true; max_tokens=64; messages=@(@{role="user"; content="Reply with one short sentence about the weather."})} | ConvertTo-Json -Compress
  [System.IO.File]::WriteAllText($tmp, $body, (New-Object System.Text.UTF8Encoding($false)))
  for ($i=1; $i -le 2; $i++) {
    $r = curl.exe -s -o NUL -w "%{time_starttransfer} %{time_total} %{http_code}" -X POST "$base/chat/completions" -H "Content-Type: application/json" -H "Authorization: Bearer $key" -d "@$tmp"
    "$label run$i ttfb_s total_s http: $r"
  }
}
Test-TTFT "mimo-v2.6-flash" "flash"
Test-TTFT "mimo-v2.6-pro" "pro"

# vision capability test
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap(8,8)
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::Red); $g.Dispose()
$ms = New-Object System.IO.MemoryStream; $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
$b64 = [Convert]::ToBase64String($ms.ToArray())
$vtmp = Join-Path $env:TEMP "mimo-vision.json"
$vbody = @{model="mimo-v2.6-flash"; max_tokens=64; messages=@(@{role="user"; content=@(
  @{type="text"; text="What color is this image? Answer with one word."},
  @{type="image_url"; image_url=@{url="data:image/png;base64,$b64"}})})} | ConvertTo-Json -Compress -Depth 6
[System.IO.File]::WriteAllText($vtmp, $vbody, (New-Object System.Text.UTF8Encoding($false)))
$vr = curl.exe -s -X POST "$base/chat/completions" -H "Content-Type: application/json" -H "Authorization: Bearer $key" -d "@$vtmp"
"VISION_HTTP_BODY: " + $vr.Substring(0, [Math]::Min(600, $vr.Length))
