//! How legion2d names deployments, positions and missions.

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
pub fn unique_deployment_name(base: &str, taken_names: &[String]) -> String {
    let candidates = std::iter::once(base.to_string()).chain((2..).map(|number| format!("{base}-{number}")));
    candidates.into_iter().find(|candidate| !taken_names.contains(candidate)).unwrap_or_else(|| base.to_string())
}

/// The idle copy to end so an operator at its limit can start on new work,
/// or None when a position is free anyway or every copy is busy. An idle
/// copy has handed off; keeping it only saves a restart if work comes back.
pub fn idle_copy_to_free(candidates: &[String], running: &[(String, bool)]) -> Option<String> {
    let running_positions: Vec<String> = running.iter().map(|(position, _)| position.clone()).collect();
    if first_free_position(candidates, &running_positions).is_some() {
        return None;
    }
    running.iter().find(|(position, is_idle)| *is_idle && candidates.contains(position)).map(|(position, _)| position.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_copy_makes_room_only_when_every_copy_is_taken() {
        let candidates = names(&["planner"]);
        assert_eq!(idle_copy_to_free(&candidates, &[("planner".into(), true)]), Some("planner".into()));
        assert_eq!(idle_copy_to_free(&candidates, &[("planner".into(), false)]), None);
        assert_eq!(idle_copy_to_free(&candidates, &[]), None);
        let two = names(&["builder", "builder-2"]);
        assert_eq!(idle_copy_to_free(&two, &[("builder".into(), true)]), None);
        assert_eq!(idle_copy_to_free(&two, &[("builder".into(), false), ("builder-2".into(), true)]), Some("builder-2".into()));
    }

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
    fn deployment_name_gets_a_number_when_taken() {
        assert_eq!(unique_deployment_name("feature", &[]), "feature");
        assert_eq!(unique_deployment_name("feature", &names(&["feature", "feature-2"])), "feature-3");
    }
}
