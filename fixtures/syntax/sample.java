// A sample that exercises the Java grammar's highlight captures.
package org.example.store;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/** A point with two coordinates. */
public record Point(int x, int y) {
    public static final Point ORIGIN = new Point(0, 0);

    public Point scaled(int factor) {
        return new Point(x * factor, y * factor);
    }
}

/* A block comment
   across lines. */
enum Mode {
    READ("r"),
    WRITE("w");

    private final String flag;

    Mode(String flag) {
        this.flag = flag;
    }
}

interface Store<K extends Comparable<K>> {
    int load(List<K> keys) throws Exception;

    default boolean isEmpty() {
        return false;
    }
}

@SuppressWarnings("unchecked")
final class Cache<K extends Comparable<K>> implements Store<K> {
    private final Map<K, String> items;
    private int count = 0;

    Cache(Map<K, String> items) {
        this.items = items;
    }

    @Override
    public int load(List<K> keys) throws Exception {
        var found = new ArrayList<String>();
        for (K key : keys) {
            if (items.get(key) != null && count < 0x10) {
                found.add(items.get(key));
                count += 1;
            }
        }
        char separator = '\n';
        double ratio = 1.5e3;
        return found.isEmpty() ? -1 : found.size();
    }
}

class Main {
    public static void main(String[] args) throws Exception {
        var cache = new Cache<String>(Map.of("a", "b"));
        System.out.println(cache.load(List.of("a")) + " items, " + true + null);
    }
}
