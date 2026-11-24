using System.Collections.ObjectModel;

namespace VniDrop.Core;

public static class DeviceListRows
{
    public static void Synchronize<T>(
        ObservableCollection<T> rows,
        IReadOnlyList<T> current,
        Func<T, string> key)
    {
        var remaining = current.Select(key).ToHashSet(StringComparer.Ordinal);
        for (var i = rows.Count - 1; i >= 0; i--)
        {
            if (!remaining.Contains(key(rows[i]))) rows.RemoveAt(i);
        }

        for (var i = 0; i < current.Count; i++)
        {
            var desired = current[i];
            var id = key(desired);
            var existing = i;
            while (existing < rows.Count && !StringComparer.Ordinal.Equals(key(rows[existing]), id))
            {
                existing++;
            }

            if (existing == rows.Count)
            {
                rows.Insert(i, desired);
                continue;
            }

            if (existing != i) rows.Move(existing, i);
            if (!EqualityComparer<T>.Default.Equals(rows[i], desired)) rows[i] = desired;
        }
    }
}
