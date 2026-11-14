//! Claude API prices, to weigh the tokens in Claude Code's transcripts into
//! a cost (usage_transcripts.rs). Only proportions between sessions come out
//! of it, so an old table skews them only when sessions use different models.

/// Dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub input: f64,
    pub write_5m: f64,
    pub write_1h: f64,
    pub read: f64,
    pub output: f64,
}

/// When the table was taken from `SOURCE`.
pub const PRICED_ON: &str = "2026-11-14";
pub const SOURCE: &str = "https://platform.claude.com/docs/en/about-claude/pricing";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tokens {
    pub input: u64,
    pub write_5m: u64,
    pub write_1h: u64,
    pub read: u64,
    pub output: u64,
}

const fn p(input: f64, write_5m: f64, write_1h: f64, read: f64, output: f64) -> Price {
    Price { input, write_5m, write_1h, read, output }
}

const FABLE_5_2: Price = p(8.0, 10.0, 16.0, 0.20, 40.0);
const FABLE_5_1: Price = p(10.0, 12.5, 20.0, 0.25, 50.0);
const FABLE_5: Price = p(10.0, 12.5, 20.0, 1.0, 50.0);
const OPUS_5_5: Price = p(4.0, 5.0, 8.0, 0.20, 20.0);
const OPUS_5: Price = p(5.0, 6.25, 10.0, 0.50, 25.0);
const SONNET_5_5: Price = p(2.0, 2.5, 4.0, 0.10, 10.0);
const SONNET_5: Price = p(2.0, 2.5, 4.0, 0.20, 10.0);
const SONNET_4_6: Price = p(3.0, 3.75, 6.0, 0.30, 15.0);
const HAIKU_5_5: Price = p(0.10, 0.125, 0.20, 0.01, 0.50);
/// Haiku 5.5 for prompts over 100,000 tokens.
const HAIKU_5_5_LONG: Price = p(0.50, 0.625, 1.0, 0.05, 2.50);
const HAIKU_4_5: Price = p(1.0, 1.25, 2.0, 0.10, 5.0);

/// Standard prices by model id.
const MODELS: &[(&str, Price)] = &[
    ("claude-fable-5-2", FABLE_5_2),
    ("claude-mythos-5-2", FABLE_5_2),
    ("claude-fable-5-1", FABLE_5_1),
    ("claude-mythos-5-1", FABLE_5_1),
    ("claude-fable-5", FABLE_5),
    ("claude-mythos-5", FABLE_5),
    ("claude-opus-5-5", OPUS_5_5),
    ("claude-opus-5", OPUS_5),
    ("claude-opus-4-8", OPUS_5),
    ("claude-opus-4-7", OPUS_5),
    ("claude-opus-4-6", OPUS_5),
    ("claude-opus-4-5", OPUS_5),
    ("claude-sonnet-5-5", SONNET_5_5),
    ("claude-sonnet-5", SONNET_5),
    ("claude-sonnet-4-6", SONNET_4_6),
    ("claude-sonnet-4-5", SONNET_4_6),
    ("claude-haiku-5-5", HAIKU_5_5),
    ("claude-haiku-4-5", HAIKU_4_5),
];

/// Fast mode's input and output prices (cache prices scale with input).
const FAST: &[(&str, f64, f64)] = &[
    ("claude-fable-5-2", 16.0, 80.0),
    ("claude-mythos-5-2", 16.0, 80.0),
    ("claude-opus-5-5", 8.0, 40.0),
    ("claude-opus-5", 10.0, 50.0),
    ("claude-opus-4-8", 10.0, 50.0),
];

/// "claude-opus-5-5[1m]" or "claude-sonnet-4-5-20250929" as "claude-opus-5-5", "claude-sonnet-4-5".
fn base_id(model: &str) -> &str {
    let id = model.split('[').next().unwrap_or(model);
    match id.rsplit_once('-') {
        Some((head, date)) if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => id,
    }
}

/// The model's price: by its id, else the newest of its family (fable,
/// mythos, opus, sonnet, haiku), else Opus 5.5's.
pub fn price(model: &str, prompt_tokens: u64, fast: bool) -> Price {
    let id = base_id(model);
    let known = MODELS.iter().find(|(m, _)| *m == id).map(|(_, p)| *p);
    let family = || {
        [("fable", FABLE_5_2), ("mythos", FABLE_5_2), ("opus", OPUS_5_5), ("sonnet", SONNET_5_5), ("haiku", HAIKU_5_5)]
            .into_iter()
            .find(|(f, _)| id.contains(f))
            .map(|(_, p)| p)
    };
    let mut price = known.or_else(family).unwrap_or(OPUS_5_5);
    if price == HAIKU_5_5 && prompt_tokens > 100_000 {
        price = HAIKU_5_5_LONG;
    }
    if fast && let Some((_, input, output)) = FAST.iter().find(|(m, _, _)| *m == id) {
        let k = input / price.input;
        price = p(*input, price.write_5m * k, price.write_1h * k, price.read * k, *output);
    }
    price
}

/// Dollars for one message's tokens.
pub fn cost(model: &str, t: &Tokens, fast: bool) -> f64 {
    let prompt = t.input + t.write_5m + t.write_1h + t.read;
    let p = price(model, prompt, fast);
    (t.input as f64 * p.input
        + t.write_5m as f64 * p.write_5m
        + t.write_1h as f64 * p.write_1h
        + t.read as f64 * p.read
        + t.output as f64 * p.output)
        / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(f: impl Fn(&mut Tokens)) -> Tokens {
        let mut t = Tokens::default();
        f(&mut t);
        t
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn prices_each_kind_of_token() {
        const M: u64 = 1_000_000;
        close(cost("claude-fable-5-2", &one(|t| t.input = M), false), 8.0);
        close(cost("claude-opus-5-5", &one(|t| t.output = M), false), 20.0);
        close(cost("claude-fable-5-2", &one(|t| t.read = M), false), 0.20);
        close(cost("claude-opus-5-5", &one(|t| t.write_1h = M), false), 8.0);
        close(cost("claude-opus-5-5", &one(|t| t.write_5m = M), false), 5.0);
        close(cost("claude-sonnet-4-5-20250929", &one(|t| t.input = M), false), 3.0);
    }

    #[test]
    fn haiku_5_5_long_prompts_cost_more() {
        close(cost("claude-haiku-5-5", &one(|t| t.input = 200_000), false), 0.10);
        close(cost("claude-haiku-5-5", &one(|t| t.input = 100_000), false), 0.01);
    }

    #[test]
    fn fast_mode_prices() {
        close(cost("claude-opus-5-5", &one(|t| t.input = 1_000_000), true), 8.0);
        close(cost("claude-fable-5-2", &one(|t| t.output = 1_000_000), true), 80.0);
        close(cost("claude-sonnet-5-5", &one(|t| t.input = 1_000_000), true), 2.0);
    }

    #[test]
    fn unknown_models_take_their_familys_newest_price() {
        assert_eq!(price("claude-fable-9", 0, false), price("claude-fable-5-2", 0, false));
        assert_eq!(price("claude-opus-5-5[1m]", 0, false), price("claude-opus-5-5", 0, false));
        assert_eq!(price("gpt-x", 0, false), price("claude-opus-5-5", 0, false));
        assert_eq!(price("claude-fable-5-3", 0, false), price("claude-fable-5-2", 0, false));
    }
}
