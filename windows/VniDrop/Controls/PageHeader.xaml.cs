using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using VniDrop.Platform;

namespace VniDrop.Controls;

public sealed partial class PageHeader : UserControl
{
    public static readonly DependencyProperty TitleProperty = DependencyProperty.Register(
        nameof(Title), typeof(string), typeof(PageHeader), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty SubtitleProperty = DependencyProperty.Register(
        nameof(Subtitle), typeof(string), typeof(PageHeader), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty ShowBackButtonProperty = DependencyProperty.Register(
        nameof(ShowBackButton), typeof(bool), typeof(PageHeader), new PropertyMetadata(false, Changed));
    public static readonly DependencyProperty ActionsProperty = DependencyProperty.Register(
        nameof(Actions), typeof(object), typeof(PageHeader), new PropertyMetadata(null, Changed));

    public string Title
    {
        get => (string)GetValue(TitleProperty);
        set => SetValue(TitleProperty, value);
    }

    public string Subtitle
    {
        get => (string)GetValue(SubtitleProperty);
        set => SetValue(SubtitleProperty, value);
    }

    public bool ShowBackButton
    {
        get => (bool)GetValue(ShowBackButtonProperty);
        set => SetValue(ShowBackButtonProperty, value);
    }

    public object? Actions
    {
        get => GetValue(ActionsProperty);
        set => SetValue(ActionsProperty, value);
    }

    public event RoutedEventHandler? BackRequested;

    public PageHeader()
    {
        InitializeComponent();
        Render();
    }

    private static void Changed(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is PageHeader header && header.TitleText is not null) header.Render();
    }

    private void Render()
    {
        TitleText.Text = Title;
        SubtitleText.Text = Subtitle;
        SubtitleText.Visibility = string.IsNullOrWhiteSpace(Subtitle) ? Visibility.Collapsed : Visibility.Visible;
        BackButton.Visibility = ShowBackButton ? Visibility.Visible : Visibility.Collapsed;
        BackButton.SetValue(AutomationProperties.NameProperty, Strings.Get("button_back"));
        ToolTipService.SetToolTip(BackButton, Strings.Get("button_back"));
        ActionsPresenter.Content = Actions;
        ActionsPresenter.Visibility = Actions is null ? Visibility.Collapsed : Visibility.Visible;
    }

    private void BackClicked(object sender, RoutedEventArgs e) => BackRequested?.Invoke(this, e);
}
