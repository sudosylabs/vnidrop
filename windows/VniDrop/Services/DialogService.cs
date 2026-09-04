using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using VniDrop.Core;
using VniDrop.Platform;

namespace VniDrop.Services;

public enum DialogIntent
{
    Standard,
    Destructive,
    Information,
}

public interface IAttentionYieldingDialog
{
    void RequestYieldForAttention();
}

public sealed class DialogService(FrameworkElement owner)
{
    private readonly List<DialogRequest> pending = [];
    private readonly List<TaskCompletionSource> idleWaiters = [];
    private DialogRequest? activeRequest;
    private ContentDialog? activeDialog;
    private bool pumpRunning;
    private long nextSequence;

    public bool HasActiveDialog => activeDialog is not null;

    public void HideActive() => activeDialog?.Hide();

    public Task WaitForIdleAsync()
    {
        if (!pumpRunning && activeRequest is null && pending.Count == 0)
        {
            return Task.CompletedTask;
        }

        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        idleWaiters.Add(completion);
        return completion.Task;
    }

    public Task<ContentDialogResult> DecideAsync(
        string title,
        object content,
        string primary,
        string? secondary = null,
        DialogIntent intent = DialogIntent.Standard,
        DialogPriority priority = DialogPriority.Standard,
        Func<bool>? canShow = null)
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

        return ShowAsync(dialog, priority, canShow);
    }

    public Task<ContentDialogResult> ShowAsync(
        ContentDialog dialog,
        DialogPriority priority = DialogPriority.Standard,
        Func<bool>? canShow = null)
    {
        var request = new DialogRequest(dialog, priority, canShow);
        if (owner.DispatcherQueue.HasThreadAccess)
        {
            Enqueue(request);
        }
        else if (!owner.DispatcherQueue.TryEnqueue(() => Enqueue(request)))
        {
            request.Completion.TrySetException(new InvalidOperationException("The dialog owner is no longer available."));
        }

        return request.Completion.Task;
    }

    private void Enqueue(DialogRequest request)
    {
        request.Sequence = nextSequence++;
        pending.Add(request);

        if (activeRequest is { } active
            && activeDialog is IAttentionYieldingDialog yielding
            && DialogPriorityPolicy.ShouldRequestYield(
                active.Priority,
                request.Priority,
                activeCanYield: true))
        {
            yielding.RequestYieldForAttention();
        }

        if (!pumpRunning)
        {
            pumpRunning = true;
            _ = PumpAsync();
        }
    }

    private async Task PumpAsync()
    {
        var restoreFocus = owner.XamlRoot is null
            ? null
            : FocusManager.GetFocusedElement(owner.XamlRoot) as Control;
        try
        {
            while (pending.Count > 0)
            {
                var request = TakeNext();
                try
                {
                    if (request.CanShow is not null && !request.CanShow())
                    {
                        request.Completion.TrySetResult(ContentDialogResult.None);
                        continue;
                    }
                }
                catch (Exception error)
                {
                    request.Completion.TrySetException(error);
                    continue;
                }

                activeRequest = request;
                activeDialog = request.Dialog;
                void MarkClosed(ContentDialog sender, ContentDialogClosedEventArgs args)
                {
                    if (ReferenceEquals(activeDialog, sender)) activeDialog = null;
                }

                request.Dialog.Closed += MarkClosed;
                try
                {
                    request.Dialog.XamlRoot = owner.XamlRoot;
                    request.Dialog.RequestedTheme = owner.RequestedTheme;
                    request.Dialog.CloseButtonStyle ??= Style("VniDropDialogButtonStyle");
                    request.Completion.TrySetResult(await request.Dialog.ShowAsync());
                }
                catch (Exception error)
                {
                    request.Completion.TrySetException(error);
                }
                finally
                {
                    request.Dialog.Closed -= MarkClosed;
                    if (ReferenceEquals(activeDialog, request.Dialog)) activeDialog = null;
                    if (ReferenceEquals(activeRequest, request)) activeRequest = null;
                }
            }
        }
        finally
        {
            pumpRunning = false;
            restoreFocus?.Focus(FocusState.Programmatic);
            foreach (var waiter in idleWaiters) waiter.TrySetResult();
            idleWaiters.Clear();
        }
    }

    private DialogRequest TakeNext()
    {
        var bestIndex = 0;
        for (var index = 1; index < pending.Count; index++)
        {
            var candidate = pending[index];
            var current = pending[bestIndex];
            if (DialogPriorityPolicy.ShouldRunBefore(
                candidate.Priority,
                candidate.Sequence,
                current.Priority,
                current.Sequence))
            {
                bestIndex = index;
            }
        }

        var result = pending[bestIndex];
        pending.RemoveAt(bestIndex);
        return result;
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

    private sealed class DialogRequest(
        ContentDialog dialog,
        DialogPriority priority,
        Func<bool>? canShow)
    {
        public ContentDialog Dialog { get; } = dialog;

        public DialogPriority Priority { get; } = priority;

        public Func<bool>? CanShow { get; } = canShow;

        public TaskCompletionSource<ContentDialogResult> Completion { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public long Sequence { get; set; }
    }
}
