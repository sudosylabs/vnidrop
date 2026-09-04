using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using VniDrop.Platform;

namespace VniDrop.Services;

public enum DialogIntent
{
    Standard,
    Destructive,
    Information,
}

public sealed class DialogService(FrameworkElement owner)
{
    private readonly SemaphoreSlim gate = new(1);
    private ContentDialog? activeDialog;

    public bool HasActiveDialog => activeDialog is not null;

    public void HideActive() => activeDialog?.Hide();

    public async Task WaitForIdleAsync()
    {
        await gate.WaitAsync();
        gate.Release();
    }

    public Task<ContentDialogResult> DecideAsync(
        string title,
        object content,
        string primary,
        string? secondary = null,
        DialogIntent intent = DialogIntent.Standard)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            Content = content is string text
                ? new TextBlock
                {
                    Text = text,
                    TextWrapping = TextWrapping.Wrap,
                    MaxWidth = 520,
                }
                : content,
            PrimaryButtonText = primary,
            SecondaryButtonText = secondary ?? "",
            CloseButtonText = Strings.Get("button_cancel"),
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            DefaultButton = intent == DialogIntent.Destructive
                ? ContentDialogButton.None
                : ContentDialogButton.Primary,
            PrimaryButtonStyle = Style(intent == DialogIntent.Destructive
                ? "VniDropDialogCriticalButtonStyle"
                : "VniDropDialogAccentButtonStyle"),
            SecondaryButtonStyle = Style("VniDropDialogButtonStyle"),
            CloseButtonStyle = Style("VniDropDialogButtonStyle"),
        };

        if (intent == DialogIntent.Destructive)
            dialog.Opened += (_, _) => FocusButton(dialog, dialog.CloseButtonText);

        return ShowAsync(dialog);
    }

    public async Task<ContentDialogResult> ShowAsync(ContentDialog dialog)
    {
        await gate.WaitAsync();
        var restoreFocus = owner.XamlRoot is null ? null : FocusManager.GetFocusedElement(owner.XamlRoot) as Control;
        void DialogClosing(ContentDialog sender, ContentDialogClosingEventArgs args)
        {
            if (!args.Cancel && ReferenceEquals(activeDialog, dialog)) activeDialog = null;
        }
        void DialogClosed(ContentDialog sender, ContentDialogClosedEventArgs args)
        {
            if (ReferenceEquals(activeDialog, dialog)) activeDialog = null;
        }
        try
        {
            activeDialog = dialog;
            dialog.Closing += DialogClosing;
            dialog.Closed += DialogClosed;
            dialog.XamlRoot = owner.XamlRoot;
            dialog.RequestedTheme = owner.RequestedTheme;
            dialog.CloseButtonStyle ??= Style("VniDropDialogButtonStyle");
            return await dialog.ShowAsync();
        }
        finally
        {
            dialog.Closing -= DialogClosing;
            dialog.Closed -= DialogClosed;
            if (ReferenceEquals(activeDialog, dialog)) activeDialog = null;
            gate.Release();
            restoreFocus?.Focus(FocusState.Programmatic);
        }
    }

    private static Style Style(string key) => (Style)Application.Current.Resources[key];

    private static void FocusButton(DependencyObject root, string text)
    {
        var count = VisualTreeHelper.GetChildrenCount(root);
        for (var index = 0; index < count; index++)
        {
            var child = VisualTreeHelper.GetChild(root, index);
            if (child is Button button && string.Equals(button.Content?.ToString(), text, StringComparison.Ordinal))
            {
                button.Focus(FocusState.Programmatic);
                return;
            }
            FocusButton(child, text);
        }
    }
}
