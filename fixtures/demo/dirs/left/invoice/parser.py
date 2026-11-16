"""Read invoices from CSV."""
import csv


def parse(path):
    with open(path, newline="") as handle:
        rows = list(csv.DictReader(handle))
    return [row_to_invoice(row) for row in rows]


def row_to_invoice(row):
    return {"id": row["id"], "amount": float(row["amount"])}
