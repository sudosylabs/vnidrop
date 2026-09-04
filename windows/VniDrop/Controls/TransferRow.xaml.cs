using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using VniDrop.Platform;
using VniDrop.ViewModels;

namespace VniDrop.Controls;

public sealed partial class TransferRow : UserControl
{
    public static readonly DependencyProperty ItemProperty = DependencyProperty.Register(
        nameof(Item), typeof(TransferItem), typeof(TransferRow), new PropertyMetadata(null, ItemChanged));

    public TransferItem? Item
    {
        get => (TransferItem?)GetValue(ItemProperty);
        set => SetValue(ItemProperty, value);
    }

    public event EventHandler<TransferItem>? ShareRequested;
    public event EventHandler<TransferItem>? StopRequested;
    public event EventHandler<TransferItem>? DeleteRequested;

    public TransferRow()
    {
        InitializeComponent();
        Render();
    }

    private static void ItemChanged(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is not TransferRow row) return;
        if (args.OldValue is TransferItem oldItem) oldItem.PropertyChanged -= row.ItemPropertyChanged;
        if (args.NewValue is TransferItem newItem) newItem.PropertyChanged += row.ItemPropertyChanged;
        row.Render();
    }

    private void ItemPropertyChanged(object? sender, PropertyChangedEventArgs e) => Render();

    private void Render()
    {
        if (Item is not { } item)
        {
            RootGrid.Visibility = Visibility.Collapsed;
            return;
        }
        RootGrid.Visibility = Visibility.Visible;
        NameText.Text = item.Name;
        DetailText.Text = $"{item.CatalogDetail} · {item.Date}";
        Badge.Text = item.Status;
        Badge.Glyph = item.StatusGlyph;
        Badge.Tone = item.StatusTone;
        ProgressPanel.Visibility = item.ShowProgress ? Visibility.Visible : Visibility.Collapsed;
        Progress.IsIndeterminate = item.IsIndeterminate;
        Progress.Value = item.Progress;
        ProgressText.Text = item.ProgressText;
        ShareMenuItem.Visibility = item.CanShare ? Visibility.Visible : Visibility.Collapsed;
        StopMenuItem.Visibility = item.CanStop ? Visibility.Visible : Visibility.Collapsed;
        DeleteMenuItem.Visibility = item.CanDelete ? Visibility.Visible : Visibility.Collapsed;
        DestructiveSeparator.Visibility = item.CanShare && (item.CanStop || item.CanDelete)
            ? Visibility.Visible
            : Visibility.Collapsed;
        AutomationProperties.SetName(this, item.AutomationName);
        AutomationProperties.SetName(MoreButton, Strings.Get("button_more_actions"));
        ToolTipService.SetToolTip(MoreButton, Strings.Get("button_more_actions"));
    }

    private void ShowActions(object sender, RoutedEventArgs e) =>
        ((MenuFlyout)Resources["TransferActionsFlyout"]).ShowAt(MoreButton);

    private void ShareClicked(object sender, RoutedEventArgs e)
    {
        if (Item is { } item) ShareRequested?.Invoke(this, item);
    }

    private void StopClicked(object sender, RoutedEventArgs e)
    {
        if (Item is { } item) StopRequested?.Invoke(this, item);
    }

    private void DeleteClicked(object sender, RoutedEventArgs e)
    {
        if (Item is { } item) DeleteRequested?.Invoke(this, item);
    }
}
