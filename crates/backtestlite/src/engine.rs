//! The engine: candle validation, the fill/accounting loop over
//! [`stratlite::Session`], and Report assembly — the crate docs' fill model
//! and accounting rules, implemented all in integers (i128 inside, i64 out).

use diaglite::Diag;
use stratlite::{Candle, Limits, Session, Signal, Strategy};

use crate::{Costs, MAX_TICKS, Report, codes};

fn dir_of(s: Signal) -> i64 {
    match s {
        Signal::Long => 1,
        Signal::Flat => 0,
        Signal::Short => -1,
    }
}

/// Validate one candle (E0301, spanless — it faults the DATA, not the
/// source; the bar index rides in the message). Prices are positive ticks
/// with OHLC coherence, capped at [`MAX_TICKS`]: full-lookback window sums
/// then provably fit the indicators' i128 accumulators (their i64 results
/// follow), and honest strategy arithmetic on prices has i64 headroom.
fn validate(i: usize, c: &Candle) -> Result<(), Diag> {
    let ok = 0 < c.low
        && c.low <= c.open.min(c.close)
        && c.open.max(c.close) <= c.high
        && c.high <= MAX_TICKS
        && (0..=MAX_TICKS).contains(&c.volume);
    if ok {
        Ok(())
    } else {
        Err(Diag::new_code(
            codes::BAD_CANDLE,
            format!(
                "candle {i} is incoherent or out of range \
                 (0 < low <= open,close <= high <= 2^53; 0 <= volume <= 2^53)"
            ),
        ))
    }
}

/// The running account. All i128 until the Report is assembled.
#[derive(Default)]
struct Book {
    realized: i128,
    trades: u32,
    wins: u32,
    gross_profit: i128,
    gross_loss: i128,
}

/// The open trade, if any: direction (±1), entry fill price, fees paid so far.
struct Open {
    dir: i64,
    entry: i64,
    fees: i128,
}

impl Book {
    /// Close `open` at fill price `px`, charging the exit fee.
    fn close(&mut self, open: &Open, px: i64, costs: &Costs) {
        let raw = i128::from(open.dir) * (i128::from(px) - i128::from(open.entry));
        let exit_fee = i128::from(costs.fee_ticks);
        self.realized += raw - exit_fee;
        let pnl = raw - open.fees - exit_fee; // trade pnl AFTER all costs
        self.trades += 1;
        if pnl > 0 {
            self.wins += 1;
            self.gross_profit += pnl;
        } else {
            self.gross_loss += -pnl;
        }
    }
}

pub(crate) fn backtest(
    program: &Strategy,
    candles: &[Candle],
    limits: Limits,
    costs: Costs,
) -> Result<Report, Diag> {
    // Costs are inputs too: bounds make the fill arithmetic provably safe
    // and the ADVERSE-slippage promise mechanically true (E0304).
    if !(0..=MAX_TICKS).contains(&costs.fee_ticks)
        || !(0..=MAX_TICKS).contains(&costs.slippage_ticks)
    {
        return Err(Diag::new_code(
            codes::BAD_COSTS,
            format!(
                "costs must be 0..=2^53 ticks (fee {}, slippage {})",
                costs.fee_ticks, costs.slippage_ticks
            ),
        ));
    }
    for (i, c) in candles.iter().enumerate() {
        validate(i, c)?;
    }
    let lookback = program.lookback() as usize;
    if candles.len() <= lookback {
        return Err(Diag::new_code(
            codes::SHORT_DATA,
            format!(
                "{} candle(s) cannot warm up a lookback of {lookback} and still evaluate",
                candles.len()
            ),
        ));
    }

    let mut session = Session::new(program, limits);
    let mut book = Book::default();
    let mut open: Option<Open> = None;
    let mut bars_in_market: u32 = 0;
    let mut equity_curve: Vec<i64> = Vec::with_capacity(candles.len());
    let mut pending: Option<Signal> = None;

    // An order's fill price: `dir` +1 buys, -1 sells; slippage is adverse.
    let fill_px = |price: i64, dir: i64| price + dir * costs.slippage_ticks;

    for candle in candles {
        // 1. Fill the target queued on the previous close, at THIS bar's open.
        if let Some(target) = pending.take() {
            let want = dir_of(target);
            if let Some(o) = open.take_if(|o| o.dir != want) {
                book.close(&o, fill_px(candle.open, -o.dir), &costs);
            }
            if open.is_none() && want != 0 {
                let entry = fill_px(candle.open, want);
                let fee = i128::from(costs.fee_ticks);
                book.realized -= fee;
                open = Some(Open {
                    dir: want,
                    entry,
                    fees: fee,
                });
            }
            let entry = open.as_ref().map_or(0, |o| o.entry);
            session.set_position(target, entry);
        }

        // 2. Step the strategy with the now-closed bar.
        let target = session.step(*candle)?;

        // 3. Mark equity at this close.
        let mark = match &open {
            Some(o) => {
                bars_in_market += 1;
                book.realized + i128::from(o.dir) * (i128::from(candle.close) - i128::from(o.entry))
            }
            None => book.realized,
        };
        equity_curve.push(to_i64(mark)?);

        // 4. Queue the target for the next open when it changes the position.
        if dir_of(target) != open.as_ref().map_or(0, |o| o.dir) {
            pending = Some(target);
        }
    }

    // Force-close any open position at the final close (costs applied); the
    // final mark reflects the forced exit.
    if let Some(o) = open.take() {
        let last_close = candles[candles.len() - 1].close;
        book.close(&o, fill_px(last_close, -o.dir), &costs);
        let last = equity_curve.len() - 1;
        equity_curve[last] = to_i64(book.realized)?;
    }

    // Drawdown in ONE pass over the FINAL curve — the exact curve the hash
    // publishes, so a pre-force-close mark can never act as a phantom peak.
    let (mut peak, mut max_drawdown) = (0i128, 0i128);
    let mut hash_input = Vec::with_capacity(equity_curve.len() * 8);
    for &e in &equity_curve {
        peak = peak.max(i128::from(e));
        max_drawdown = max_drawdown.max(peak - i128::from(e));
        hash_input.extend_from_slice(&e.to_le_bytes());
    }
    Ok(Report {
        bars_seen: candles.len(),
        bars_evaluated: session.bars_evaluated(),
        trades: book.trades,
        wins: book.wins,
        net_pnl_ticks: to_i64(book.realized)?,
        gross_profit_ticks: to_i64(book.gross_profit)?,
        gross_loss_ticks: to_i64(book.gross_loss)?,
        max_drawdown_ticks: to_i64(max_drawdown)?,
        bars_in_market,
        max_fuel_per_bar: session.max_fuel_per_bar(),
        equity_hash: caplite::fnv1a_64(&hash_input),
    })
}

fn to_i64(v: i128) -> Result<i64, Diag> {
    i64::try_from(v).map_err(|_| {
        Diag::new_code(
            codes::ACCOUNT_OVERFLOW,
            format!("accounting value {v} does not fit i64 ticks"),
        )
    })
}
