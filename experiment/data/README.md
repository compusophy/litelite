# experiment/data — pinned candles for the §5 verifier and fine-tune benchmark

Three 1-hour OHLCV kline files, committed so every §5 number reproduces from a
clone (the paper's reproducibility claim rests on it):

| file | role in the paper |
|---|---|
| `BTCUSDT-1h-2024-01.csv` | the train reward window (§5.2–5.6) |
| `BTCUSDT-1h-2024-06.csv` | the distant, five-month-embargo held-out window (§5.6) |
| `ETHUSDT-1h-2024-06.csv`  | the cross-asset held-out window (§5.6) |

**Provenance.** Public Binance historical 1h klines (the standard
`data.binance.vision` schema: open-time ms, open, high, low, close, volume,
close-time, quote-volume, trades, taker-buy-base, taker-buy-quote, ignore).
Freely available public market data, committed here as small fixed samples for
academic reproducibility.

**Validate against the verifier's contract** (every candle must satisfy
`backtestlite`'s integer bounds before it can be scored):

    cd experiment && ./target/release/s5 data data/BTCUSDT-1h-2024-01.csv

reports the candle count, the train/held-out split, and the train-only cost
model. `backtestlite` converts prices to integer ticks (cents); the files stay
well inside the `MAX_TICKS` headroom `s5 data` prints.
