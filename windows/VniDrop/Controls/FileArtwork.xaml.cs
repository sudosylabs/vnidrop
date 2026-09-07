using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;
using VniDrop.Core;
using VniDrop.Platform;
using Windows.Storage.Streams;

namespace VniDrop.Controls;

public sealed partial class FileArtwork : UserControl
{
    public static readonly DependencyProperty SourcePathProperty = DependencyProperty.Register(
        nameof(SourcePath), typeof(string), typeof(FileArtwork), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty FileNameProperty = DependencyProperty.Register(
        nameof(FileName), typeof(string), typeof(FileArtwork), new PropertyMetadata("", Changed));
    public static readonly DependencyProperty IsDirectoryProperty = DependencyProperty.Register(
        nameof(IsDirectory), typeof(bool), typeof(FileArtwork), new PropertyMetadata(false, Changed));
    public static readonly DependencyProperty FileCountProperty = DependencyProperty.Register(
        nameof(FileCount), typeof(ulong), typeof(FileArtwork), new PropertyMetadata(1UL, Changed));
    public static readonly DependencyProperty TransferIdProperty = DependencyProperty.Register(
        nameof(TransferId), typeof(ulong), typeof(FileArtwork), new PropertyMetadata(0UL, Changed));

    public string SourcePath { get => (string)GetValue(SourcePathProperty); set => SetValue(SourcePathProperty, value); }
    public string FileName { get => (string)GetValue(FileNameProperty); set => SetValue(FileNameProperty, value); }
    public bool IsDirectory { get => (bool)GetValue(IsDirectoryProperty); set => SetValue(IsDirectoryProperty, value); }
    public ulong FileCount { get => (ulong)GetValue(FileCountProperty); set => SetValue(FileCountProperty, value); }
    public ulong TransferId { get => (ulong)GetValue(TransferIdProperty); set => SetValue(TransferIdProperty, value); }
    private int revision;

    public FileArtwork()
    {
        InitializeComponent();
        Loaded += (_, _) => { App.Window.Model.PreviewSaved += PreviewSaved; LoadPreview(); };
        Unloaded += (_, _) => { App.Window.Model.PreviewSaved -= PreviewSaved; revision++; PreviewImage.Source = null; };
    }

    private void PreviewSaved(ulong id)
    {
        if (id == TransferId) LoadPreview();
    }

    private static void Changed(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        var artwork = (FileArtwork)sender;
        if (artwork.IsLoaded) artwork.LoadPreview();
    }

    private async void LoadPreview()
    {
        var request = ++revision;
        FallbackIcon.Glyph = FileArtworkPresentation.Glyph(FileName, IsDirectory, FileCount);
        FallbackIcon.Visibility = Visibility.Visible;
        PreviewImage.Visibility = Visibility.Collapsed;
        PreviewImage.Source = null;
        try
        {
            var id = TransferId;
            var bytes = !string.IsNullOrWhiteSpace(SourcePath) ? await WindowsFilePreviews.ReadAsync(SourcePath)
                : id != 0 ? await Task.Run(() => App.Window.Model.Previews.Read(id)) : null;
            if (bytes is null || request != revision) return;
            using var stream = new InMemoryRandomAccessStream();
            using (var writer = new DataWriter(stream.GetOutputStreamAt(0)))
            {
                writer.WriteBytes(bytes);
                await writer.StoreAsync();
            }
            stream.Seek(0);
            var image = new BitmapImage { DecodePixelWidth = 128 };
            await image.SetSourceAsync(stream);
            if (request != revision) return;
            PreviewImage.Source = image;
            PreviewImage.Visibility = Visibility.Visible;
            FallbackIcon.Visibility = Visibility.Collapsed;
        }
        catch (Exception) { /* Artwork is optional; unavailable or corrupt previews retain the file-type icon. */ }
    }
}
