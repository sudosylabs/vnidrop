using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace VniDrop.Controls;

public enum StatusTone
{
    Neutral,
    Accent,
    Success,
    Warning,
    Critical,
}

public sealed partial class StatusBadge : UserControl
{
    public static readonly DependencyProperty TextProperty = DependencyProperty.Register(
        nameof(Text), typeof(string), typeof(StatusBadge), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty GlyphProperty = DependencyProperty.Register(
        nameof(Glyph), typeof(string), typeof(StatusBadge), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty ToneProperty = DependencyProperty.Register(
        nameof(Tone), typeof(StatusTone), typeof(StatusBadge), new PropertyMetadata(StatusTone.Neutral, Changed));

    public string Text { get => (string)GetValue(TextProperty); set => SetValue(TextProperty, value); }
    public string Glyph { get => (string)GetValue(GlyphProperty); set => SetValue(GlyphProperty, value); }
    public StatusTone Tone { get => (StatusTone)GetValue(ToneProperty); set => SetValue(ToneProperty, value); }

    public StatusBadge()
    {
        InitializeComponent();
        ActualThemeChanged += (_, _) => Render();
        Render();
    }

    private static void Changed(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is StatusBadge badge && badge.BadgeText is not null) badge.Render();
    }

    private void Render()
    {
        BadgeText.Text = Text;
        BadgeIcon.Glyph = Glyph;
        BadgeIcon.Visibility = string.IsNullOrWhiteSpace(Glyph) ? Visibility.Collapsed : Visibility.Visible;
        VisualStateManager.GoToState(this, Tone.ToString(), false);
        AutomationProperties.SetName(this, Text);
    }
}
