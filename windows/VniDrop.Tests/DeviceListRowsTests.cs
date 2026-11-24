using System.Collections.ObjectModel;
using System.Collections.Specialized;
using VniDrop.Core;
using Xunit;

namespace VniDrop.Tests;

public class DeviceListRowsTests
{
    private sealed record Row(string Id, string Name);

    [Fact]
    public void RefreshKeepsUnchangedRowsAndNotifiesOnlyActualChanges()
    {
        var saved = new Row("saved", "Laptop");
        ObservableCollection<Row> rows = [saved, new("phone", "Phone"), new("obsolete", "Old")];
        var actions = new List<NotifyCollectionChangedAction>();
        rows.CollectionChanged += (_, change) => actions.Add(change.Action);

        DeviceListRows.Synchronize(rows, [new("saved", "Laptop"), new("phone", "Phone"), new("obsolete", "Old")], row => row.Id);
        Assert.Empty(actions);

        DeviceListRows.Synchronize(rows, [new("phone", "New name"), new("saved", "Laptop"), new("tablet", "Tablet")], row => row.Id);

        Assert.Equal(["phone", "saved", "tablet"], rows.Select(row => row.Id));
        Assert.Same(saved, rows[1]);
        Assert.Equal("New name", rows[0].Name);
        Assert.Equal([
            NotifyCollectionChangedAction.Remove,
            NotifyCollectionChangedAction.Move,
            NotifyCollectionChangedAction.Replace,
            NotifyCollectionChangedAction.Add,
        ], actions);
    }
}
