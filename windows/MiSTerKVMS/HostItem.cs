using System.ComponentModel;
using System.Runtime.CompilerServices;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media.Imaging;
using MiSTerKVMS.Core;
using Windows.Storage.Streams;

namespace MiSTerKVMS;

/// <summary>A host's picture and what it's running, from the core's
/// hostPreviews (refreshed on its own; previewsChanged says when).</summary>
public sealed class HostPreview
{
    public string Id { get; set; } = "";
    /// <summary>JPEG, base64; null until there's a picture.</summary>
    public string? Jpeg { get; set; }
    public uint Width { get; set; }
    public uint Height { get; set; }
    /// <summary>The core ("No core running" in the menu), "Offline",
    /// "Connecting…", or "" when not known.</summary>
    public string Caption { get; set; } = "";
    public ulong Version { get; set; }
}

/// <summary>A host in the list: its picture on top, the name, and what
/// it's running (or where its picture comes from until that's known).</summary>
public sealed class HostItem : INotifyPropertyChanged
{
    public HostItem(HostProfile profile) => Profile = profile;

    public HostProfile Profile { get; }
    public string Id => Profile.Id;
    public string Name => Profile.Name;

    private string _caption = "";
    public string Caption => _caption.Length > 0 ? _caption : Profile.Subtitle;

    private BitmapImage? _picture;
    public BitmapImage? Picture => _picture;
    public Visibility NoPicture => _picture == null ? Visibility.Visible : Visibility.Collapsed;

    private double _pictureHeight = 150;
    public double PictureHeight
    {
        get => _pictureHeight;
        set { if (_pictureHeight != value) { _pictureHeight = value; Changed(); } }
    }

    private ulong _version;

    public async void Update(HostPreview p)
    {
        if (p.Caption != _caption)
        {
            _caption = p.Caption;
            Changed(nameof(Caption));
        }
        if (p.Version == _version || p.Jpeg == null) return;
        _version = p.Version;
        var bytes = Convert.FromBase64String(p.Jpeg);
        using var stream = new InMemoryRandomAccessStream();
        using (var writer = new DataWriter(stream.GetOutputStreamAt(0)))
        {
            writer.WriteBytes(bytes);
            await writer.StoreAsync();
            await writer.FlushAsync();
            writer.DetachStream();
        }
        stream.Seek(0);
        var image = new BitmapImage();
        await image.SetSourceAsync(stream);
        // A newer picture arrived while this one was decoding.
        if (_version != p.Version) return;
        _picture = image;
        Changed(nameof(Picture));
        Changed(nameof(NoPicture));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    private void Changed([CallerMemberName] string? name = null) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}
