# Reads Windows endpoint volume/mute state without changing audio settings.
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class GoXLREndpoints {
    [ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class Enumerator {}
    [ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IEnumerator {
        int EnumAudioEndpoints(int flow, uint state, out ICollection devices);
        int GetDefaultAudioEndpoint(int flow, int role, out IDevice device);
        int GetDevice([MarshalAs(UnmanagedType.LPWStr)] string id, out IDevice device);
    }
    [ComImport, Guid("0BD7A1BE-7A1A-44DB-8397-C0A83B2E8EC1"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface ICollection { int GetCount(out uint count); int Item(uint index, out IDevice device); }
    [ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IDevice {
        int Activate(ref Guid id, uint context, IntPtr parameters, [MarshalAs(UnmanagedType.IUnknown)] out object instance);
        int OpenPropertyStore(uint access, out IntPtr store);
        int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
        int GetState(out uint state);
    }
    [ComImport, Guid("5CDF2C82-841E-4546-9722-0CF74078229A"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IVolume {
        int RegisterControlChangeNotify(IntPtr callback); int UnregisterControlChangeNotify(IntPtr callback);
        int GetChannelCount(out uint count);
        int SetMasterVolumeLevel(float level, IntPtr context); int SetMasterVolumeLevelScalar(float level, IntPtr context);
        int GetMasterVolumeLevel(out float level); int GetMasterVolumeLevelScalar(out float level);
        int SetChannelVolumeLevel(uint channel, float level, IntPtr context);
        int SetChannelVolumeLevelScalar(uint channel, float level, IntPtr context);
        int GetChannelVolumeLevel(uint channel, out float level); int GetChannelVolumeLevelScalar(uint channel, out float level);
        int SetMute([MarshalAs(UnmanagedType.Bool)] bool mute, IntPtr context);
        int GetMute([MarshalAs(UnmanagedType.Bool)] out bool mute);
    }
    public static string Inspect(string id) {
        var enumerator = (IEnumerator)new Enumerator();
        IDevice device = null;
        object instance = null;
        try {
            Marshal.ThrowExceptionForHR(enumerator.GetDevice(id, out device));
            var iid = typeof(IVolume).GUID;
            Marshal.ThrowExceptionForHR(device.Activate(ref iid, 23, IntPtr.Zero, out instance));
            var volume = (IVolume)instance;
            float scalar; bool mute;
            Marshal.ThrowExceptionForHR(volume.GetMasterVolumeLevelScalar(out scalar));
            Marshal.ThrowExceptionForHR(volume.GetMute(out mute));
            return String.Format("Level={0:F1}% Muted={1}", scalar * 100, mute);
        } finally {
            if (instance != null) Marshal.ReleaseComObject(instance);
            if (device != null) Marshal.ReleaseComObject(device);
            Marshal.ReleaseComObject(enumerator);
        }
    }
}
'@
foreach ($flow in 'Capture', 'Render') {
    $root = "HKLM:/SOFTWARE/Microsoft/Windows/CurrentVersion/MMDevices/Audio/$flow"
    foreach ($endpoint in Get-ChildItem $root) {
        $properties = Get-ItemProperty (Join-Path $endpoint.PSPath 'Properties')
        $name = $properties.'{a45c254e-df1c-4efd-8020-67d146a850e0},2'
        $interface = $properties.'{b3f8fa53-0004-438e-9003-51a46e139bfc},6'
        if ($interface -notmatch 'GoXLR') { continue }
        if ((Get-ItemProperty $endpoint.PSPath).DeviceState -ne 1) { continue }
        $prefix = if ($flow -eq 'Capture') { '0.0.1' } else { '0.0.0' }
        [pscustomobject]@{ Flow = $flow; Name = $name; State = [GoXLREndpoints]::Inspect("{$prefix.00000000}.$($endpoint.PSChildName)") }
    }
}
