//! How legion2d names runs, positions and missions.

use crate::constants::MISSION_SLUG_MAX_LENGTH;

/// A mission's title as a file and branch name: "Add a hello file" → "add-a-hello-file".
pub fn mission_slug(title: &str) -> String {
    let dashed: String = title
        .to_lowercase()
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '-' })
        .collect();
    let joined_words = dashed.split('-').filter(|word| !word.is_empty()).collect::<Vec<_>>().join("-");
    joined_words.chars().take(MISSION_SLUG_MAX_LENGTH).collect()
}

/// The operator a position belongs to: builder-2 is a copy of builder.
pub fn operator_of_position(position: &str) -> &str {
    let copy_number_split = position.rsplit_once('-');
    let is_numbered_copy = copy_number_split
        .is_some_and(|(_, copy_number)| !copy_number.is_empty() && copy_number.chars().all(|c| c.is_ascii_digit()));
    match copy_number_split {
        Some((operator, _)) if is_numbered_copy => operator,
        _ => position,
    }
}

/// The positions an operator's copies take: builder, builder-2, builder-3 …
pub fn copy_positions(operator: &str, copy_limit: u32) -> Vec<String> {
    (1..=copy_limit)
        .map(|copy_number| if copy_number == 1 { operator.to_string() } else { format!("{operator}-{copy_number}") })
        .collect()
}

pub fn first_free_position(candidates: &[String], running_positions: &[String]) -> Option<String> {
    candidates.iter().find(|candidate| !running_positions.contains(candidate)).cloned()
}

/// `base`, or `base-2`, `base-3` … whichever isn't taken yet.
pub fn unique_run_name(base: &str, taken_names: &[String]) -> String {
    let candidates = std::iter::once(base.to_string()).chain((2..).map(|number| format!("{base}-{number}")));
    candidates.into_iter().find(|candidate| !taken_names.contains(candidate)).unwrap_or_else(|| base.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn slug_lowercases_and_dashes_words() {
        assert_eq!(mission_slug("Add a hello file"), "add-a-hello-file");
    }

    #[test]
    fn slug_collapses_punctuation_and_trims_dashes() {
        assert_eq!(mission_slug("  Fix: the (login) bug!! "), "fix-the-login-bug");
    }

    #[test]
    fn slug_caps_length() {
        assert_eq!(mission_slug(&"a".repeat(100)).len(), MISSION_SLUG_MAX_LENGTH);
    }

    #[test]
    fn slug_of_nothing_is_empty() {
        assert_eq!(mission_slug("!!!"), "");
    }

    #[test]
    fn numbered_copy_belongs_to_its_operator() {
        assert_eq!(operator_of_position("builder-2"), "builder");
        assert_eq!(operator_of_position("builder-12"), "builder");
    }

    #[test]
    fn plain_and_hyphenated_names_are_their_own_operator() {
        assert_eq!(operator_of_position("builder"), "builder");
        assert_eq!(operator_of_position("code-reviewer"), "code-reviewer");
        assert_eq!(operator_of_position("builder-"), "builder-");
    }

    #[test]
    fn copies_are_numbered_from_two() {
        assert_eq!(copy_positions("builder", 3), names(&["builder", "builder-2", "builder-3"]));
        assert!(copy_positions("builder", 0).is_empty());
    }

    #[test]
    fn first_free_position_skips_running_ones() {
        let candidates = names(&["builder", "builder-2"]);
        assert_eq!(first_free_position(&candidates, &names(&["builder"])), Some("builder-2".into()));
        assert_eq!(first_free_position(&candidates, &names(&["builder", "builder-2"])), None);
    }

    #[test]
    fn run_name_gets_a_number_when_taken() {
        assert_eq!(unique_run_name("feature", &[]), "feature");
        assert_eq!(unique_run_name("feature", &names(&["feature", "feature-2"])), "feature-3");
    }
}
