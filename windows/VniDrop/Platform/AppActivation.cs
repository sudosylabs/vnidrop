using System.Runtime.InteropServices;
using Microsoft.Windows.AppLifecycle;
using VniDrop.Core;
using Windows.ApplicationModel.Activation;

namespace VniDrop.Platform;

public static class AppActivation
{
    public static string[] Invitations(AppActivationArguments args) => args.Data switch
    {
        IFileActivatedEventArgs files => files.Files.Select(f => f.Path).Where(p => p.EndsWith(".vnd", StringComparison.OrdinalIgnoreCase)).ToArray(),
        ILaunchActivatedEventArgs launch => LaunchOptions.Parse(SplitCommandLine(launch.Arguments)).Invitations,
        _ => [],
    };

    private static string[] SplitCommandLine(string commandLine)
    {
        if (string.IsNullOrWhiteSpace(commandLine)) return [];
        var argv = CommandLineToArgvW(commandLine, out var count);
        if (argv == IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
        try { return Enumerable.Range(0, count).Select(i => Marshal.PtrToStringUni(Marshal.ReadIntPtr(argv, i * IntPtr.Size))!).ToArray(); }
        finally { LocalFree(argv); }
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CommandLineToArgvW(string commandLine, out int count);
    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);
}
