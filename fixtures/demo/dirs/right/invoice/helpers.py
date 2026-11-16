def chunks(items, size):
    for start in range(0, len(items), size):
        yield items[start : start + size]
