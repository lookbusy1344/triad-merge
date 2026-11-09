// A sample that exercises the C# grammar's highlight captures.
using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;

#nullable enable

namespace Sample.Highlight;

/// <summary>A point with <see cref="double"/> coordinates.</summary>
/// <param name="X">The first coordinate.</param>
public readonly record struct Point(double X, double Y)
{
    public double Length => Math.Sqrt(X * X + Y * Y);
}

[Flags]
public enum Mode : byte
{
    None = 0,
    Read = 1 << 0,
    Write = 1 << 1,
}

/* A block comment
   across lines. */
[Serializable, Obsolete("Use Other", error: false)]
public sealed class Store<TKey, TValue> : IDisposable
    where TKey : notnull
    where TValue : class, new()
{
    private const int Limit = 0x10;
    private readonly Dictionary<TKey, TValue> _items = new();
    private bool _disposed;

    public event EventHandler<TKey>? Added;

    public int Count { get; private set; }

    public TValue this[TKey key] => _items.TryGetValue(key, out var value) ? value : new TValue();

    public async Task<int> LoadAsync(IEnumerable<TKey> keys, CancellationToken token = default)
    {
        var path = @"C:\data\""quoted"".txt";
        var name = $"store {Count,4:D} of {Limit} \"{path}\"";
        var json = """
            { "name": "raw" }
            """;
        char tab = '\t';
        foreach (var key in keys.Where(k => k is not null).Distinct())
        {
            token.ThrowIfCancellationRequested();
            await Task.Yield();
            _items[key] = new TValue();
            Added?.Invoke(this, key);
            Count++;
        }
#if DEBUG
        Console.WriteLine(name + json + tab);
#endif
        return Count switch
        {
            0 => -1,
            > Limit and < 100 => Limit,
            _ => Count,
        };
    }

    public static bool IsEmpty(object? value) => value is null or "" or 0;

    #region Disposal
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        GC.SuppressFinalize(this);
    }
    #endregion
}

public static class Program
{
    public static void Main(string[] args)
    {
        var query = from a in args
                    where a.Length > 0
                    orderby a descending
                    select a.ToUpperInvariant();
        Console.WriteLine(string.Join(", ", query) ?? nameof(Main));
        unchecked { int x = int.MaxValue + 1; _ = x; }
        Store<string, object> store = new();
        _ = store.Count == 0 ? true : false;
    }
}
