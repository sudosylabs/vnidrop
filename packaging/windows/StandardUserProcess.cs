using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
using Microsoft.Win32.SafeHandles;

public static class StandardUserProcess
{
    [StructLayout(LayoutKind.Sequential)]
    struct MandatoryLabel { public IntPtr Sid; public uint Attributes; }
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct StartupInfo
    {
        public int Size;
        public string Reserved, Desktop, Title;
        public uint X, Y, XSize, YSize, XCountChars, YCountChars, FillAttribute, Flags;
        public short ShowWindow, ReservedSize;
        public IntPtr ReservedBytes, StdInput, StdOutput, StdError;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct ProcessInfo { public IntPtr Process, Thread; public uint ProcessId, ThreadId; }
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool OpenProcessToken(IntPtr process, uint access, out SafeAccessTokenHandle token);
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool GetTokenInformation(SafeAccessTokenHandle token, int kind, IntPtr data, int size, out int needed);
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool CreateRestrictedToken(SafeAccessTokenHandle existing, uint flags,
        uint disabledCount, IntPtr disabled, uint deletedCount, IntPtr deleted,
        uint restrictedCount, IntPtr restricted, out SafeAccessTokenHandle token);
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool SetTokenInformation(SafeAccessTokenHandle token, int kind, ref MandatoryLabel label, int size);
    [DllImport("advapi32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool CreateProcessAsUser(SafeAccessTokenHandle token, string application, StringBuilder command,
        IntPtr processAttributes, IntPtr threadAttributes, bool inheritHandles, uint flags,
        IntPtr environment, string directory, ref StartupInfo startup, out ProcessInfo process);
    [DllImport("kernel32.dll")]
    static extern bool CloseHandle(IntPtr handle);

    public static int IntegrityLevel(int processId)
    {
        using (var process = Process.GetProcessById(processId))
        {
            SafeAccessTokenHandle token;
            if (!OpenProcessToken(process.Handle, 0x0008, out token)) throw new Win32Exception();
            using (token)
            {
                int size;
                GetTokenInformation(token, 25, IntPtr.Zero, 0, out size);
                if (size == 0) throw new Win32Exception();
                var data = Marshal.AllocHGlobal(size);
                try
                {
                    if (!GetTokenInformation(token, 25, data, size, out size)) throw new Win32Exception();
                    var sid = new SecurityIdentifier(Marshal.ReadIntPtr(data)).Value;
                    return int.Parse(sid.Substring(sid.LastIndexOf('-') + 1));
                }
                finally { Marshal.FreeHGlobal(data); }
            }
        }
    }

    public static Process Start(string executable, string arguments, string directory)
    {
        using (var current = Process.GetCurrentProcess())
        {
            if (IntegrityLevel(current.Id) < 0x3000)
                return Process.Start(new ProcessStartInfo(executable, arguments) {
                    WorkingDirectory = directory, UseShellExecute = false, CreateNoWindow = true
                });

            SafeAccessTokenHandle existing;
            if (!OpenProcessToken(current.Handle, 0x008B, out existing)) throw new Win32Exception();
            using (existing)
            {
                SafeAccessTokenHandle restricted;
                // Hosted runners have UAC disabled and no linked standard-user token.
                // Remove administrator groups/privileges, retaining the same user/profile.
                if (!CreateRestrictedToken(existing, 0x5, 0, IntPtr.Zero, 0, IntPtr.Zero, 0, IntPtr.Zero, out restricted))
                    throw new Win32Exception();
                using (restricted)
                {
                    var sid = new SecurityIdentifier("S-1-16-8192");
                    var bytes = new byte[sid.BinaryLength];
                    sid.GetBinaryForm(bytes, 0);
                    var data = Marshal.AllocHGlobal(bytes.Length);
                    try
                    {
                        Marshal.Copy(bytes, 0, data, bytes.Length);
                        var label = new MandatoryLabel { Sid = data, Attributes = 0x20 };
                        if (!SetTokenInformation(restricted, 25, ref label, Marshal.SizeOf(label) + bytes.Length))
                            throw new Win32Exception();
                        var startup = new StartupInfo { Size = Marshal.SizeOf(typeof(StartupInfo)), Flags = 1 };
                        ProcessInfo child;
                        if (!CreateProcessAsUser(restricted, executable, new StringBuilder("\"" + executable + "\" " + arguments),
                            IntPtr.Zero, IntPtr.Zero, false, 0x08000000, IntPtr.Zero, directory, ref startup, out child))
                            throw new Win32Exception();
                        try
                        {
                            var process = Process.GetProcessById((int)child.ProcessId);
                            // Keep a managed handle open before releasing the creation handle,
                            // so .NET Framework can read the exit code after the child exits.
                            var handle = process.Handle;
                            return process;
                        }
                        finally { CloseHandle(child.Thread); CloseHandle(child.Process); }
                    }
                    finally { Marshal.FreeHGlobal(data); }
                }
            }
        }
    }
}
