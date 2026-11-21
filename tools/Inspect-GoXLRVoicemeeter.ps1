# Read the existing Voicemeeter routing and levels without changing audio settings.
param([string]$OutputPath)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class GoXLRVoicemeeter {
    const string Dll = "C:/Program Files (x86)/VB/Voicemeeter/VoicemeeterRemote64.dll";
    [DllImport(Dll)] public static extern int VBVMR_Login();
    [DllImport(Dll)] public static extern int VBVMR_Logout();
    [DllImport(Dll)] public static extern int VBVMR_IsParametersDirty();
    [DllImport(Dll, CharSet=CharSet.Ansi)] static extern int VBVMR_GetParameterFloat(string key, out float value);
    [DllImport(Dll, CharSet=CharSet.Unicode)] static extern int VBVMR_GetParameterStringW([MarshalAs(UnmanagedType.LPStr)] string key, StringBuilder value);
    public static float Number(string key) { float value; if (VBVMR_GetParameterFloat(key,out value)!=0) throw new Exception("Cannot read " + key); return value; }
    public static string Text(string key) { var value=new StringBuilder(512); if(VBVMR_GetParameterStringW(key,value)!=0) throw new Exception("Cannot read " + key); return value.ToString(); }
}
'@
if ([GoXLRVoicemeeter]::VBVMR_Login() -ne 0) { throw 'Voicemeeter is not running or login failed.' }
try {
    [void][GoXLRVoicemeeter]::VBVMR_IsParametersDirty()
    $rows = foreach ($index in 0..7) {
        $row = [ordered]@{ Index=$index }
        foreach ($key in 'Gain','Mute','A1','A2','A3','B1','B2','B3') {
            $row[$key] = [GoXLRVoicemeeter]::Number("Strip[$index].$key")
        }
        [pscustomobject]$row
    }
    $result = [ordered]@{ ReadOnly=$true; Strips=@($rows); A1Device=[GoXLRVoicemeeter]::Text('Bus[0].device.name'); A1Gain=[GoXLRVoicemeeter]::Number('Bus[0].Gain'); A1Mute=[GoXLRVoicemeeter]::Number('Bus[0].Mute'); A1Eq=[GoXLRVoicemeeter]::Number('Bus[0].EQ.on') }
    $json = $result | ConvertTo-Json -Depth 5
    if ($OutputPath) {
        if (Test-Path -LiteralPath $OutputPath) { throw 'Choose a new snapshot file.' }
        $json | Set-Content -LiteralPath $OutputPath -Encoding UTF8
    }
    $json
} finally { [void][GoXLRVoicemeeter]::VBVMR_Logout() }
