using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using VniDrop.Platform;

namespace VniDrop.Controls;

public sealed class ScrollableListDialog : ContentDialog
{
    private readonly ScrollViewer viewport;
    private XamlRoot? observedRoot;

    public ScrollableListDialog(string title, UIElement content)
    {
        Title = title;
        CloseButtonText = Strings.Get("button_close");
        CloseButtonStyle = (Style)Application.Current.Resources["VniDropDialogButtonStyle"];
        HorizontalContentAlignment = HorizontalAlignment.Stretch;
        viewport = new ScrollViewer
        {
            Content = content,
            Style = (Style)Application.Current.Resources["VniDropPageScrollStyle"],
            MaxHeight = 440,
        };
        AutomationProperties.SetAutomationId(viewport, "DialogListViewport");
        Content = viewport;
        Opened += (_, _) =>
        {
            observedRoot = XamlRoot;
            observedRoot.Changed += RootChanged;
            UpdateHeight();
        };
        Closed += (_, _) =>
        {
            if (observedRoot is not null) observedRoot.Changed -= RootChanged;
            observedRoot = null;
        };
    }

    private void RootChanged(XamlRoot sender, XamlRootChangedEventArgs args) => UpdateHeight();

    private void UpdateHeight()
    {
        // Reserve space for the native title, close button, and window margins as the window shrinks.
        viewport.MaxHeight = Math.Min(440, XamlRoot.Size.Height * 0.55);
    }
}
