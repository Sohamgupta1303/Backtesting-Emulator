#!/usr/bin/env python3
"""Generates a synthetic daily OHLCV dataset for chronos-bt's sample data.

Uses geometric Brownian motion with a regime shift partway through (a
change in drift and volatility) so reference strategies have both a
trending and a mean-reverting-ish regime to react to. Purely synthetic —
no network access, no API keys — so the repo works out of the box.
"""

import argparse
import csv
import datetime
import random


def generate_bars(n_bars: int, start: datetime.date, seed: int):
    rng = random.Random(seed)
    price = 100.0

    # Two regimes: trending-up-and-calm, then choppier-and-flatter.
    regime_split = n_bars // 2
    bars = []
    date = start

    for i in range(n_bars):
        if i < regime_split:
            mu, sigma = 0.0006, 0.010
        else:
            mu, sigma = 0.0001, 0.020

        # Advance one trading day at a time, skipping weekends.
        date += datetime.timedelta(days=1)
        while date.weekday() >= 5:
            date += datetime.timedelta(days=1)

        shock = rng.gauss(mu, sigma)
        open_px = price
        close_px = max(0.01, open_px * (1.0 + shock))

        intraday_range = abs(rng.gauss(0, sigma)) * open_px
        high = max(open_px, close_px) + intraday_range * rng.uniform(0.1, 0.6)
        low = min(open_px, close_px) - intraday_range * rng.uniform(0.1, 0.6)
        low = max(0.01, low)

        volume = max(1000, int(rng.gauss(1_000_000, 200_000)))

        bars.append(
            {
                "timestamp": date.isoformat(),
                "open": round(open_px, 4),
                "high": round(high, 4),
                "low": round(low, 4),
                "close": round(close_px, 4),
                "volume": volume,
            }
        )
        price = close_px

    return bars


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bars", type=int, default=750, help="number of daily bars")
    parser.add_argument("--seed", type=int, default=42, help="RNG seed (determinism)")
    parser.add_argument(
        "--start", type=str, default="2021-01-01", help="start date, YYYY-MM-DD"
    )
    parser.add_argument(
        "--out",
        type=str,
        default="data/sample/spy_daily.csv",
        help="output CSV path",
    )
    args = parser.parse_args()

    start = datetime.date.fromisoformat(args.start)
    bars = generate_bars(args.bars, start, args.seed)

    with open(args.out, "w", newline="") as f:
        writer = csv.DictWriter(
            f, fieldnames=["timestamp", "open", "high", "low", "close", "volume"]
        )
        writer.writeheader()
        writer.writerows(bars)

    print(f"wrote {len(bars)} bars to {args.out}")


if __name__ == "__main__":
    main()
