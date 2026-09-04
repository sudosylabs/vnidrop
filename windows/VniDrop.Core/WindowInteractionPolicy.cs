namespace VniDrop.Core;

public static class WindowInteractionPolicy
{
    public static bool AllowsNavigation(bool ready, bool maintaining, bool closing) =>
        ready && !maintaining && !closing;
}
