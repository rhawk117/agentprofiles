use crate::render::cell::{Line, Segment};

pub const ALWAYS_KEEP: u8 = 0;

#[derive(Debug, Clone)]
pub struct PriorityGroup {
    pub priority: u8,
    pub segments: Vec<Segment>,
}

impl PriorityGroup {
    #[must_use]
    pub const fn new(priority: u8, segments: Vec<Segment>) -> Self {
        Self { priority, segments }
    }

    #[must_use]
    pub fn width(&self) -> usize {
        self.segments.iter().map(Segment::width).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|segment| segment.text.is_empty())
    }
}

#[must_use]
pub fn fit_to_width(groups: Vec<PriorityGroup>, columns: usize) -> Line {
    let mut retained: Vec<PriorityGroup> = groups
        .into_iter()
        .filter(|group| !group.is_empty())
        .collect();

    while total_width(&retained) > columns {
        let Some(index) = index_of_least_important(&retained) else {
            break;
        };
        retained.remove(index);
    }

    let mut line = Line::new();
    for group in retained {
        line.extend(group.segments);
    }
    line.truncated_to(columns)
}

fn total_width(groups: &[PriorityGroup]) -> usize {
    groups.iter().map(PriorityGroup::width).sum()
}

fn index_of_least_important(groups: &[PriorityGroup]) -> Option<usize> {
    groups
        .iter()
        .enumerate()
        .filter(|(_, group)| group.priority > ALWAYS_KEEP)
        .max_by_key(|(index, group)| (group.priority, *index))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::cell::Style;

    fn group(priority: u8, text: &str) -> PriorityGroup {
        PriorityGroup::new(priority, vec![Segment::new(text, Style::plain())])
    }

    #[test]
    fn everything_survives_when_the_budget_is_generous() {
        let line = fit_to_width(vec![group(0, "aaa"), group(1, "bbb"), group(2, "ccc")], 40);
        assert_eq!(line.to_plain_text(), "aaabbbccc");
    }

    #[test]
    fn the_least_important_group_is_dropped_first() {
        let line = fit_to_width(vec![group(0, "aaa"), group(1, "bbb"), group(9, "ccc")], 6);
        assert_eq!(line.to_plain_text(), "aaabbb");
    }

    #[test]
    fn groups_drop_in_descending_priority_order() {
        let line = fit_to_width(vec![group(0, "aa"), group(3, "bb"), group(9, "cc")], 2);
        assert_eq!(line.to_plain_text(), "aa");
    }

    #[test]
    fn rightmost_group_loses_when_priorities_tie() {
        let line = fit_to_width(vec![group(0, "aa"), group(4, "bb"), group(4, "cc")], 4);
        assert_eq!(line.to_plain_text(), "aabb");
    }

    #[test]
    fn protected_groups_are_truncated_rather_than_dropped() {
        let line = fit_to_width(vec![group(0, "aaaaaa"), group(0, "bbbbbb")], 8);
        assert_eq!(line.to_plain_text(), "aaaaaabb");
        assert_eq!(line.width(), 8);
    }

    #[test]
    fn empty_groups_never_consume_budget() {
        let line = fit_to_width(vec![group(0, "aa"), group(1, ""), group(2, "cc")], 4);
        assert_eq!(line.to_plain_text(), "aacc");
    }

    #[test]
    fn output_never_exceeds_the_column_budget() {
        for columns in 0..30 {
            let line = fit_to_width(
                vec![group(0, "aaaa"), group(1, "bbbb"), group(2, "cccc")],
                columns,
            );
            assert!(line.width() <= columns, "overflowed at {columns} columns");
        }
    }
}
