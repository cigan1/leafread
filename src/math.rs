//! Best-effort LaTeX -> Unicode rendering for terminal display.
//!
//! This is intentionally not a full TeX engine: it covers the constructs that
//! appear in technical Markdown (symbols, sums/integrals, fractions, roots,
//! superscripts/subscripts, accents) and degrades to readable text otherwise.

/// Convert a LaTeX fragment to a Unicode string.
pub fn render(tex: &str) -> String {
    let mut parser = Parser::new(tex);
    parser.sequence(None)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    /// Parse until `stop` (consumed) or end of input.
    fn sequence(&mut self, stop: Option<char>) -> String {
        let mut out = String::new();
        while let Some(ch) = self.peek() {
            match ch {
                '\\' => {
                    self.pos += 1;
                    out.push_str(&self.command());
                }
                '^' => {
                    self.pos += 1;
                    let group = self.group();
                    out.push_str(&super_script(&group));
                }
                '_' => {
                    self.pos += 1;
                    let group = self.group();
                    out.push_str(&sub_script(&group));
                }
                '{' => {
                    self.pos += 1;
                    out.push_str(&self.sequence(Some('}')));
                }
                '}' => {
                    if stop == Some('}') {
                        self.pos += 1;
                        return out;
                    }
                    self.pos += 1;
                }
                '&' => {
                    self.pos += 1;
                    out.push(' ');
                }
                '$' => {
                    self.pos += 1;
                }
                '~' => {
                    self.pos += 1;
                    out.push(' ');
                }
                c if stop == Some(c) => {
                    self.pos += 1;
                    return out;
                }
                c => {
                    self.pos += 1;
                    out.push(c);
                }
            }
        }
        out
    }

    /// Parse a group: `{...}` or a single token.
    fn group(&mut self) -> String {
        self.skip_spaces();
        if self.peek() == Some('{') {
            self.pos += 1;
            let inner = self.sequence(Some('}'));
            return inner.trim().to_string();
        }
        if self.peek() == Some('\\') {
            self.pos += 1;
            return self.command().trim().to_string();
        }
        self.bump().map(|c| c.to_string()).unwrap_or_default()
    }

    /// Parse an optional `[...]` argument.
    fn optional_group(&mut self) -> Option<String> {
        self.skip_spaces();
        if self.peek() != Some('[') {
            return None;
        }
        self.pos += 1;
        let inner = self.sequence(Some(']'));
        Some(inner.trim().to_string())
    }

    fn command(&mut self) -> String {
        let Some(first) = self.peek() else {
            return String::from("\\");
        };
        if !first.is_ascii_alphabetic() {
            self.pos += 1;
            return match first {
                '{' => "{".into(),
                '}' => "}".into(),
                '%' => "%".into(),
                '&' => "&".into(),
                '_' => "_".into(),
                '#' => "#".into(),
                '$' => "$".into(),
                '\\' => " ".into(),
                ',' | ';' | ':' | ' ' => " ".into(),
                '!' => String::new(),
                '|' => "‖".into(),
                other => other.to_string(),
            };
        }
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                name.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        self.render_command(&name)
    }

    fn render_command(&mut self, name: &str) -> String {
        match name {
            "frac" | "dfrac" | "tfrac" => {
                let num = self.group();
                let den = self.group();
                format!("{}/{}", paren_if_needed(&num), paren_if_needed(&den))
            }
            "sqrt" => {
                let index = self.optional_group();
                let body = self.group();
                match index.as_deref() {
                    None | Some("") => format!("√({body})"),
                    Some("3") => format!("∛({body})"),
                    Some("4") => format!("∜({body})"),
                    Some(n) => format!("{}({body})", super_script(n)),
                }
            }
            "text" | "mathrm" | "mathbf" | "mathit" | "mathsf" | "mathtt" | "operatorname"
            | "mbox" => self.group(),
            "mathbb" => {
                let body = self.group();
                body.chars()
                    .map(|c| match c {
                        'R' => 'ℝ',
                        'N' => 'ℕ',
                        'Z' => 'ℤ',
                        'Q' => 'ℚ',
                        'C' => 'ℂ',
                        'H' => 'ℍ',
                        'P' => 'ℙ',
                        other => other,
                    })
                    .collect()
            }
            "mathcal" => {
                let body = self.group();
                body.chars()
                    .map(|c| match c {
                        'L' => 'ℒ',
                        'M' => 'ℳ',
                        'B' => 'ℬ',
                        'E' => 'ℰ',
                        'F' => 'ℱ',
                        'H' => 'ℋ',
                        'I' => 'ℐ',
                        'R' => 'ℛ',
                        other => other,
                    })
                    .collect()
            }
            "left" | "right" | "middle" => self.delimiter(),
            "hat" | "widehat" => self.accent('\u{0302}'),
            "bar" | "overline" => self.accent('\u{0304}'),
            "tilde" | "widetilde" => self.accent('\u{0303}'),
            "dot" => self.accent('\u{0307}'),
            "ddot" => self.accent('\u{0308}'),
            "vec" => self.accent('\u{20d7}'),
            "check" => self.accent('\u{030c}'),
            "breve" => self.accent('\u{0306}'),
            "underline" => self.accent('\u{0332}'),
            "begin" => self.environment(),
            "end" => {
                let _ = self.group();
                String::new()
            }
            "quad" | "qquad" | "enspace" | "thinspace" | "hspace" => {
                let _ = self.group();
                " ".into()
            }
            "limits" | "nolimits" | "displaystyle" | "textstyle" | "scriptstyle" => String::new(),
            "not" => "¬".into(),
            "pmod" => format!(" (mod {})", self.group()),
            "binom" => {
                let n = self.group();
                let k = self.group();
                format!("C({n},{k})")
            }
            "overbrace" | "underbrace" => self.group(),
            _ => symbol(name)
                .map(str::to_string)
                .unwrap_or_else(|| name.to_string()),
        }
    }

    fn accent(&mut self, mark: char) -> String {
        let body = self.group();
        let mut out = String::new();
        let mut chars = body.chars().peekable();
        while let Some(c) = chars.next() {
            out.push(c);
            if chars.peek().is_none() {
                out.push(mark);
            }
        }
        out
    }

    fn delimiter(&mut self) -> String {
        self.skip_spaces();
        match self.peek() {
            Some('\\') => {
                self.pos += 1;
                self.command()
            }
            Some('<') => {
                self.pos += 1;
                "⟨".into()
            }
            Some('>') => {
                self.pos += 1;
                "⟩".into()
            }
            Some(c) => {
                self.pos += 1;
                c.to_string()
            }
            None => String::new(),
        }
    }

    /// Render simple matrix-like environments as bracketed rows.
    fn environment(&mut self) -> String {
        let env = self.group();
        let matrix = matches!(
            env.as_str(),
            "matrix" | "pmatrix" | "bmatrix" | "cases" | "aligned" | "array" | "smallmatrix"
        );
        let mut raw = String::new();
        let marker: String = format!("end{{{env}}}");
        while self.pos < self.chars.len() {
            if self.peek() == Some('\\') {
                self.pos += 1;
                let mut word = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphabetic() || c == '{' || c == '}' {
                        word.push(c);
                        self.pos += 1;
                    } else if c == '\\' && word.is_empty() {
                        // `\\` row separator
                        word.push('\\');
                        self.pos += 1;
                        break;
                    } else {
                        break;
                    }
                }
                if word == marker {
                    if matrix {
                        let body = raw
                            .replace('&', " , ")
                            .replace("\\\\", " ; ")
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        let body = body.trim_end_matches(';').trim().to_string();
                        return match env.as_str() {
                            "pmatrix" => format!("[{body}]"),
                            "bmatrix" => format!("[{body}]"),
                            "cases" => format!("{{{body}}}"),
                            _ => body,
                        };
                    }
                    return raw.trim().to_string();
                }
                raw.push('\\');
                raw.push_str(&word);
            } else if let Some(c) = self.bump() {
                raw.push(c);
            }
        }
        raw.trim().to_string()
    }
}

fn paren_if_needed(s: &str) -> String {
    if s.chars().count() <= 1 {
        s.to_string()
    } else {
        format!("({s})")
    }
}

fn super_script(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match superscript_char(c) {
            Some(mapped) => out.push(mapped),
            None => return format!("^({s})"),
        }
    }
    out
}

fn sub_script(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match subscript_char(c) {
            Some(mapped) => out.push(mapped),
            None => return format!("_({s})"),
        }
    }
    out
}

fn superscript_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        'A' => 'ᴬ',
        'B' => 'ᴮ',
        'D' => 'ᴰ',
        'E' => 'ᴱ',
        'G' => 'ᴳ',
        'H' => 'ᴴ',
        'I' => 'ᴵ',
        'J' => 'ᴶ',
        'K' => 'ᴷ',
        'L' => 'ᴸ',
        'M' => 'ᴹ',
        'N' => 'ᴺ',
        'O' => 'ᴼ',
        'P' => 'ᴾ',
        'R' => 'ᴿ',
        'T' => 'ᵀ',
        'U' => 'ᵁ',
        'V' => 'ⱽ',
        'W' => 'ᵂ',
        'α' => 'ᵅ',
        'β' => 'ᵝ',
        'γ' => 'ᵞ',
        'δ' => 'ᵟ',
        'φ' => 'ᵠ',
        'χ' => 'ᵡ',
        _ => return None,
    })
}

fn subscript_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        'β' => 'ᵦ',
        'γ' => 'ᵧ',
        'ρ' => 'ᵨ',
        'φ' => 'ᵩ',
        'χ' => 'ᵪ',
        _ => return None,
    })
}

fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        // Function names render as themselves.
        "sin" | "cos" | "tan" | "cot" | "sec" | "csc" | "log" | "ln" | "exp" | "lim" | "min"
        | "max" | "arg" | "det" | "dim" | "ker" | "deg" | "gcd" | "hom" | "sup" | "inf" => {
            return None;
        }
        // Greek
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ε",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "varrho" => "ϱ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "φ",
        "varphi" => "ϕ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        // Binary operators
        "times" => "×",
        "div" => "÷",
        "cdot" => "·",
        "pm" => "±",
        "mp" => "∓",
        "oplus" => "⊕",
        "otimes" => "⊗",
        "odot" => "⊙",
        "circ" => "∘",
        "bullet" => "•",
        "star" => "⋆",
        "ast" => "∗",
        "cup" => "∪",
        "cap" => "∩",
        "setminus" => "∖",
        "wedge" => "∧",
        "vee" => "∨",
        "land" => "∧",
        "lor" => "∨",
        "dagger" => "†",
        "ddagger" => "‡",
        // Relations
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "equiv" => "≡",
        "approx" => "≈",
        "sim" => "∼",
        "simeq" => "≃",
        "cong" => "≅",
        "propto" => "∝",
        "ll" => "≪",
        "gg" => "≫",
        "prec" => "≺",
        "succ" => "≻",
        "perp" => "⊥",
        "parallel" => "∥",
        "mid" => "∣",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "supset" => "⊃",
        "supseteq" => "⊇",
        "emptyset" => "∅",
        "varnothing" => "∅",
        "doteq" => "≐",
        "asymp" => "≍",
        // Arrows
        "to" | "rightarrow" => "→",
        "gets" | "leftarrow" => "←",
        "Rightarrow" | "implies" => "⇒",
        "Leftarrow" => "⇐",
        "leftrightarrow" => "↔",
        "Leftrightarrow" | "iff" => "⇔",
        "mapsto" => "↦",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "updownarrow" => "↕",
        "longrightarrow" => "⟶",
        "longleftarrow" => "⟵",
        "Longrightarrow" => "⟹",
        "hookrightarrow" => "↪",
        // Big operators
        "sum" => "∑",
        "prod" => "∏",
        "coprod" => "∐",
        "int" => "∫",
        "iint" => "∬",
        "iiint" => "∭",
        "oint" => "∮",
        // Misc
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "forall" => "∀",
        "exists" => "∃",
        "nexists" => "∄",
        "neg" | "lnot" => "¬",
        "angle" => "∠",
        "triangle" => "△",
        "square" => "□",
        "diamond" => "◇",
        "dots" | "ldots" => "…",
        "cdots" => "⋯",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "hbar" => "ℏ",
        "ell" => "ℓ",
        "Re" => "ℜ",
        "Im" => "ℑ",
        "aleph" => "ℵ",
        "wp" => "℘",
        "prime" => "′",
        "degree" => "°",
        "checkmark" => "✓",
        "top" => "⊤",
        "bot" => "⊥",
        "vdash" => "⊢",
        "dashv" => "⊣",
        "models" => "⊨",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "|" => "‖",
        "quad" => " ",
        "qquad" => "  ",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_greek_and_relations() {
        assert_eq!(render(r"\alpha \le \beta \ge \gamma"), "α ≤ β ≥ γ");
    }

    #[test]
    fn renders_superscripts_and_subscripts() {
        assert_eq!(render("x^2"), "x²");
        assert_eq!(render("x^{10}"), "x¹⁰");
        assert_eq!(render("a_i"), "aᵢ");
        assert_eq!(render("x^2_i"), "x²ᵢ");
    }

    #[test]
    fn renders_fractions_and_roots() {
        assert_eq!(render(r"\frac{a}{b}"), "a/b");
        assert_eq!(render(r"\frac{x+1}{y}"), "(x+1)/y");
        assert_eq!(render(r"\sqrt{2}"), "√(2)");
        assert_eq!(render(r"\sqrt[3]{x}"), "∛(x)");
    }

    #[test]
    fn renders_sums_and_integrals() {
        assert_eq!(render(r"\sum_{i=1}^{n} i"), "∑ᵢ₌₁ⁿ i");
        assert_eq!(render(r"\int_0^1 x^2 dx"), "∫₀¹ x² dx");
    }

    #[test]
    fn renders_blackboard_bold() {
        assert_eq!(render(r"\mathbb{R}^n"), "ℝⁿ");
    }

    #[test]
    fn unknown_commands_degrade_to_name() {
        assert_eq!(render(r"\weirdcommand x"), "weirdcommand x");
    }

    #[test]
    fn function_names_render_as_plain_text() {
        assert_eq!(render(r"\sin x"), "sin x");
    }

    #[test]
    fn matrices_render_on_one_line() {
        assert_eq!(
            render(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"),
            "[a , b ; c , d]"
        );
    }

    #[test]
    fn accents_combine() {
        assert_eq!(render(r"\hat{x}"), "x\u{0302}");
    }
}
