using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace VniDrop.Views.Settings;

public sealed partial class FactRow : UserControl
{
    public static readonly DependencyProperty GlyphProperty = DependencyProperty.Register(nameof(Glyph), typeof(string), typeof(FactRow), new PropertyMetadata("\uE946", Changed));
    public static readonly DependencyProperty TextProperty = DependencyProperty.Register(nameof(Text), typeof(string), typeof(FactRow), new PropertyMetadata("", Changed));
    public string Glyph { get => (string)GetValue(GlyphProperty); set => SetValue(GlyphProperty, value); }
    public string Text { get => (string)GetValue(TextProperty); set => SetValue(TextProperty, value); }
    public FactRow()
    {
        InitializeComponent();
        Render();
    }

    private static void Changed(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is FactRow row && row.FactText is not null) row.Render();
    }

    private void Render()
    {
        FactIcon.Glyph = Glyph;
        FactText.Text = Text;
    }
}
