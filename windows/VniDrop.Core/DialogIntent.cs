namespace VniDrop.Core;

public enum DialogIntent
{
    Standard,
    Destructive,
    Information,
    Consent,
}

public static class DialogIntentPolicy
{
    public static bool RequiresExplicitPrimaryAction(DialogIntent intent) =>
        intent is DialogIntent.Destructive or DialogIntent.Consent;
}
