using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace VniDrop.Controls;

public sealed partial class EmptyState : UserControl
{
    public static readonly DependencyProperty GlyphProperty = DependencyProperty.Register(
        nameof(Glyph), typeof(string), typeof(EmptyState), new PropertyMetadata("\uE946", Changed));
    public static readonly DependencyProperty TitleProperty = DependencyProperty.Register(
        nameof(Title), typeof(string), typeof(EmptyState), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty DescriptionProperty = DependencyProperty.Register(
        nameof(Description), typeof(string), typeof(EmptyState), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty PrimaryButtonTextProperty = DependencyProperty.Register(
        nameof(PrimaryButtonText), typeof(string), typeof(EmptyState), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty PrimaryButtonGlyphProperty = DependencyProperty.Register(
        nameof(PrimaryButtonGlyph), typeof(string), typeof(EmptyState), new PropertyMetadata("\uE710", Changed));
    public static readonly DependencyProperty SecondaryButtonTextProperty = DependencyProperty.Register(
        nameof(SecondaryButtonText), typeof(string), typeof(EmptyState), new PropertyMetadata("", Changed));

    public string Glyph { get => (string)GetValue(GlyphProperty); set => SetValue(GlyphProperty, value); }
    public string Title { get => (string)GetValue(TitleProperty); set => SetValue(TitleProperty, value); }
    public string Description { get => (string)GetValue(DescriptionProperty); set => SetValue(DescriptionProperty, value); }
    public string PrimaryButtonText { get => (string)GetValue(PrimaryButtonTextProperty); set => SetValue(PrimaryButtonTextProperty, value); }
    public string PrimaryButtonGlyph { get => (string)GetValue(PrimaryButtonGlyphProperty); set => SetValue(PrimaryButtonGlyphProperty, value); }
    public string SecondaryButtonText { get => (string)GetValue(SecondaryButtonTextProperty); set => SetValue(SecondaryButtonTextProperty, value); }

    public event RoutedEventHandler? PrimaryClick;
    public event RoutedEventHandler? SecondaryClick;

    public EmptyState()
    {
        InitializeComponent();
        Render();
    }

    private static void Changed(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is EmptyState state && state.StateIcon is not null) state.Render();
    }

    private void Render()
    {
        StateIcon.Glyph = Glyph;
        TitleText.Text = Title;
        DescriptionText.Text = Description;
        DescriptionText.Visibility = string.IsNullOrWhiteSpace(Description) ? Visibility.Collapsed : Visibility.Visible;
        PrimaryText.Text = PrimaryButtonText;
        PrimaryIcon.Glyph = PrimaryButtonGlyph;
        PrimaryButton.Visibility = string.IsNullOrWhiteSpace(PrimaryButtonText) ? Visibility.Collapsed : Visibility.Visible;
        SecondaryText.Text = SecondaryButtonText;
        SecondaryButton.Visibility = string.IsNullOrWhiteSpace(SecondaryButtonText) ? Visibility.Collapsed : Visibility.Visible;
        AutomationProperties.SetName(PrimaryButton, PrimaryButtonText);
        AutomationProperties.SetName(SecondaryButton, SecondaryButtonText);
    }

    private void PrimaryClicked(object sender, RoutedEventArgs e) => PrimaryClick?.Invoke(this, e);
    private void SecondaryClicked(object sender, RoutedEventArgs e) => SecondaryClick?.Invoke(this, e);
}
