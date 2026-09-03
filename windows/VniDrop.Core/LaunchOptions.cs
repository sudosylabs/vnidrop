namespace VniDrop.Core;

public sealed record LaunchOptions(string Profile, string[] Invitations)
{
    public static LaunchOptions Parse(IEnumerable<string> arguments)
    {
        var values = arguments.ToArray();
        var profile = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".vnidrop");
        var invitations = new List<string>();
        for (var i = 0; i < values.Length; i++)
        {
            if (values[i] == "--profile")
            {
                if (i + 1 >= values.Length) throw new ArgumentException("An option requires a value.");
                profile = values[++i];
            }
            else if (values[i].EndsWith(".vnd", StringComparison.OrdinalIgnoreCase)) invitations.Add(Path.GetFullPath(values[i]));
        }
        return new(Path.TrimEndingDirectorySeparator(Path.GetFullPath(profile)), invitations.Distinct(StringComparer.OrdinalIgnoreCase).ToArray());
    }
}
