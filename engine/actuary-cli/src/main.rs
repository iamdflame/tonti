//! Native driver for actuary-core. It runs the exact code the Stylus contract runs, so external
//! checks (mpmath precision, the float reference model, the simulator, the backtest) test the
//! real engine rather than a re-implementation.
//!
//!   actuary-cli fx   < lines of "<op> <raw_i128> [<raw_i128>]"   (op: exp ln pow sqrt mul div)
//!   actuary-cli fairness <members> <trials> <seed>
//!   actuary-cli quote <A> <B> <theta> <kappa> <age> <year> <start_age> <lump> <monthly> <spy_ret> <spy_vol> <safe> <rate> <paths> <seed>
//!     (floats in, JSON out; the engine itself runs in Q64.64)
//!   actuary-cli ghosts <hidden> <horizon_months> <runs> <seed> <A,B,theta,kappa,born,members;...>
//!   actuary-cli replay <members> <pot> <seed>  < lines of "spy tbill q_month payout_fraction inflation"

mod fairness;
mod ghosts;
mod replay;

use actuary_core::mortality::Gm;
use actuary_core::Fx;
use std::io::{self, BufRead, Write};

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    match cmd.as_str() {
        "fx" => fx_ops(),
        "fairness" => fairness_cmd(),
        "quote" => quote_cmd(),
        "ghosts" => ghosts_cmd(),
        "replay" => replay_cmd(),
        _ => {
            eprintln!("usage: actuary-cli fx | fairness <members> <trials> <seed>");
            std::process::exit(2);
        }
    }
}

fn fx_ops() {
    let stdin = io::stdin();
    let mut out = io::BufWriter::new(io::stdout());
    for line in stdin.lock().lines() {
        let line = line.expect("stdin");
        let mut it = line.split_whitespace();
        let (Some(op), Some(a)) = (it.next(), it.next()) else { continue };
        let a = Fx(a.parse().expect("raw i128"));
        let b = it.next().map(|v| Fx(v.parse().expect("raw i128")));
        let r = match (op, b) {
            ("exp", _) => a.exp(),
            ("ln", _) => a.ln(),
            ("sqrt", _) => a.sqrt(),
            ("pow", Some(b)) => a.pow(b),
            ("mul", Some(b)) => a.mul(b),
            ("div", Some(b)) => a.div(b),
            _ => panic!("bad op line: {line}"),
        };
        match r {
            Ok(v) => writeln!(out, "{}", v.0),
            Err(e) => writeln!(out, "err {e:?}"),
        }
        .expect("stdout");
    }
}

fn fairness_cmd() {
    let args: Vec<u64> = std::env::args().skip(2).map(|a| a.parse().expect("integer")).collect();
    let (members, trials, seed) = (args[0] as usize, args[1] as usize, args[2]);
    // Round Gompertz–Makeham parameters until the fitted WPP parameters are in (same range).
    let gm = Gm { a: Fx::from_ratio(5, 10_000).unwrap(), b: Fx::from_ratio(3, 100_000).unwrap(), theta: Fx::from_ratio(1, 10).unwrap(), kappa: Fx::from_ratio(15, 1000).unwrap() };
    let r = fairness::run(members, trials, seed, &gm, Fx::from_int(2026));
    println!(
        "{{\"members\":{},\"trials\":{},\"worst_bias\":{:.4e},\"worst_bias_se\":{:.2e},\"rms_bias\":{:.4e},\"mean_abs_z\":{:.3},\"ledger_checks\":{},\"uncorrected_worst_bias\":{:.4e},\"uncorrected_rms_bias\":{:.4e}}}",
        r.members, r.trials, r.worst_bias, r.worst_bias_se, r.rms_bias, r.mean_abs_z, r.ledger_checks, r.uncorrected_worst_bias, r.uncorrected_rms_bias
    );
}

fn fx(v: f64) -> Fx {
    Fx((v * 18_446_744_073_709_551_616.0) as i128)
}

fn fl(v: Fx) -> f64 {
    v.0 as f64 / 18_446_744_073_709_551_616.0
}

fn quote_cmd() {
    use actuary_core::quote::{quote, Market, Plan};
    let raw: Vec<String> = std::env::args().skip(2).collect();
    // Numbers as f64, but the seed must be exact: f64 holds only 53 bits.
    let a: Vec<f64> = raw[..14].iter().map(|v| v.parse().expect("number")).collect();
    let seed: u64 = raw[14].parse().expect("u64 seed");
    let gm = Gm { a: fx(a[0]), b: fx(a[1]), theta: fx(a[2]), kappa: fx(a[3]) };
    let plan = Plan { age: fx(a[4]), year: fx(a[5]), start_age: fx(a[6]), lump_sum: fx(a[7]), monthly_contribution: fx(a[8]) };
    // An optional 16th argument is the pool's annual fee (default 0.30%, as the pool charges).
    let pool_fee = raw.get(15).map(|v| v.parse::<f64>().expect("fee")).unwrap_or(0.003);
    let market = Market { spy_return: fx(a[9]), spy_vol: fx(a[10]), safe_rate: fx(a[11]), pool_fee: fx(pool_fee) };
    let q = quote(&gm, &plan, &market, fx(a[12]), a[13] as u32, seed).expect("quote");
    let f3 = |v: [Fx; 3]| format!("[{:.12},{:.12},{:.12}]", fl(v[0]), fl(v[1]), fl(v[2]));
    println!(
        "{{\"income_start\":{},\"income_at_85\":{},\"solo_runout_age_p50\":{:.12},\"solo_outlive_probability\":{:.12}}}",
        f3(q.income_start), f3(q.income_at_85), fl(q.solo_runout_age_p50), fl(q.solo_outlive_probability)
    );
}

fn ghosts_cmd() {
    let a: Vec<String> = std::env::args().skip(2).collect();
    let (hidden, horizon, runs, seed): (f64, usize, usize, u64) = (a[0].parse().unwrap(), a[1].parse().unwrap(), a[2].parse().unwrap(), a[3].parse().unwrap());
    let cohorts: Vec<(Gm, u32, usize)> = a[4]
        .split(';')
        .filter(|c| !c.is_empty())
        .map(|c| {
            let v: Vec<&str> = c.split(',').collect();
            let f = |i: usize| fx(v[i].parse::<f64>().unwrap());
            (Gm { a: f(0), b: f(1), theta: f(2), kappa: f(3) }, v[4].parse().unwrap(), v[5].parse().unwrap())
        })
        .collect();
    // Optional: α (false-alarm rate per test; default the contract's 0.1%) and the report lag in
    // months (default 5, as the contract uses with monthly epochs).
    let alpha: f64 = a.get(5).map(|v| v.parse().unwrap()).unwrap_or(0.001);
    let lag: usize = a.get(6).map(|v| v.parse().unwrap()).unwrap_or(5);
    // Optional: the group's true mortality against the tables (default 1) and honest deaths'
    // finality delay, first month and span (default 4 and 3: final after 4 to 6 months).
    let health: f64 = a.get(7).map(|v| v.parse().unwrap()).unwrap_or(1.0);
    let delay: (usize, usize) = (a.get(8).map(|v| v.parse().unwrap()).unwrap_or(4), a.get(9).map(|v| v.parse().unwrap()).unwrap_or(3));
    let t = actuary_core::fraud::thresholds_for(fx(alpha), fx(0.05)).unwrap();
    let o = ghosts::run(&cohorts, hidden, horizon, runs, seed, t, lag, health, delay);
    println!(
        "{{\"hidden\":{},\"runs\":{},\"alpha\":{},\"lag\":{},\"health\":{},\"delay\":[{},{}],\"flagged_12m\":{:.4},\"flagged_24m\":{:.4},\"flagged_36m\":{:.4},\"median_months\":{}}}",
        o.hidden, o.runs, alpha, lag, health, delay.0, delay.0 + delay.1 - 1, o.flagged_12m, o.flagged_24m, o.flagged_36m, o.median_months.map(|m| m.to_string()).unwrap_or("null".into())
    );
}

fn replay_cmd() {
    let a: Vec<String> = std::env::args().skip(2).collect();
    let (members, pot, seed): (usize, f64, u64) = (a[0].parse().unwrap(), a[1].parse().unwrap(), a[2].parse().unwrap());
    let months: Vec<replay::Month> = io::stdin()
        .lock()
        .lines()
        .map(|l| {
            let v: Vec<f64> = l.unwrap().split_whitespace().map(|x| x.parse().unwrap()).collect();
            replay::Month { spy: v[0], tbill: v[1], q: v[2], payout: v[3], inflation: v[4] }
        })
        .collect();
    let r = replay::run(&months, members, pot, seed);
    let rows: Vec<String> = r
        .years
        .iter()
        .map(|y| {
            format!(
                "[{},{:.4},{:.4},{:.4},{:.4},{:.4}]",
                y.alive, y.income, y.income_real, y.self_income_real, y.shadow_balance, y.rule4_balance
            )
        })
        .collect();
    let month = |m: Option<usize>| m.map(|m| m.to_string()).unwrap_or("null".into());
    println!(
        "{{\"first_month_income\":{:.6},\"shadow_runout_month\":{},\"rule4_runout_month\":{},\"columns\":[\"alive\",\"income\",\"income_real\",\"self_income_real\",\"shadow_balance\",\"rule4_balance\"],\"years\":[{}]}}",
        r.first_month_income,
        month(r.shadow_runout),
        month(r.rule4_runout),
        rows.join(",")
    );
}
