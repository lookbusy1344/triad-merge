// A sample Zig file for the highlighting tests.
const std = @import("std");
const Allocator = std.mem.Allocator;

/// The largest number of lines a chunk holds.
pub const MAX_LINES: usize = 2_000;

pub const Side = enum(u2) {
    base,
    left,
    right,

    pub fn name(self: Side) []const u8 {
        return switch (self) {
            .base => "base",
            .left => "left",
            .right => "right",
        };
    }
};

const Span = struct {
    start: u32,
    end: u32 = 0,
    label: ?[]const u8 = null,

    fn len(self: Span) u32 {
        return self.end -| self.start;
    }
};

const ParseError = error{ OutOfMemory, TooLong };

fn countLines(allocator: Allocator, text: []const u8) ParseError![]usize {
    var starts = std.ArrayList(usize).init(allocator);
    errdefer starts.deinit();
    try starts.append(0);
    for (text, 0..) |byte, i| {
        if (byte == '\n') {
            if (starts.items.len >= MAX_LINES) return error.TooLong;
            try starts.append(i + 1);
        }
    }
    return starts.toOwnedSlice();
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const lines = countLines(gpa.allocator(), "a\nb\n") catch |err| {
        std.debug.print("failed: {s}\n", .{@errorName(err)});
        return err;
    };
    defer gpa.allocator().free(lines);
    const ratio: f64 = @as(f64, @floatFromInt(lines.len)) / 0x10;
    std.debug.print("{d} lines, {d:.2}\n", .{ lines.len, ratio });
}

test "a span without an end is empty" {
    const span = Span{ .start = 3 };
    try std.testing.expectEqual(@as(u32, 0), span.len());
    try std.testing.expect(span.label == null and true);
}
