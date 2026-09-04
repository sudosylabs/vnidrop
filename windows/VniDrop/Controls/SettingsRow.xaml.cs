using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace VniDrop.Controls;

public sealed partial class SettingsRow : UserControl
{
    public static readonly DependencyProperty GlyphProperty = DependencyProperty.Register(
        nameof(Glyph), typeof(string), typeof(SettingsRow), new PropertyMetadata("\uE946", Changed));
    public static readonly DependencyProperty TitleProperty = DependencyProperty.Register(
        nameof(Title), typeof(string), typeof(SettingsRow), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty DescriptionProperty = DependencyProperty.Register(
        nameof(Description), typeof(string), typeof(SettingsRow), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty ValueProperty = DependencyProperty.Register(
        nameof(Value), typeof(string), typeof(SettingsRow), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty ShowChevronProperty = DependencyProperty.Register(
        nameof(ShowChevron), typeof(bool), typeof(SettingsRow), new PropertyMetadata(false, Changed));
    public static readonly DependencyProperty IsCriticalProperty = DependencyProperty.Register(
        nameof(IsCritical), typeof(bool), typeof(SettingsRow), new PropertyMetadata(false, Changed));
    public static readonly DependencyProperty TrailingContentProperty = DependencyProperty.Register(
        nameof(TrailingContent), typeof(object), typeof(SettingsRow), new PropertyMetadata(null, Changed));

    public string Glyph { get => (string)GetValue(GlyphProperty); set => SetValue(GlyphProperty, value); }
    public string Title { get => (string)GetValue(TitleProperty); set => SetValue(TitleProperty, value); }
    public string Description { get => (string)GetValue(DescriptionProperty); set => SetValue(DescriptionProperty, value); }
    public string Value { get => (string)GetValue(ValueProperty); set => SetValue(ValueProperty, value); }
    public bool ShowChevron { get => (bool)GetValue(ShowChevronProperty); set => SetValue(ShowChevronProperty, value); }
    public bool IsCritical { get => (bool)GetValue(IsCriticalProperty); set => SetValue(IsCriticalProperty, value); }
    public object? TrailingContent { get => GetValue(TrailingContentProperty); set => SetValue(TrailingContentProperty, value); }

    public event RoutedEventHandler? Click;

    public SettingsRow()
    {
        InitializeComponent();
        Render();
        UpdateResponsiveLayout(ActualWidth);
    }

    private static void Changed(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is SettingsRow row && row.RootButton is not null) row.Render();
    }

    private void Render()
    {
        RootButton.Style = (Style)Application.Current.Resources[
            IsCritical ? "VniDropCriticalRowButtonStyle" : "VniDropRowButtonStyle"];
        LeadingIcon.Glyph = Glyph;
        TitleText.Text = Title;
        DescriptionText.Text = Description;
        DescriptionText.Visibility = string.IsNullOrWhiteSpace(Description) ? Visibility.Collapsed : Visibility.Visible;
        ValueText.Text = Value;
        ValueText.Visibility = string.IsNullOrWhiteSpace(Value) ? Visibility.Collapsed : Visibility.Visible;
        ChevronIcon.Visibility = ShowChevron ? Visibility.Visible : Visibility.Collapsed;
        TrailingPresenter.Content = TrailingContent;
        TrailingPresenter.Visibility = TrailingContent is null ? Visibility.Collapsed : Visibility.Visible;
        AutomationProperties.SetName(RootButton, string.IsNullOrWhiteSpace(Value) ? Title : $"{Title}, {Value}");
        AutomationProperties.SetHelpText(RootButton, Description);
        var automationId = AutomationProperties.GetAutomationId(this);
        if (!string.IsNullOrWhiteSpace(automationId)) AutomationProperties.SetAutomationId(RootButton, automationId);
    }

    private void RootClicked(object sender, RoutedEventArgs e) => Click?.Invoke(this, e);

    private void RowSizeChanged(object sender, SizeChangedEventArgs e) => UpdateResponsiveLayout(e.NewSize.Width);

    private void UpdateResponsiveLayout(double width)
    {
        var compact = width < 680;
        Grid.SetColumn(ValueText, compact ? 1 : 2);
        Grid.SetRow(ValueText, compact ? 1 : 0);
        ValueText.TextTrimming = compact ? TextTrimming.None : TextTrimming.CharacterEllipsis;
        ValueText.TextWrapping = compact ? TextWrapping.Wrap : TextWrapping.NoWrap;
    }
}
