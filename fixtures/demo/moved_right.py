"""Invoice totals: a demo of moved code for triad."""

def format_total(total):
    """The total as text."""
    return f"{total:.2f}"

def load_config(path):
    """Read the settings file."""
    return read_toml(path)

def parse_header(line):
    """Split the header into its fields."""
    return line.split(";")

def read_lines(path):
    """Read the invoice's lines."""
    return open_file(path).readlines()

def tax_rate(region):
    """The tax rate for a region."""
    return RATES.get(region, 0.25)

def validate_row(row):
    """Check a row has every field."""
    return len(row) == FIELDS

def sum_rows(rows):
    """Add up the rows' amounts."""
    return sum(r.amount for r in rows)

def subtotal(rows):
    """The total before tax."""
    return round_cents(sum_rows(rows))

def write_report(path, text):
    """Write the report."""
    open_file(path, 'w').write(text)

def log_line(text):
    """Log one line."""
    LOG.append(text)

def round_cents(amount):
    """Round to whole cents."""
    return round(amount, 2)

def open_file(path, mode='r'):
    """Open a file as UTF-8."""
    return open(path, mode, encoding='utf-8')

def close_file(handle):
    """Close a file."""
    handle.close()

def count_rows(rows):
    """How many rows there are."""
    return len(rows)

def apply_discount(total, rate):
    """Take the discount off."""
    return total * (1 - rate)

def parse_args(argv):
    """Read the command line."""
    return argv[1:]

def clamp(value, low, high):
    """Keep a value within bounds."""
    return max(low, min(value, high))

def main():
    """Print the invoice's total."""
    print(format_total(subtotal(read_lines(ARGS))))

