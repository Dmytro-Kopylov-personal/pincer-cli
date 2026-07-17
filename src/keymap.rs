use crossterm::event::KeyCode;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum KeymapPreset {
    #[default]
    Vim,
    Plain,
}

impl KeymapPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vim => "vim",
            Self::Plain => "plain",
        }
    }
}

impl FromStr for KeymapPreset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "vim" | "default" => Ok(Self::Vim),
            "plain" | "non-vim" | "nonvim" => Ok(Self::Plain),
            _ => Err(format!(
                "unknown keymap preset '{s}' (expected 'vim' or 'plain')"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContext {
    List,
    Comments,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    ToggleHelp,
    ToggleProfiling,
    Quit,
    Escape,
    MoveDown,
    MoveUp,
    JumpTop,
    JumpBottom,
    Refresh,
    NextPage,
    PrevPage,
    CycleFeed,
    OpenComments,
    OpenStoryLink,
    OpenCommentsThread,
    OpenCommentPermalink,
    ToggleCommentCollapse,
    StartSearch,
    NextMatch,
    NextHighScore,
    SearchCancel,
    SearchApply,
    SearchBackspace,
}

#[derive(Debug, Clone, Copy)]
pub struct Keymap {
    preset: KeymapPreset,
}

impl Keymap {
    pub fn new(preset: KeymapPreset) -> Self {
        Self { preset }
    }

    pub fn preset(self) -> KeymapPreset {
        self.preset
    }

    pub fn action_for(self, context: KeyContext, code: KeyCode) -> Option<KeyAction> {
        Self::global_action(code).or_else(|| match self.preset {
            KeymapPreset::Vim => Self::vim_action(context, code),
            KeymapPreset::Plain => Self::plain_action(context, code),
        })
    }

    fn global_action(code: KeyCode) -> Option<KeyAction> {
        match code {
            KeyCode::Char('?') => Some(KeyAction::ToggleHelp),
            KeyCode::Char('p') => Some(KeyAction::ToggleProfiling),
            KeyCode::Char('q') => Some(KeyAction::Quit),
            KeyCode::Esc | KeyCode::Backspace => Some(KeyAction::Escape),
            KeyCode::Char('o') => Some(KeyAction::OpenStoryLink),
            KeyCode::Char('b') => Some(KeyAction::OpenCommentsThread),
            _ => None,
        }
    }

    fn vim_action(context: KeyContext, code: KeyCode) -> Option<KeyAction> {
        match context {
            KeyContext::Search => match code {
                KeyCode::Esc => Some(KeyAction::SearchCancel),
                KeyCode::Enter => Some(KeyAction::SearchApply),
                KeyCode::Backspace => Some(KeyAction::SearchBackspace),
                _ => None,
            },
            KeyContext::List => match code {
                KeyCode::Char('j') | KeyCode::Down => Some(KeyAction::MoveDown),
                KeyCode::Char('k') | KeyCode::Up => Some(KeyAction::MoveUp),
                KeyCode::Char('g') => Some(KeyAction::JumpTop),
                KeyCode::Char('G') => Some(KeyAction::JumpBottom),
                KeyCode::Char('r') => Some(KeyAction::Refresh),
                KeyCode::Char(']') | KeyCode::PageDown => Some(KeyAction::NextPage),
                KeyCode::Char('[') | KeyCode::PageUp => Some(KeyAction::PrevPage),
                KeyCode::Tab => Some(KeyAction::CycleFeed),
                KeyCode::Enter => Some(KeyAction::OpenComments),
                _ => None,
            },
            KeyContext::Comments => match code {
                KeyCode::Char('j') | KeyCode::Down => Some(KeyAction::MoveDown),
                KeyCode::Char('k') | KeyCode::Up => Some(KeyAction::MoveUp),
                KeyCode::Char('g') => Some(KeyAction::JumpTop),
                KeyCode::Char('G') => Some(KeyAction::JumpBottom),
                KeyCode::Char('r') => Some(KeyAction::Refresh),
                KeyCode::Char('c') => Some(KeyAction::OpenCommentPermalink),
                KeyCode::Char('z') => Some(KeyAction::ToggleCommentCollapse),
                KeyCode::Char('/') => Some(KeyAction::StartSearch),
                KeyCode::Char('n') => Some(KeyAction::NextMatch),
                KeyCode::Char('H') => Some(KeyAction::NextHighScore),
                _ => None,
            },
        }
    }

    fn plain_action(context: KeyContext, code: KeyCode) -> Option<KeyAction> {
        match context {
            KeyContext::Search => match code {
                KeyCode::Esc => Some(KeyAction::SearchCancel),
                KeyCode::Enter => Some(KeyAction::SearchApply),
                KeyCode::Backspace => Some(KeyAction::SearchBackspace),
                _ => None,
            },
            KeyContext::List => match code {
                KeyCode::Down => Some(KeyAction::MoveDown),
                KeyCode::Up => Some(KeyAction::MoveUp),
                KeyCode::Home => Some(KeyAction::JumpTop),
                KeyCode::End => Some(KeyAction::JumpBottom),
                KeyCode::Char('r') => Some(KeyAction::Refresh),
                KeyCode::PageDown => Some(KeyAction::NextPage),
                KeyCode::PageUp => Some(KeyAction::PrevPage),
                KeyCode::Tab => Some(KeyAction::CycleFeed),
                KeyCode::Enter => Some(KeyAction::OpenComments),
                _ => None,
            },
            KeyContext::Comments => match code {
                KeyCode::Down => Some(KeyAction::MoveDown),
                KeyCode::Up => Some(KeyAction::MoveUp),
                KeyCode::Home => Some(KeyAction::JumpTop),
                KeyCode::End => Some(KeyAction::JumpBottom),
                KeyCode::Char('r') => Some(KeyAction::Refresh),
                KeyCode::Char('c') => Some(KeyAction::OpenCommentPermalink),
                KeyCode::Char('z') => Some(KeyAction::ToggleCommentCollapse),
                KeyCode::Char('/') => Some(KeyAction::StartSearch),
                KeyCode::Char('n') => Some(KeyAction::NextMatch),
                KeyCode::Char('H') => Some(KeyAction::NextHighScore),
                _ => None,
            },
        }
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Self::new(KeymapPreset::default())
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyAction, KeyContext, Keymap, KeymapPreset};
    use crossterm::event::KeyCode;

    #[test]
    fn default_preset_is_vim() {
        assert_eq!(Keymap::default().preset(), KeymapPreset::Vim);
    }

    #[test]
    fn vim_keeps_existing_jk_navigation() {
        let map = Keymap::new(KeymapPreset::Vim);
        assert_eq!(
            map.action_for(KeyContext::List, KeyCode::Char('j')),
            Some(KeyAction::MoveDown)
        );
        assert_eq!(
            map.action_for(KeyContext::List, KeyCode::Char('k')),
            Some(KeyAction::MoveUp)
        );
    }

    #[test]
    fn plain_uses_home_end_instead_of_vim_jumps() {
        let map = Keymap::new(KeymapPreset::Plain);
        assert_eq!(
            map.action_for(KeyContext::List, KeyCode::Home),
            Some(KeyAction::JumpTop)
        );
        assert_eq!(map.action_for(KeyContext::List, KeyCode::Char('g')), None);
    }
}
