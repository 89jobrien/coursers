//! Heatmap of rule firings by hour-of-day and day-of-week.
//!
//! Consumes the `last_seen` timestamps from `Stats` plus a richer per-firing
//! log (if available). When only aggregate stats are available (just one
//! `last_seen` per rule), the heatmap is sparse but still useful.

use std::collections::HashMap;

/// A cell value in the heatmap: number of blocks at (day, hour).
///
/// day: 0 = Monday … 6 = Sunday
/// hour: 0–23
#[derive(Debug, Default, Clone)]
pub struct HeatMap {
    /// (day, hour) → block count
    pub cells: HashMap<(u8, u8), u64>,
    pub total_blocks: u64,
}

impl HeatMap {
    /// Record one block at the given Unix timestamp (seconds).
    pub fn record(&mut self, unix_secs: u64) {
        let (day, hour) = day_hour(unix_secs);
        *self.cells.entry((day, hour)).or_default() += 1;
        self.total_blocks += 1;
    }

    /// Return the maximum cell value (for normalisation).
    pub fn max_cell(&self) -> u64 {
        self.cells.values().copied().max().unwrap_or(0)
    }

    /// Render as a compact ASCII grid (7 rows × 24 cols).
    /// Intensity: ` ` → `░` → `▒` → `▓` → `█`
    pub fn render(&self) -> String {
        const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let max = self.max_cell().max(1);
        let mut out = String::new();

        // Header
        out.push_str("     ");
        for h in 0..24u8 {
            out.push_str(&format!("{h:02} "));
        }
        out.push('\n');

        for day in 0u8..7 {
            out.push_str(&format!("{} ", DAYS[day as usize]));
            for hour in 0..24u8 {
                let count = self.cells.get(&(day, hour)).copied().unwrap_or(0);
                let intensity = count * 4 / max;
                let ch = match intensity {
                    0 => ' ',
                    1 => '░',
                    2 => '▒',
                    3 => '▓',
                    _ => '█',
                };
                out.push(ch);
                out.push_str("  ");
            }
            out.push('\n');
        }

        out
    }
}

/// Build a HeatMap from a list of (rule_id, unix_timestamp_secs) pairs.
/// Pass all firing timestamps you have; each one is recorded as one cell hit.
pub fn build(firings: &[(String, u64)]) -> HeatMap {
    let mut hm = HeatMap::default();
    for (_, ts) in firings {
        hm.record(*ts);
    }
    hm
}

/// Convert a Unix timestamp to (day_of_week, hour_of_day).
/// day: 0 = Monday, 6 = Sunday (ISO weekday − 1).
/// Uses a simple integer calculation — no external crate needed.
fn day_hour(unix_secs: u64) -> (u8, u8) {
    // 1970-01-01 was a Thursday → weekday index 3 (Mon=0)
    let days = unix_secs / 86400;
    let hour = ((unix_secs % 86400) / 3600) as u8;
    let day = ((days + 3) % 7) as u8; // +3 because epoch = Thu
    (day, hour)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_heatmap_has_no_cells() {
        let hm = build(&[]);
        assert_eq!(hm.total_blocks, 0);
        assert!(hm.cells.is_empty());
    }

    #[test]
    fn single_firing_records_correct_day_and_hour() {
        // 1970-01-01 00:00:00 UTC = Thursday = day index 3, hour 0
        let hm = build(&[("no-grep".to_string(), 0)]);
        assert_eq!(hm.cells.get(&(3, 0)), Some(&1));
        assert_eq!(hm.total_blocks, 1);
    }

    #[test]
    fn multiple_firings_accumulate_in_same_cell() {
        let firings = vec![("r1".to_string(), 0), ("r2".to_string(), 0)];
        let hm = build(&firings);
        assert_eq!(hm.cells.get(&(3, 0)), Some(&2));
    }

    #[test]
    fn hour_extraction_is_correct() {
        // 3600 secs = 1 hour past midnight on Thursday
        let (day, hour) = day_hour(3600);
        assert_eq!(day, 3);
        assert_eq!(hour, 1);
    }

    #[test]
    fn max_cell_returns_highest_count() {
        let mut hm = HeatMap::default();
        hm.cells.insert((0, 9), 10);
        hm.cells.insert((1, 14), 3);
        assert_eq!(hm.max_cell(), 10);
    }

    #[test]
    fn render_produces_seven_day_rows() {
        let hm = build(&[("r".to_string(), 0)]);
        let rendered = hm.render();
        let day_lines: Vec<&str> = rendered
            .lines()
            .skip(1) // skip header
            .collect();
        assert_eq!(day_lines.len(), 7);
    }

    #[test]
    fn render_header_has_24_hour_labels() {
        let hm = HeatMap::default();
        let rendered = hm.render();
        let header = rendered.lines().next().unwrap();
        // "23" must appear in the header
        assert!(header.contains("23"));
    }

    #[test]
    fn day_wraps_correctly_for_sunday() {
        // 1970-01-04 = Sunday = day index 6
        let (day, _) = day_hour(3 * 86400);
        assert_eq!(day, 6);
    }
}
