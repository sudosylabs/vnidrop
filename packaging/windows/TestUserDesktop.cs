using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Principal;

public sealed class TestUserDesktop : IDisposable
{
    [DllImport("user32.dll")]
    static extern IntPtr GetProcessWindowStation();
    [DllImport("user32.dll")]
    static extern IntPtr GetThreadDesktop(uint threadId);
    [DllImport("kernel32.dll")]
    static extern uint GetCurrentThreadId();
    [DllImport("user32.dll", SetLastError = true)]
    static extern bool GetUserObjectSecurity(IntPtr handle, ref uint information, byte[] descriptor, uint length, out uint needed);
    [DllImport("user32.dll", SetLastError = true)]
    static extern bool SetUserObjectSecurity(IntPtr handle, ref uint information, byte[] descriptor);

    readonly IntPtr station = GetProcessWindowStation();
    readonly IntPtr desktop = GetThreadDesktop(GetCurrentThreadId());
    byte[] stationSecurity, desktopSecurity;

    public TestUserDesktop(string userSid)
    {
        var sid = new SecurityIdentifier(userSid);
        try
        {
            stationSecurity = Grant(station, sid, 0xF037F);
            desktopSecurity = Grant(desktop, sid, 0xF01FF);
        }
        catch { Dispose(); throw; }
    }

    static byte[] Grant(IntPtr handle, SecurityIdentifier sid, int access)
    {
        uint information = 4, needed;
        GetUserObjectSecurity(handle, ref information, null, 0, out needed);
        if (needed == 0) throw new Win32Exception();
        var original = new byte[needed];
        if (!GetUserObjectSecurity(handle, ref information, original, needed, out needed)) throw new Win32Exception();
        var descriptor = new RawSecurityDescriptor(original, 0);
        if (descriptor.DiscretionaryAcl == null) return original;
        descriptor.DiscretionaryAcl.InsertAce(descriptor.DiscretionaryAcl.Count,
            new CommonAce(AceFlags.None, AceQualifier.AccessAllowed, access, sid, false, null));
        var updated = new byte[descriptor.BinaryLength];
        descriptor.GetBinaryForm(updated, 0);
        if (!SetUserObjectSecurity(handle, ref information, updated)) throw new Win32Exception();
        return original;
    }

    public void Dispose()
    {
        uint information = 4;
        try
        {
            if (desktopSecurity != null && !SetUserObjectSecurity(desktop, ref information, desktopSecurity)) throw new Win32Exception();
            desktopSecurity = null;
        }
        finally
        {
            if (stationSecurity != null && !SetUserObjectSecurity(station, ref information, stationSecurity)) throw new Win32Exception();
            stationSecurity = null;
        }
    }
}
