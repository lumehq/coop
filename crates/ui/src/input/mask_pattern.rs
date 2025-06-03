use gpui::SharedString;

#[derive(Clone, PartialEq, Debug)]
pub enum MaskToken {
    /// 0 Digit, equivalent to `[0]`
    // Digit0,
    /// Digit, equivalent to `[0-9]`
    Digit,
    /// Letter, equivalent to `[a-zA-Z]`
    Letter,
    /// Letter or digit, equivalent to `[a-zA-Z0-9]`
    LetterOrDigit,
    /// Separator
    Sep(char),
    /// Any character
    Any,
}

#[allow(unused)]
impl MaskToken {
    /// Check if the token is any character.
    pub fn is_any(&self) -> bool {
        matches!(self, MaskToken::Any)
    }

    /// Check if the token is a match for the given character.
    ///
    /// The separator is always a match any input character.
    fn is_match(&self, ch: char) -> bool {
        match self {
            MaskToken::Digit => ch.is_ascii_digit(),
            MaskToken::Letter => ch.is_ascii_alphabetic(),
            MaskToken::LetterOrDigit => ch.is_ascii_alphanumeric(),
            MaskToken::Any => true,
            MaskToken::Sep(c) => *c == ch,
        }
    }

    /// Is the token a separator (Can be ignored)
    fn is_sep(&self) -> bool {
        matches!(self, MaskToken::Sep(_))
    }

    /// Check if the token is a number.
    pub fn is_number(&self) -> bool {
        matches!(self, MaskToken::Digit)
    }

    pub fn placeholder(&self) -> char {
        match self {
            MaskToken::Sep(c) => *c,
            _ => '_',
        }
    }

    fn mask_char(&self, ch: char) -> char {
        match self {
            MaskToken::Digit | MaskToken::LetterOrDigit | MaskToken::Letter => ch,
            MaskToken::Sep(c) => *c,
            MaskToken::Any => ch,
        }
    }

    fn unmask_char(&self, ch: char) -> Option<char> {
        match self {
            MaskToken::Digit => Some(ch),
            MaskToken::Letter => Some(ch),
            MaskToken::LetterOrDigit => Some(ch),
            MaskToken::Any => Some(ch),
            _ => None,
        }
    }
}

#[derive(Clone, Default)]
pub enum MaskPattern {
    #[default]
    None,
    Pattern {
        pattern: SharedString,
        tokens: Vec<MaskToken>,
    },
    Number {
        /// Group separator, e.g. "," or " "
        separator: Option<char>,
        /// Number of fraction digits, e.g. 2 for 123.45
        fraction: Option<usize>,
    },
}

impl From<&str> for MaskPattern {
    fn from(pattern: &str) -> Self {
        Self::new(pattern)
    }
}

impl MaskPattern {
    /// Create a new mask pattern
    ///
    /// - `9` - Digit
    /// - `A` - Letter
    /// - `#` - Letter or Digit
    /// - `*` - Any character
    /// - other characters - Separator
    ///
    /// For example:
    ///
    /// - `(999)999-9999` - US phone number: (123)456-7890
    /// - `99999-9999` - ZIP code: 12345-6789
    /// - `AAAA-99-####` - Custom pattern: ABCD-12-3AB4
    /// - `*999*` - Custom pattern: (123) or [123]
    pub fn new(pattern: &str) -> Self {
        let tokens = pattern
            .chars()
            .map(|ch| match ch {
                // '0' => MaskToken::Digit0,
                '9' => MaskToken::Digit,
                'A' => MaskToken::Letter,
                '#' => MaskToken::LetterOrDigit,
                '*' => MaskToken::Any,
                _ => MaskToken::Sep(ch),
            })
            .collect();

        Self::Pattern {
            pattern: pattern.to_owned().into(),
            tokens,
        }
    }

    #[allow(unused)]
    fn tokens(&self) -> Option<&Vec<MaskToken>> {
        match self {
            Self::Pattern { tokens, .. } => Some(tokens),
            Self::Number { .. } => None,
            Self::None => None,
        }
    }

    /// Create a new mask pattern with group separator, e.g. "," or " "
    pub fn number(sep: Option<char>) -> Self {
        Self::Number {
            separator: sep,
            fraction: None,
        }
    }

    pub fn placeholder(&self) -> Option<String> {
        match self {
            Self::Pattern { tokens, .. } => {
                Some(tokens.iter().map(|token| token.placeholder()).collect())
            }
            Self::Number { .. } => None,
            Self::None => None,
        }
    }

    /// Return true if the mask pattern is None or no any pattern.
    pub fn is_none(&self) -> bool {
        match self {
            Self::Pattern { tokens, .. } => tokens.is_empty(),
            Self::Number { .. } => false,
            Self::None => true,
        }
    }

    /// Check is the mask text is valid.
    ///
    /// If the mask pattern is None, always return true.
    pub fn is_valid(&self, mask_text: &str) -> bool {
        if self.is_none() {
            return true;
        }

        let mut text_index = 0;
        let mask_text_chars: Vec<char> = mask_text.chars().collect();
        match self {
            Self::Pattern { tokens, .. } => {
                for token in tokens {
                    if text_index >= mask_text_chars.len() {
                        break;
                    }

                    let ch = mask_text_chars[text_index];
                    if token.is_match(ch) {
                        text_index += 1;
                    }
                }
                text_index == mask_text.len()
            }
            Self::Number { separator, .. } => {
                if mask_text.is_empty() {
                    return true;
                }

                // check if the text is valid number
                let mut parts = mask_text.split('.');
                let int_part = parts.next().unwrap_or("");
                let frac_part = parts.next();

                if int_part.is_empty() {
                    return false;
                }

                // check if the integer part is valid
                if !int_part
                    .chars()
                    .all(|ch| ch.is_ascii_digit() || Some(ch) == *separator)
                {
                    return false;
                }

                // check if the fraction part is valid
                if let Some(frac) = frac_part {
                    if !frac
                        .chars()
                        .all(|ch| ch.is_ascii_digit() || Some(ch) == *separator)
                    {
                        return false;
                    }
                }

                true
            }
            Self::None => true,
        }
    }

    /// Check if valid input char at the given position.
    pub fn is_valid_at(&self, ch: char, pos: usize) -> bool {
        if self.is_none() {
            return true;
        }

        match self {
            Self::Pattern { tokens, .. } => {
                if let Some(token) = tokens.get(pos) {
                    if token.is_match(ch) {
                        return true;
                    }

                    if token.is_sep() {
                        // If next token is match, it's valid
                        if let Some(next_token) = tokens.get(pos + 1) {
                            if next_token.is_match(ch) {
                                return true;
                            }
                        }
                    }
                }

                false
            }
            Self::Number { .. } => true,
            Self::None => true,
        }
    }

    /// Format the text according to the mask pattern
    ///
    /// For example:
    ///
    /// - pattern: (999)999-999
    /// - text: 123456789
    /// - mask_text: (123)456-789
    pub fn mask(&self, text: &str) -> SharedString {
        if self.is_none() {
            return text.to_owned().into();
        }

        match self {
            Self::Number {
                separator,
                fraction,
            } => {
                if let Some(sep) = *separator {
                    // Remove the existing group separator
                    let text = text.replace(sep, "");

                    let mut parts = text.split('.');
                    let int_part = parts.next().unwrap_or("");

                    // Limit the fraction part to the given range, if not enough, pad with 0
                    let frac_part = parts.next().map(|part| {
                        part.chars()
                            .take(fraction.unwrap_or(usize::MAX))
                            .collect::<String>()
                    });

                    // Reverse the integer part for easier grouping
                    let chars: Vec<char> = int_part.chars().rev().collect();
                    let mut result = String::new();
                    for (i, ch) in chars.iter().enumerate() {
                        if i > 0 && i % 3 == 0 {
                            result.push(sep);
                        }
                        result.push(*ch);
                    }
                    let int_with_sep: String = result.chars().rev().collect();

                    let final_str = if let Some(frac) = frac_part {
                        if fraction == &Some(0) {
                            int_with_sep
                        } else {
                            format!("{}.{}", int_with_sep, frac)
                        }
                    } else {
                        int_with_sep
                    };
                    return final_str.into();
                }

                text.to_owned().into()
            }
            Self::Pattern { tokens, .. } => {
                let mut result = String::new();
                let mut text_index = 0;
                let text_chars: Vec<char> = text.chars().collect();
                for (pos, token) in tokens.iter().enumerate() {
                    if text_index >= text_chars.len() {
                        break;
                    }
                    let ch = text_chars[text_index];
                    // Break if expected char is not match
                    if !token.is_sep() && !self.is_valid_at(ch, pos) {
                        break;
                    }
                    let mask_ch = token.mask_char(ch);
                    result.push(mask_ch);
                    if ch == mask_ch {
                        text_index += 1;
                        continue;
                    }
                }
                result.into()
            }
            Self::None => text.to_owned().into(),
        }
    }

    /// Extract original text from masked text
    pub fn unmask(&self, mask_text: &str) -> String {
        match self {
            Self::Number { separator, .. } => {
                if let Some(sep) = *separator {
                    let mut result = String::new();
                    for ch in mask_text.chars() {
                        if ch == sep {
                            continue;
                        }
                        result.push(ch);
                    }

                    if result.contains('.') {
                        result = result.trim_end_matches('0').to_string();
                    }
                    return result;
                }

                mask_text.to_owned()
            }
            Self::Pattern { tokens, .. } => {
                let mut result = String::new();
                let mask_text_chars: Vec<char> = mask_text.chars().collect();
                for (text_index, token) in tokens.iter().enumerate() {
                    if text_index >= mask_text_chars.len() {
                        break;
                    }
                    let ch = mask_text_chars[text_index];
                    let unmask_ch = token.unmask_char(ch);
                    if let Some(ch) = unmask_ch {
                        result.push(ch);
                    }
                }
                result
            }
            Self::None => mask_text.to_owned(),
        }
    }
}
