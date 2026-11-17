// sample.c exercises the C grammar's highlight captures.
#include <stdio.h>

#define LIMIT 10
#define SQUARE(x) ((x) * (x))

/*
 * A block comment.
 *
 * It spans a blank line.
 */

typedef unsigned long size_type;

enum colour { RED, GREEN = 2, BLUE };

struct point {
    int x;
    int y;
    const char *label;
};

static const double ratio = 0.75;
static char separator = '\t';

#ifdef DEBUG
static int verbose = 1;
#else
static int verbose = 0;
#endif

static int area(const struct point *a, const struct point *b) {
    int width = b->x - a->x;
    int height = b->y - a->y;
    return width < 0 ? -width * height : width * height;
}

size_type count(const char *text) {
    size_type n = 0;
    while (*text != '\0') {
        if (*text++ == ' ') {
            continue;
        }
        n += 1;
    }
    return n;
}

int main(int argc, char **argv) {
    struct point origin = {.x = 0, .y = 0, .label = "origin"};
    for (int i = 0; i < LIMIT && i < argc; i++) {
        printf("%s: %d\n", argv[i], SQUARE(i));
    }
    return verbose ? 0x1F : sizeof(origin) % 2;
}
