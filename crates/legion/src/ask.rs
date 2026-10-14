//! Getting each input the same way everywhere: from its flag when given;
//! otherwise from a prompt when there's a terminal; otherwise an error that
//! names the flag. Lists with one choice pick it without asking.

use std::io::IsTerminal;

use crate::arguments::Choice;

pub struct Asker {
    is_interactive: bool,
}

/// A prompt was cancelled (Esc or Ctrl-C), or failed.
fn asked<T>(answer: std::io::Result<T>) -> Result<T, String> {
    answer.map_err(|error| match error.kind() {
        std::io::ErrorKind::Interrupted => "cancelled".to_string(),
        _ => error.to_string(),
    })
}

/// One thing to pick: its value, what the list says, and a hint beside it.
pub struct Item<T> {
    pub value: T,
    pub label: String,
    pub hint: String,
}

pub fn item<T>(value: T, label: impl ToString, hint: impl ToString) -> Item<T> {
    Item { value, label: label.to_string(), hint: hint.to_string() }
}

/// How a long text arrives when it isn't a flag.
#[derive(Clone, PartialEq, Eq)]
enum TextSource {
    Typed,
    File,
}

pub fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path.trim()).map_err(|error| format!("{}: {error}", path.trim()))
}

impl Asker {
    pub fn new() -> Asker {
        Asker { is_interactive: std::io::stdin().is_terminal() && std::io::stdout().is_terminal() }
    }

    pub fn is_interactive(&self) -> bool {
        self.is_interactive
    }

    fn missing(&self, flag: &str) -> String {
        format!("say it with {flag}: there's no terminal to ask in")
    }

    pub fn note(&self, text: impl std::fmt::Display) {
        match self.is_interactive {
            true => {
                let _ = cliclack::log::info(text);
            }
            false => eprintln!("{text}"),
        }
    }

    pub fn success(&self, text: impl std::fmt::Display) {
        match self.is_interactive {
            true => {
                let _ = cliclack::log::success(text);
            }
            false => println!("{text}"),
        }
    }

    /// What to do, when the command was given without its action word.
    pub fn action<A: Choice>(&self, given: Option<A>, flag_hint: &str) -> Result<A, String> {
        if let Some(action) = given {
            return Ok(action);
        }
        if !self.is_interactive {
            let words: Vec<String> = A::every().iter().map(Choice::word).collect();
            return Err(format!("say what to do: {flag_hint} {}", words.join("|")));
        }
        let mut menu = cliclack::select("What do you want to do?");
        for action in A::every() {
            let label = action.label();
            menu = menu.item(action, label, "");
        }
        asked(menu.interact())
    }

    pub fn text(&self, given: Option<String>, prompt: &str, placeholder: &str, flag: &str) -> Result<String, String> {
        match given {
            Some(text) => Ok(text),
            None if self.is_interactive => asked(cliclack::input(prompt).placeholder(placeholder).interact()),
            None => Err(self.missing(flag)),
        }
    }

    /// A short text with a suggestion the admin can keep with Enter.
    pub fn text_or_default(&self, given: Option<String>, prompt: &str, default: &str) -> Result<String, String> {
        match given {
            Some(text) => Ok(text),
            None if self.is_interactive => asked(cliclack::input(prompt).default_input(default).interact()),
            None => Ok(default.to_string()),
        }
    }

    /// A text that may be left empty: None when it is.
    pub fn optional_text(&self, given: Option<String>, prompt: &str, placeholder: &str) -> Result<Option<String>, String> {
        let text: String = match given {
            Some(text) => text,
            None if self.is_interactive => asked(cliclack::input(prompt).placeholder(placeholder).required(false).interact())?,
            None => return Ok(None),
        };
        Ok(Some(text).filter(|text| !text.trim().is_empty()))
    }

    /// A long text: typed, or read from a file, the admin's choice first.
    pub fn long_text(&self, text: Option<String>, file: Option<String>, prompt: &str, placeholder: &str) -> Result<String, String> {
        if let Some(text) = text {
            return Ok(text);
        }
        if let Some(path) = file {
            return read_file(&path);
        }
        if !self.is_interactive {
            return Err(self.missing("--text or --file"));
        }
        let source = asked(
            cliclack::select(prompt)
                .item(TextSource::Typed, "Type it", "Enter twice to finish")
                .item(TextSource::File, "From a file", "a path")
                .interact(),
        )?;
        match source {
            TextSource::Typed => asked(cliclack::input(prompt).placeholder(placeholder).multiline().interact()),
            TextSource::File => {
                let path: String = asked(
                    cliclack::input("Which file?")
                        .placeholder("./mission.md")
                        .validate(|path: &String| match std::path::Path::new(path.trim()).is_file() {
                            true => Ok(()),
                            false => Err("no such file"),
                        })
                        .interact(),
                )?;
                read_file(&path)
            }
        }
    }

    /// One of `items`. Given, it's taken as is; one item is picked without
    /// asking; none is `empty`.
    pub fn pick<T: Clone + Eq>(&self, given: Option<T>, prompt: &str, items: Vec<Item<T>>, flag: &str, empty: &str) -> Result<T, String> {
        if let Some(value) = given {
            return Ok(value);
        }
        match items.as_slice() {
            [] => Err(empty.to_string()),
            [only] => Ok(only.value.clone()),
            _ if !self.is_interactive => Err(self.missing(flag)),
            _ => {
                let mut menu = cliclack::select(prompt);
                for choice in items {
                    menu = menu.item(choice.value, choice.label, choice.hint);
                }
                asked(menu.interact())
            }
        }
    }

    /// Some of `items`, all ticked to begin with.
    pub fn pick_many<T: Clone + Eq>(&self, given: Option<Vec<T>>, prompt: &str, items: Vec<Item<T>>, flag: &str) -> Result<Vec<T>, String> {
        if let Some(values) = given {
            return Ok(values);
        }
        if !self.is_interactive {
            return Ok(items.into_iter().map(|choice| choice.value).collect());
        }
        let everyone: Vec<T> = items.iter().map(|choice| choice.value.clone()).collect();
        let mut menu = cliclack::multiselect(format!("{prompt} (space to tick, Enter when done)")).initial_values(everyone).required(true);
        for choice in items {
            menu = menu.item(choice.value, choice.label, choice.hint);
        }
        let _ = flag;
        asked(menu.interact())
    }

    /// Several of `items` in an order: picked one at a time.
    pub fn pick_in_order(&self, given: Option<Vec<String>>, items: Vec<Item<String>>, flag: &str) -> Result<Vec<String>, String> {
        if let Some(values) = given {
            return Ok(values);
        }
        if !self.is_interactive {
            return Err(self.missing(flag));
        }
        const DONE: &str = "\u{0}done";
        let mut chosen: Vec<String> = Vec::new();
        loop {
            let left: Vec<&Item<String>> = items.iter().filter(|choice| !chosen.contains(&choice.value)).collect();
            if left.is_empty() {
                return Ok(chosen);
            }
            let mut menu = cliclack::select(format!("Step {}", chosen.len() + 1));
            if !chosen.is_empty() {
                menu = menu.item(DONE.to_string(), "That's all", chosen.join(" → "));
            }
            for choice in left {
                menu = menu.item(choice.value.clone(), &choice.label, &choice.hint);
            }
            match asked(menu.interact())? {
                done if done == DONE => return Ok(chosen),
                step => chosen.push(step),
            }
        }
    }

    /// Whether to go ahead with something that can't be undone.
    pub fn confirm(&self, is_given: bool, prompt: &str) -> Result<bool, String> {
        match is_given {
            true => Ok(true),
            false if self.is_interactive => asked(cliclack::confirm(prompt).initial_value(false).interact()),
            false => Err(self.missing("--yes")),
        }
    }

    pub fn yes_no(&self, prompt: &str, default: bool) -> Result<bool, String> {
        match self.is_interactive {
            true => asked(cliclack::confirm(prompt).initial_value(default).interact()),
            false => Ok(default),
        }
    }

    pub fn spinner(&self, message: &str) -> Option<cliclack::ProgressBar> {
        self.is_interactive.then(|| {
            let spinner = cliclack::spinner();
            spinner.start(message);
            spinner
        })
    }

    pub fn intro(&self, title: &str) {
        if self.is_interactive {
            let _ = cliclack::intro(console::style(format!(" {title} ")).on_cyan().black());
        }
    }

    pub fn outro(&self, text: &str) {
        match self.is_interactive {
            true => {
                let _ = cliclack::outro(text);
            }
            false => println!("{text}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> Asker {
        Asker { is_interactive: false }
    }

    #[test]
    fn a_flag_wins_and_a_missing_one_is_named() {
        assert_eq!(quiet().text(Some("hi".into()), "Title?", "", "--title"), Ok("hi".into()));
        assert!(quiet().text(None, "Title?", "", "--title").unwrap_err().contains("--title"));
        assert!(quiet().long_text(None, None, "What?", "").unwrap_err().contains("--text or --file"));
        assert!(quiet().confirm(false, "Sure?").unwrap_err().contains("--yes"));
        assert_eq!(quiet().confirm(true, "Sure?"), Ok(true));
    }

    #[test]
    fn one_choice_is_picked_and_none_says_why() {
        let one = vec![item("builder".to_string(), "builder", "")];
        assert_eq!(quiet().pick(None, "Who?", one, "--to", "nobody"), Ok("builder".to_string()));
        assert_eq!(quiet().pick::<String>(None, "Who?", vec![], "--to", "nobody is running"), Err("nobody is running".into()));
        let two = vec![item("a".to_string(), "a", ""), item("b".to_string(), "b", "")];
        assert!(quiet().pick(None, "Who?", two, "--to", "").unwrap_err().contains("--to"));
    }

    #[test]
    fn optional_text_left_out_or_blank_is_none() {
        assert_eq!(quiet().optional_text(None, "Check?", ""), Ok(None));
        assert_eq!(quiet().optional_text(Some("  ".into()), "Check?", ""), Ok(None));
        assert_eq!(quiet().optional_text(Some("npm test".into()), "Check?", ""), Ok(Some("npm test".into())));
    }
}
