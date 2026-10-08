# Read-only diagnostics in the same desktop session as the native GUI test.
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
    [DllImport("kernel32.dll")]
    public static extern uint WTSGetActiveConsoleSessionId();
    public static string[] Read() {
        var lines = new List<string>();
        int logical = 0;
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
            if (opened && count > 0) DestroyPhysicalMonitors(count, physical);
            return true;
        };
        bool enumerated = EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, callback, IntPtr.Zero);
        int enumerationError = enumerated ? 0 : Marshal.GetLastWin32Error();
        lines.Add("Logical monitor enumeration: count=" + logical + ", error=" + enumerationError);
        return lines.ToArray();
    }
}
'@
Write-Output "Interactive session: $((Get-Process -Id $PID).SessionId); active console session: $([SsbMonitorDiagnostics]::WTSGetActiveConsoleSessionId())"
[SsbMonitorDiagnostics]::Read()
