using System.ComponentModel;
using VniDrop.Core;
using Xunit;

namespace VniDrop.Tests;

public sealed class PropertyChangeSubscriptionTests
{
    [Fact]
    public void ClearAndRebindReleaseTheOldSourceAndSubscribeExactlyOnce()
    {
        var first = new ObservableItem();
        var second = new ObservableItem();
        var changes = new List<object?>();
        var subscription = new PropertyChangeSubscription((sender, _) => changes.Add(sender));

        subscription.Observe(first);
        subscription.Observe(first);
        first.Change();
        subscription.Observe(null);
        first.Change();

        subscription.Observe(first);
        first.Change();
        subscription.Observe(second);
        first.Change();
        second.Change();
        subscription.Observe(null);
        second.Change();

        Assert.Equal([first, first, second], changes);
    }

    private sealed class ObservableItem : INotifyPropertyChanged
    {
        public event PropertyChangedEventHandler? PropertyChanged;
        public void Change() => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(null));
    }
}
