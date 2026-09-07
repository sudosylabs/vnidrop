namespace VniDrop.Core;

public sealed record LaunchOptions(string Profile, string[] Invitations)
{
    private bool NotificationActivation { get; init; }

    public T? ReadActivation<T>(bool notificationsAvailable, Func<T> read) where T : class =>
        NotificationActivation && !notificationsAvailable ? null : read();

    public static LaunchOptions Parse(IEnumerable<string> arguments)
    {
        var values = arguments.ToArray();
        var profile = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".vnidrop");
        var invitations = new List<string>();
        var notificationActivation = false;
        for (var i = 0; i < values.Length; i++)
        {
            if (values[i] == "--profile")
            {
                if (i + 1 >= values.Length) throw new ArgumentException("An option requires a value.");
                profile = values[++i];
            }
            else if (values[i].StartsWith("----AppNotificationActivated:", StringComparison.Ordinal)) notificationActivation = true;
            else if (values[i].EndsWith(".vnd", StringComparison.OrdinalIgnoreCase)) invitations.Add(Path.GetFullPath(values[i]));
        }
        return new(Path.TrimEndingDirectorySeparator(Path.GetFullPath(profile)), invitations.Distinct(StringComparer.OrdinalIgnoreCase).ToArray())
        { NotificationActivation = notificationActivation };
    }
}
