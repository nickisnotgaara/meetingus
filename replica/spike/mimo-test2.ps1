$cfg = Get-Content "$env:USERPROFILE\.config\opencode\opencode.json" -Raw | ConvertFrom-Json
$key = $cfg.provider.mimo.options.apiKey
$base = $cfg.provider.mimo.options.baseURL
$tmp = Join-Path $env:TEMP "mimo-req-big.json"

$block = @"
[00:00:12] THEM: Thanks everyone for joining. Let's start with the budget breakdown for Q4.
[00:00:20] ME: Sure. Revenue grew fourteen percent while operating costs stayed flat.
[00:00:31] THEM: And the billing service migration?
[00:00:35] ME: The engineering team shipped the new billing service two weeks ahead of schedule.
[00:00:44] THEM: What about churn? The board asked about retention numbers.
[00:00:49] ME: Customer churn dropped to three percent, the lowest we have seen in two years.
[00:01:02] THEM: Marketing wants europe next quarter. Any blockers?
[00:01:07] ME: Legal still needs to approve the data processing agreements before we announce anything.
[00:01:18] THEM: OK, schedule a follow up with compliance then. Infrastructure status?
[00:01:23] ME: The migration to the new cluster is eighty percent complete, on track for month end.
[00:01:35] THEM: Good. One more thing - the customer advisory board wants a preview in november.
[00:01:41] ME: We can do that if the cluster lands on time. I will confirm with the team friday.
"@
$transcript = $block * 6
$question = "Here is the live transcript of the meeting so far:`n" + $transcript + "`n`nQuestion: what should I say about the europe expansion blocker? Answer in two short sentences."

function Test-Big($model, $label, $q, $tfile) {
  $body = @{model=$model; stream=$true; max_tokens=64; messages=@(
    @{role="system"; content="You are an assistant helping during a live meeting. Answer briefly."},
    @{role="user"; content=$q})} | ConvertTo-Json -Compress -Depth 5
  [System.IO.File]::WriteAllText($tfile, $body, (New-Object System.Text.UTF8Encoding($false)))
  "$label prompt_chars: $($body.Length)"
  for ($i=1; $i -le 2; $i++) {
    $r = curl.exe -s -o NUL -w "%{time_starttransfer} %{time_total} %{http_code}" -X POST "$base/chat/completions" -H "Content-Type: application/json" -H "Authorization: Bearer $key" -d "@$tfile"
    "$label big-prompt run$i ttfb_s total_s http: $r"
  }
}
Test-Big "mimo-v2.6-flash" "flash" $question $tmp
Test-Big "mimo-v2.6-pro" "pro" $question $tmp
