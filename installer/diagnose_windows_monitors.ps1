param([switch] $CheckBrightness)
# Read-only by default; the optional DDC check writes the exact current value.
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class SsbMonitorDiagnostics {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct PhysicalMonitor {
        public IntPtr Handle;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string Description;
    }
    private delegate bool MonitorCallback(IntPtr monitor, IntPtr dc, IntPtr rect, IntPtr data);
    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr rect, MonitorCallback callback, IntPtr data);
    [DllImport("dxva2.dll", SetLastError = true)]
    private static extern bool GetNumberOfPhysicalMonitorsFromHMONITOR(IntPtr monitor, out uint count);
    [DllImport("dxva2.dll", SetLastError = true)]
    private static extern bool GetPhysicalMonitorsFromHMONITOR(IntPtr monitor, uint count, [Out] PhysicalMonitor[] monitors);
    [DllImport("dxva2.dll", SetLastError = true)]
    private static extern bool DestroyPhysicalMonitors(uint count, PhysicalMonitor[] monitors);
    [DllImport("dxva2.dll", SetLastError = true)]
    private static extern bool GetMonitorBrightness(IntPtr monitor, out uint minimum, out uint current, out uint maximum);
    [DllImport("dxva2.dll", SetLastError = true)]
    private static extern bool SetMonitorBrightness(IntPtr monitor, uint value);
    [DllImport("kernel32.dll")]
    public static extern uint WTSGetActiveConsoleSessionId();
    private static string Failure(string operation, int code) {
        return operation + " failed: 0x" + unchecked((uint)code).ToString("X8") +
            " (" + new System.ComponentModel.Win32Exception(code).Message + ")";
    }
    public static string[] Read(bool checkBrightness) {
        var lines = new List<string>();
        int logical = 0;
        int checkedMonitors = 0;
        MonitorCallback callback = delegate(IntPtr monitor, IntPtr dc, IntPtr rect, IntPtr data) {
            logical++;
            uint count;
            bool counted = GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, out count);
            int countError = counted ? 0 : Marshal.GetLastWin32Error();
            lines.Add("Logical monitor " + logical + ": physical count=" + count + ", count error=" + countError);
            if (!counted || count > 32) return true;
            var physical = new PhysicalMonitor[count];
            bool opened = GetPhysicalMonitorsFromHMONITOR(monitor, count, physical);
            int openError = opened ? 0 : Marshal.GetLastWin32Error();
            lines.Add("Physical monitor handles: success=" + opened + ", error=" + openError);
            if (opened && count > 0) {
                try {
                    foreach (var item in physical) {
                        lines.Add("Physical monitor: " + item.Description);
                    }
                    if (checkBrightness) {
                        foreach (var item in physical) {
                            uint minimum, current, maximum;
                            if (!GetMonitorBrightness(item.Handle, out minimum, out current, out maximum)) {
                                lines.Add(Failure("DDC brightness read", Marshal.GetLastWin32Error()));
                                continue;
                            }
                            if (minimum > current || current > maximum) {
                                lines.Add("DDC brightness range is invalid.");
                                continue;
                            }
                            if (!SetMonitorBrightness(item.Handle, current)) {
                                lines.Add(Failure("DDC same-value write", Marshal.GetLastWin32Error()));
                                continue;
                            }
                            uint readback;
                            if (!GetMonitorBrightness(item.Handle, out minimum, out readback, out maximum)) {
                                lines.Add(Failure("DDC brightness readback", Marshal.GetLastWin32Error()));
                                continue;
                            }
                            if (readback != current) {
                                lines.Add("DDC brightness changed during the roundtrip.");
                                continue;
                            }
                            checkedMonitors++;
                            lines.Add("DDC same-value roundtrip passed; brightness unchanged.");
                        }
                    }
                } finally { DestroyPhysicalMonitors(count, physical); }
            }
            return true;
        };
        bool enumerated = EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, callback, IntPtr.Zero);
        int enumerationError = enumerated ? 0 : Marshal.GetLastWin32Error();
        lines.Add("Logical monitor enumeration: count=" + logical + ", error=" + enumerationError);
        if (checkBrightness) lines.Add("DDC checked monitors: " + checkedMonitors);
        return lines.ToArray();
    }
}
'@
Write-Output "Interactive session: $((Get-Process -Id $PID).SessionId); active console session: $([SsbMonitorDiagnostics]::WTSGetActiveConsoleSessionId())"
$lines = [SsbMonitorDiagnostics]::Read($CheckBrightness.IsPresent)
$lines
if ($CheckBrightness -and -not ($lines -match '^DDC checked monitors: [1-9][0-9]*$')) {
    throw 'No physical monitor passed the DDC same-value roundtrip.'
}
