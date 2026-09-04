using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace VniDrop.Views.Settings;

public sealed partial class SettingsPageLayout : UserControl
{
    public static readonly DependencyProperty TitleProperty = DependencyProperty.Register(nameof(Title), typeof(string), typeof(SettingsPageLayout), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty SubtitleProperty = DependencyProperty.Register(nameof(Subtitle), typeof(string), typeof(SettingsPageLayout), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty HeaderActionsProperty = DependencyProperty.Register(nameof(HeaderActions), typeof(object), typeof(SettingsPageLayout), new PropertyMetadata(null, Changed));
    public static readonly DependencyProperty PageContentProperty = DependencyProperty.Register(nameof(PageContent), typeof(object), typeof(SettingsPageLayout), new PropertyMetadata(null, Changed));
    public static readonly DependencyProperty ContentMaxWidthProperty = DependencyProperty.Register(nameof(ContentMaxWidth), typeof(double), typeof(SettingsPageLayout), new PropertyMetadata(760d, Changed));

    public string Title { get => (string)GetValue(TitleProperty); set => SetValue(TitleProperty, value); }
    public string Subtitle { get => (string)GetValue(SubtitleProperty); set => SetValue(SubtitleProperty, value); }
    public object? HeaderActions { get => GetValue(HeaderActionsProperty); set => SetValue(HeaderActionsProperty, value); }
    public object? PageContent { get => GetValue(PageContentProperty); set => SetValue(PageContentProperty, value); }
    public double ContentMaxWidth { get => (double)GetValue(ContentMaxWidthProperty); set => SetValue(ContentMaxWidthProperty, value); }

    public event RoutedEventHandler? BackRequested;

    public SettingsPageLayout()
    {
        InitializeComponent();
        Loaded += (_, _) => UpdateContentWidth();
        SizeChanged += (_, _) => UpdateContentWidth();
        Render();
    }

    private static void Changed(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is SettingsPageLayout layout && layout.Header is not null) layout.Render();
    }

    private void Render()
    {
        Header.Title = Title;
        Header.Subtitle = Subtitle;
        Header.Actions = HeaderActions;
        PageContentPresenter.Content = PageContent;
        ContentColumn.MaxWidth = ContentMaxWidth;
        UpdateContentWidth();
    }

    private void UpdateContentWidth()
    {
        var available = Math.Max(0, Root.ActualWidth - Root.Padding.Left - Root.Padding.Right);
        ContentColumn.Width = Math.Min(ContentMaxWidth, available);
    }

    private void BackRequestedByHeader(object sender, RoutedEventArgs e) => BackRequested?.Invoke(this, e);
}
