import sys

from invoice.totals import report


def main():
    report(sys.argv[1])
