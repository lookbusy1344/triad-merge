from invoice.parser import parse

TAX_RATE = 0.2


def total(invoices):
    return sum(invoice["amount"] for invoice in invoices)


def with_tax(amount):
    return amount * (1 + TAX_RATE)


def report(path):
    invoices = parse(path)
    print(f"{len(invoices)} invoices, total {with_tax(total(invoices)):.2f}")
