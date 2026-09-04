using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace VniDrop.Views.Settings;

public sealed partial class SettingsValueRow : UserControl
{
    public static readonly DependencyProperty LabelProperty = DependencyProperty.Register(nameof(Label), typeof(string), typeof(SettingsValueRow), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty ValueProperty = DependencyProperty.Register(nameof(Value), typeof(string), typeof(SettingsValueRow), new PropertyMetadata("", Changed));
    public string Label { get => (string)GetValue(LabelProperty); set => SetValue(LabelProperty, value); }
    public string Value { get => (string)GetValue(ValueProperty); set => SetValue(ValueProperty, value); }
    public SettingsValueRow()
    {
        InitializeComponent();
        SizeChanged += (_, args) => UpdateLayout(args.NewSize.Width);
        Render();
    }

    private static void Changed(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is SettingsValueRow row && row.LabelText is not null) row.Render();
    }

    private void Render()
    {
        LabelText.Text = Label;
        ValueText.Text = Value;
    }

    private void UpdateLayout(double width)
    {
        var compact = width < 420;
        Grid.SetRow(ValueText, compact ? 1 : 0);
        Grid.SetColumn(ValueText, compact ? 0 : 1);
        ValueText.TextAlignment = compact ? TextAlignment.Left : TextAlignment.Right;
        ValueText.Margin = compact ? new Thickness(0, 4, 0, 0) : new Thickness(0);
    }
}
