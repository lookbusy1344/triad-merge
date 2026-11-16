from invoice.io.parser import parse

TAX_RATE = 0.2


def total(invoices):
    return sum(invoice["amount"] for invoice in invoices)


def with_tax(amount, rate=TAX_RATE):
    return amount * (1 + rate)


def report(path):
    invoices = parse(path)
    print(f"{len(invoices)} invoices, total {with_tax(total(invoices)):.2f}")
    print(f"largest {max(invoice['amount'] for invoice in invoices):.2f}")
