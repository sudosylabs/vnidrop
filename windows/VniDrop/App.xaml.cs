using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;
using System.Security.Cryptography;
using System.Text;
using VniDrop.Core;
using VniDrop.Platform;

namespace VniDrop;

public partial class App : Application
{
    public static MainWindow Window { get; internal set; } = null!;
    private AppInstance? instance;
    private readonly Queue<string> pendingInvitations = new();
    public App() => InitializeComponent();
    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var options = LaunchOptions.Parse(Environment.GetCommandLineArgs().Skip(1));
        var dispatcher = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
        NativeNotifications.Register(() => dispatcher.TryEnqueue(() => Window?.Activate()));
        var activation = AppInstance.GetCurrent().GetActivatedEventArgs();
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(options.Profile.ToUpperInvariant())));
        instance = AppInstance.FindOrRegisterForKey(key);
        if (!instance.IsCurrent)
        {
            await instance.RedirectActivationToAsync(activation);
            NativeNotifications.Unregister(); Exit(); return;
        }
        instance.Activated += (_, redirected) => dispatcher.TryEnqueue(async () =>
        {
            var paths = AppActivation.Invitations(redirected);
            if (Window is null) { foreach (var path in paths) pendingInvitations.Enqueue(path); return; }
            Window.Activate();
            foreach (var path in paths) await Window.OpenInvitationAsync(path);
        });
        var invitations = options.Invitations.Concat(AppActivation.Invitations(activation)).Concat(pendingInvitations).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
        Window = new MainWindow(options.Profile, invitations);
        Window.Closed += (_, _) => { NativeNotifications.Unregister(); instance.UnregisterKey(); };
        Window.Activate();
    }
}
