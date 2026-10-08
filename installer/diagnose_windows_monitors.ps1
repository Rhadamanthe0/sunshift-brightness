param([switch] $CheckBrightness)
# Read-only by default; the optional check preserves the current native values.
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class SsbMonitorDiagnostics {
    [StructLayout(LayoutKind.Sequential)]
    private struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct MonitorInfo {
        public uint Size;
        public Rect Monitor, Work;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string Device;
    }
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct DisplayDevice {
        public uint Size;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string Name;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string Description;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string Id;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string Key;
    }
    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    private struct DisplayBrightness {
        public byte Policy, AC, DC;
    }
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct PhysicalMonitor {
        public IntPtr Handle;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string Description;
    }
    private delegate bool MonitorCallback(IntPtr monitor, IntPtr dc, IntPtr rect, IntPtr data);
    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr rect, MonitorCallback callback, IntPtr data);
    [DllImport("user32.dll", EntryPoint = "GetMonitorInfoW", ExactSpelling = true, CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
    [DllImport("user32.dll", EntryPoint = "EnumDisplayDevicesW", ExactSpelling = true, CharSet = CharSet.Unicode)]
    private static extern bool EnumDisplayDevices(string adapter, uint index, ref DisplayDevice device, uint flags);
    [DllImport("kernel32.dll", EntryPoint = "CreateFileW", ExactSpelling = true, CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern Microsoft.Win32.SafeHandles.SafeFileHandle CreateFile(string path, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", EntryPoint = "DeviceIoControl", SetLastError = true)]
    private static extern bool QueryBrightness(Microsoft.Win32.SafeHandles.SafeFileHandle file, uint code, IntPtr input, uint inputSize, out DisplayBrightness output, uint outputSize, out uint returned, IntPtr overlapped);
    [DllImport("kernel32.dll", EntryPoint = "DeviceIoControl", SetLastError = true)]
    private static extern bool SetBrightness(Microsoft.Win32.SafeHandles.SafeFileHandle file, uint code, ref DisplayBrightness input, uint inputSize, IntPtr output, uint outputSize, out uint returned, IntPtr overlapped);
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
    private static List<DisplayDevice> Devices(IntPtr monitor, List<string> lines) {
        var result = new List<DisplayDevice>();
        var info = new MonitorInfo { Size = (uint)Marshal.SizeOf(typeof(MonitorInfo)) };
        if (!GetMonitorInfo(monitor, ref info)) {
            lines.Add(Failure("Monitor information", Marshal.GetLastWin32Error()));
            return result;
        }
        for (uint index = 0; index < 32; index++) {
            var device = new DisplayDevice { Size = (uint)Marshal.SizeOf(typeof(DisplayDevice)) };
            if (!EnumDisplayDevices(info.Device, index, ref device, 1)) break;
            if ((device.Flags & 1) != 0) result.Add(device);
        }
        return result;
    }
    private static string InstanceId(string path) {
        if (!path.StartsWith(@"\\?\")) return "";
        var parts = path.Substring(4).Split('#');
        return parts.Length == 4 ? String.Join(@"\", parts, 0, 3) : "";
    }
    private static bool CheckInternal(string path, List<string> lines) {
        // Same IOCTL interface as brightness::blocking::windows. Preserve both
        // AC/DC values and the policy; don't replace DC with the active AC value.
        using (var file = CreateFile(path, 0xC0000000, 3, IntPtr.Zero, 3, 0, IntPtr.Zero)) {
            if (file.IsInvalid) {
                lines.Add(Failure("Internal monitor open", Marshal.GetLastWin32Error()));
                return false;
            }
            DisplayBrightness before, after;
            uint returned;
            uint size = (uint)Marshal.SizeOf(typeof(DisplayBrightness));
            if (!QueryBrightness(file, 2294936, IntPtr.Zero, 0, out before, size, out returned, IntPtr.Zero)) {
                lines.Add(Failure("Internal IOCTL brightness read", Marshal.GetLastWin32Error()));
                return false;
            }
            if (returned != size || before.AC > 100 || before.DC > 100 ||
                (before.Policy != 1 && before.Policy != 2)) {
                lines.Add("Internal IOCTL brightness response is invalid.");
                return false;
            }
            if (!SetBrightness(file, 2294940, ref before, size, IntPtr.Zero, 0, out returned, IntPtr.Zero)) {
                lines.Add(Failure("Internal IOCTL same-value write", Marshal.GetLastWin32Error()));
                return false;
            }
            if (!QueryBrightness(file, 2294936, IntPtr.Zero, 0, out after, size, out returned, IntPtr.Zero)) {
                lines.Add(Failure("Internal IOCTL brightness readback", Marshal.GetLastWin32Error()));
                return false;
            }
            if (returned != size || before.Policy != after.Policy || before.AC != after.AC || before.DC != after.DC) {
                lines.Add("Internal brightness changed during the roundtrip.");
                return false;
            }
            lines.Add("Internal IOCTL same-value roundtrip passed; AC/DC brightness unchanged.");
            return true;
        }
    }
    public static string[] Read(bool checkBrightness, string[] internalIds) {
        var lines = new List<string>();
        var internalMonitors = new HashSet<string>(internalIds, StringComparer.OrdinalIgnoreCase);
        int logical = 0;
        int checkedMonitors = 0;
        MonitorCallback callback = delegate(IntPtr monitor, IntPtr dc, IntPtr rect, IntPtr data) {
            logical++;
            var devices = Devices(monitor, lines);
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
                        for (int index = 0; index < physical.Length; index++) {
                            var item = physical[index];
                            if (devices.Count != physical.Length) {
                                lines.Add("Display device / physical monitor mapping is unavailable.");
                                continue;
                            }
                            var device = devices[index];
                            if (internalMonitors.Contains(InstanceId(device.Id))) {
                                lines.Add("Brightness interface: internal IOCTL");
                                if (CheckInternal(device.Id, lines)) checkedMonitors++;
                                continue;
                            }
                            lines.Add("Brightness interface: external DDC/CI");
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
        if (checkBrightness) lines.Add("Brightness checked monitors: " + checkedMonitors);
        return lines.ToArray();
    }
}
'@
Write-Output "Interactive session: $((Get-Process -Id $PID).SessionId); active console session: $([SsbMonitorDiagnostics]::WTSGetActiveConsoleSessionId())"
$internalIds = @()
try {
    $connections = Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorConnectionParams -OperationTimeoutSec 5
    $internalIds = @($connections | Where-Object { $_.VideoOutputTechnology -eq 2147483648 } |
        ForEach-Object { $_.InstanceName -replace '_\d+$', '' })
    $connections | ForEach-Object { Write-Output "Display connection: $($_.InstanceName); technology=$($_.VideoOutputTechnology)" }
} catch {
    Write-Output "Unable to classify internal monitors: $($_.Exception.Message)"
}
$lines = [SsbMonitorDiagnostics]::Read($CheckBrightness.IsPresent, [string[]]$internalIds)
$lines
if ($CheckBrightness -and -not ($lines -match '^Brightness checked monitors: [1-9][0-9]*$')) {
    throw 'No physical monitor passed the brightness same-value roundtrip.'
}
