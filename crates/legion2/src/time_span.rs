//! Spans like 30m, 2h or 1d, for "only entries this recent".

const MILLISECONDS_PER_MINUTE: i64 = 60_000;
const MILLISECONDS_PER_HOUR: i64 = 60 * MILLISECONDS_PER_MINUTE;
const MILLISECONDS_PER_DAY: i64 = 24 * MILLISECONDS_PER_HOUR;

pub fn parse_time_span_ms(span: &str) -> Result<i64, String> {
    let complaint = || format!("can't read {span:?}; try 30m, 2h or 1d");
    let unit_start = span.find(|character: char| !character.is_ascii_digit()).unwrap_or(span.len());
    let (count_text, unit) = span.split_at(unit_start);
    let count: i64 = count_text.parse().map_err(|_| complaint())?;
    let unit_length = match unit {
        "m" => MILLISECONDS_PER_MINUTE,
        "h" => MILLISECONDS_PER_HOUR,
        "d" => MILLISECONDS_PER_DAY,
        _ => return Err(complaint()),
    };
    Ok(count * unit_length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_minutes_hours_and_days() {
        assert_eq!(parse_time_span_ms("30m"), Ok(30 * MILLISECONDS_PER_MINUTE));
        assert_eq!(parse_time_span_ms("2h"), Ok(2 * MILLISECONDS_PER_HOUR));
        assert_eq!(parse_time_span_ms("1d"), Ok(MILLISECONDS_PER_DAY));
    }

    #[test]
    fn refuses_missing_counts_and_unknown_units() {
        assert!(parse_time_span_ms("m").is_err());
        assert!(parse_time_span_ms("5").is_err());
        assert!(parse_time_span_ms("5w").is_err());
        assert!(parse_time_span_ms("").is_err());
    }
}
