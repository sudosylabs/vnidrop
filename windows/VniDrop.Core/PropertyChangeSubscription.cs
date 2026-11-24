using System.ComponentModel;

namespace VniDrop.Core;

public sealed class PropertyChangeSubscription(PropertyChangedEventHandler handler)
{
    private INotifyPropertyChanged? source;

    public void Observe(INotifyPropertyChanged? next)
    {
        if (ReferenceEquals(source, next)) return;
        if (source is not null) source.PropertyChanged -= handler;
        source = next;
        if (source is not null) source.PropertyChanged += handler;
    }
}
